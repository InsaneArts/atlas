//! Pi references the bundled plugin in place. Only Pi writes its settings;
//! Atlas reads package sources and delegates changes to Pi's locked writer.

use super::{
    Agent, CHANGE, Removal, Source, atlas_folder, atlas_plugin, exec_in, problem, same_place,
};
use crate::error::Result;
use serde_json::Value;
use std::{
    fs,
    path::{Component, Path, PathBuf},
};

pub(super) fn home() -> Option<PathBuf> {
    let person = std::env::var_os("HOME").map(PathBuf::from);
    let configured = std::env::var_os("PI_CODING_AGENT_DIR").filter(|s| !s.is_empty());
    let path = match configured {
        Some(value) => expand_home(Path::new(&value), person.as_deref())?,
        None => person?.join(".pi/agent"),
    };
    if path.is_absolute() {
        Some(normalize(&path))
    } else {
        Some(normalize(&std::env::current_dir().ok()?.join(path)))
    }
}

fn expand_home(path: &Path, person: Option<&Path>) -> Option<PathBuf> {
    if path == Path::new("~") {
        return person.map(Path::to_path_buf);
    }
    if let Ok(tail) = path.strip_prefix("~/") {
        return person.map(|home| home.join(tail));
    }
    Some(path.to_path_buf())
}

/// Collapse dot components even when a moved bundle no longer exists.
pub(super) fn normalize(path: &Path) -> PathBuf {
    let mut clean = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                clean.pop();
            }
            other => clean.push(other.as_os_str()),
        }
    }
    clean
}

#[derive(Debug)]
struct Package {
    stored: String,
    folder: PathBuf,
}

fn packages(home: &Path) -> Result<Vec<Package>> {
    let path = home.join("settings.json");
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
        Err(error) => {
            return Err(problem(format!(
                "Could not read {}: {error}",
                path.display()
            )));
        }
    };
    packages_in(&text, home).map_err(|error| {
        problem(format!(
            "Could not read {}: {}",
            path.display(),
            error.error
        ))
    })
}

fn packages_in(text: &str, home: &Path) -> Result<Vec<Package>> {
    let value: Value = serde_json::from_str(text.trim_start_matches('\u{feff}'))
        .map_err(|error| problem(error.to_string()))?;
    let object = value
        .as_object()
        .ok_or_else(|| problem("Expected a settings object"))?;
    let Some(entries) = object.get("packages") else {
        return Ok(vec![]);
    };
    let entries = entries
        .as_array()
        .ok_or_else(|| problem("Expected a packages array"))?;
    let person = std::env::var_os("HOME").map(PathBuf::from);
    let mut found = Vec::new();
    for entry in entries {
        let stored = entry
            .as_str()
            .or_else(|| entry.get("source").and_then(Value::as_str))
            .filter(|source| !source.is_empty())
            .ok_or_else(|| problem("Expected a package source"))?;
        let Some(path) = local_path(stored, home, person.as_deref()) else {
            continue;
        };
        // Only Atlas candidates matter; never remove another installed package.
        if atlas_plugin(&path) || path.file_name().is_some_and(|name| name == "atlas") {
            found.push(Package {
                stored: stored.to_owned(),
                folder: path,
            });
        }
    }
    Ok(found)
}

fn local_path(source: &str, home: &Path, person: Option<&Path>) -> Option<PathBuf> {
    let source = if let Some(file) = source.strip_prefix("file://") {
        // Local file URLs only. Percent escapes are decoded without treating '+' as a space.
        let file = file.strip_prefix("localhost/").map_or(file, |tail| tail);
        if !source.starts_with("file:///") && !source.starts_with("file://localhost/") {
            return None;
        }
        let file = if source.starts_with("file://localhost/") {
            format!("/{file}")
        } else {
            file.to_owned()
        };
        decode_file_path(&file)?
    } else {
        if source.contains(':') || source.starts_with("git@") {
            return None;
        }
        source.to_owned()
    };
    let path = expand_home(Path::new(&source), person)?;
    Some(normalize(&if path.is_absolute() {
        path
    } else {
        home.join(path)
    }))
}

fn decode_file_path(source: &str) -> Option<String> {
    let bytes = source.as_bytes();
    let mut decoded = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let digits = std::str::from_utf8(bytes.get(i + 1..i + 3)?).ok()?;
            decoded.push(u8::from_str_radix(digits, 16).ok()?);
            i += 3;
        } else {
            decoded.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(decoded)
        .ok()
        .filter(|s| !s.contains('\0'))
}

fn owned(package: &Package) -> bool {
    atlas_plugin(&package.folder)
        || package
            .folder
            .file_name()
            .is_some_and(|name| name == "atlas")
            && package.folder.parent().is_some_and(atlas_folder)
}

fn source_of(packages: &[Package]) -> Source {
    // An unrelated Atlas-named package wins over ours, preventing silent replacement.
    if let Some(package) = packages.iter().find(|package| !owned(package)) {
        return Source::Elsewhere(package.folder.display().to_string());
    }
    let current = packages
        .iter()
        .find(|package| package.folder.parent().is_some_and(super::this_atlas))
        .or_else(|| packages.first());
    current.map_or(Source::Absent, |package| {
        Source::Folder(
            package
                .folder
                .parent()
                .unwrap_or(&package.folder)
                .to_path_buf(),
        )
    })
}

pub(super) fn source() -> Result<Source> {
    let home = home().ok_or_else(|| problem("Pi's config folder could not be found"))?;
    Ok(source_of(&packages(&home)?))
}

pub(super) fn change(marketplace: Option<&Path>) -> Result<Removal> {
    let _lock = super::connection_lock()?;
    let home = home().ok_or_else(|| problem("Pi's config folder could not be found"))?;
    let current = packages(&home)?;
    if let Source::Elsewhere(place) = source_of(&current) {
        return if marketplace.is_some() {
            Err(problem(format!(
                "Pi has another package named atlas, from {place}. Remove it in Pi before connecting Atlas"
            )))
        } else {
            remove_owned(&home, &current)?;
            Ok(Removal::Kept(place))
        };
    }
    let Some(marketplace) = marketplace else {
        if current.is_empty() {
            return Ok(Removal::Nothing);
        }
        remove_owned(&home, &current)?;
        return Ok(Removal::Changed);
    };
    let plugin = fs::canonicalize(marketplace.join("atlas"))
        .map_err(|error| problem(format!("Could not find Atlas's Pi package: {error}")))?;
    // Preserve resource selections on a current entry; Pi's writer updates its spelling in place.
    let connected = current
        .iter()
        .any(|package| same_place(&package.folder, &plugin));
    let mut changed = false;
    for package in &current {
        if !same_place(&package.folder, &plugin)
            && packages(&home)?
                .iter()
                .any(|entry| entry.stored == package.stored)
        {
            mutate(&home, "remove", &package.stored)?;
            changed = true;
        }
    }
    if connected {
        return Ok(if changed {
            Removal::Changed
        } else {
            Removal::Nothing
        });
    }
    fs::create_dir_all(&home)
        .map_err(|error| problem(format!("Could not create {}: {error}", home.display())))?;
    mutate(&home, "install", &plugin.to_string_lossy())?;
    let connected = packages(&home)?
        .iter()
        .any(|package| same_place(&package.folder, &plugin));
    if !connected {
        return Err(problem("Pi did not keep the Atlas package"));
    }
    Ok(Removal::Changed)
}

fn remove_owned(home: &Path, packages: &[Package]) -> Result<()> {
    for package in packages.iter().filter(|package| owned(package)) {
        // A prior exact removal may also have removed duplicate entries.
        if self::packages(home)?
            .iter()
            .any(|entry| entry.stored == package.stored)
        {
            mutate(home, "remove", &package.stored)?;
        }
    }
    if self::packages(home)?.iter().any(owned) {
        return Err(problem("Pi still lists the Atlas package"));
    }
    Ok(())
}

fn mutate(home: &Path, verb: &str, source: &str) -> Result<()> {
    let output = exec_in(
        Agent::Pi,
        &[verb, source, "--no-approve"],
        CHANGE,
        Some(home),
    )?;
    if output.status.success() {
        return Ok(());
    }
    Err(problem(format!(
        "pi {verb} failed: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn pi_resolves_local_sources_without_confusing_remote_packages() {
        let home = Path::new("/Users/person/.pi/agent");
        let person = Some(Path::new("/Users/person"));
        for (source, path) in [
            (
                "../../../Applications/Atlas.app/Contents/Resources/agents/atlas",
                "/Users/Applications/Atlas.app/Contents/Resources/agents/atlas",
            ),
            ("~/Applications/atlas", "/Users/person/Applications/atlas"),
            (
                "file:///Applications/Atlas%20Test.app/Contents/Resources/agents/atlas",
                "/Applications/Atlas Test.app/Contents/Resources/agents/atlas",
            ),
            ("file://localhost/Applications/atlas", "/Applications/atlas"),
        ] {
            assert_eq!(
                local_path(source, home, person),
                Some(PathBuf::from(path)),
                "{source}"
            );
        }
        for source in [
            "npm:atlas",
            "git:atlas",
            "github:a/atlas",
            "https://host/atlas",
            "ssh://host/atlas",
            "builtin:atlas",
            "git@host:atlas",
            "file://host/atlas",
        ] {
            assert_eq!(local_path(source, home, person), None, "{source}");
        }
    }

    #[test]
    fn pi_reads_object_packages_and_preserves_their_exact_removal_source() {
        let home = Path::new("/config");
        let value = json!({"packages": ["npm:elsewhere", {"source": "../old/atlas", "extensions": []}, "../other"]});
        let entries = packages_in(&value.to_string(), home).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].stored, "../old/atlas");
        assert_eq!(entries[0].folder, Path::new("/old/atlas"));
        assert_eq!(source_of(&entries), Source::Elsewhere("/old/atlas".into()));
    }

    #[test]
    fn pi_distinguishes_old_atlas_bundles_from_unrelated_packages() {
        let value = json!({"packages": ["/missing/Atlas.app/Contents/Resources/agents/atlas", "/someone/atlas"]});
        let entries = packages_in(&value.to_string(), Path::new("/config")).unwrap();
        assert!(owned(&entries[0]));
        assert!(!owned(&entries[1]));
        assert_eq!(
            source_of(&entries),
            Source::Elsewhere("/someone/atlas".into())
        );
        assert_eq!(
            source_of(&entries[..1]),
            Source::Folder("/missing/Atlas.app/Contents/Resources/agents".into())
        );
    }

    #[test]
    fn pi_refuses_malformed_settings_and_treats_absence_as_empty() {
        for text in [
            "not json",
            "[]",
            r#"{"packages":false}"#,
            r#"{"packages":[{}]}"#,
        ] {
            assert!(packages_in(text, Path::new("/config")).is_err(), "{text}");
        }
        assert!(
            packages_in("\u{feff}{}", Path::new("/config"))
                .unwrap()
                .is_empty()
        );
        let temp = tempfile::tempdir().unwrap();
        assert!(packages(temp.path()).unwrap().is_empty());
        fs::write(temp.path().join("settings.json"), "{").unwrap();
        assert!(
            packages(temp.path())
                .unwrap_err()
                .error
                .contains("settings.json")
        );
    }
}

//! opencode loads one symlinked server plugin. Its JSON/JSONC configuration
//! stays the person's; the plugin contributes only in-memory defaults.
use super::{Removal, Source, atlas_folder, connection_lock, pi::normalize, problem, same_place};
use crate::error::Result;
use std::{
    fs,
    path::{Path, PathBuf},
};

pub(super) fn home() -> Option<PathBuf> {
    std::env::var_os("XDG_CONFIG_HOME")
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .map(|home| home.join("opencode"))
}

fn link() -> Result<PathBuf> {
    Ok(home()
        .ok_or_else(|| problem("opencode's config folder could not be found"))?
        .join("plugins/atlas.js"))
}

pub(super) fn source() -> Result<Source> {
    source_at(&link()?)
}

fn source_at(link: &Path) -> Result<Source> {
    let metadata = match fs::symlink_metadata(link) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Source::Absent),
        Err(error) => {
            return Err(problem(format!(
                "Could not read {}: {error}",
                link.display()
            )));
        }
    };
    if !metadata.is_symlink() {
        return Ok(Source::Elsewhere(link.display().to_string()));
    }
    let target = fs::read_link(link)
        .map_err(|error| problem(format!("Could not read {}: {error}", link.display())))?;
    let target = if target.is_absolute() {
        target
    } else {
        link.parent().unwrap_or(Path::new(".")).join(target)
    };
    let target = normalize(&target);
    if target.ends_with("atlas/opencode/atlas.js")
        && let Some(root) = target.ancestors().nth(3)
    {
        return Ok(Source::Folder(root.to_path_buf()));
    }
    Ok(Source::Elsewhere(target.display().to_string()))
}

pub(super) fn installed() -> Result<bool> {
    Ok(link()?.is_file())
}

pub(super) fn change(marketplace: Option<&Path>) -> Result<Removal> {
    let _lock = connection_lock()?;
    if super::stopping() {
        return Err(problem("Atlas is stopping"));
    }
    change_at(&link()?, marketplace)
}

fn change_at(link: &Path, marketplace: Option<&Path>) -> Result<Removal> {
    let source = source_at(link)?;
    match &source {
        Source::Folder(folder) if atlas_folder(folder) => {}
        Source::Absent => {}
        Source::Folder(folder) => {
            return collision(&folder.display().to_string(), marketplace.is_some());
        }
        Source::Elsewhere(place) => return collision(place, marketplace.is_some()),
    }
    let Some(marketplace) = marketplace else {
        if source == Source::Absent {
            return Ok(Removal::Nothing);
        }
        fs::remove_file(link)
            .map_err(|error| problem(format!("Could not remove {}: {error}", link.display())))?;
        return Ok(Removal::Changed);
    };
    let target = fs::canonicalize(marketplace.join("atlas/opencode/atlas.js"))
        .map_err(|error| problem(format!("Could not find Atlas's opencode plugin: {error}")))?;
    if matches!(&source, Source::Folder(folder) if same_place(folder, marketplace))
        && link.is_file()
    {
        return Ok(Removal::Nothing);
    }
    let parent = link
        .parent()
        .ok_or_else(|| problem("The plugin link has no parent folder"))?;
    fs::create_dir_all(parent)
        .map_err(|error| problem(format!("Could not create {}: {error}", parent.display())))?;
    let temporary = parent.join(format!(".atlas-{}.tmp", rand::random::<u64>()));
    std::os::unix::fs::symlink(&target, &temporary).map_err(|error| {
        problem(format!(
            "Could not create the opencode plugin link: {error}"
        ))
    })?;
    let outcome = (|| {
        if source_at(link)? != source {
            return Err(problem(
                "opencode's Atlas plugin changed while connecting; try again",
            ));
        }
        // On first installation use the exclusive create operation, so a newly arrived personal file is kept.
        if source == Source::Absent {
            std::os::unix::fs::symlink(&target, link)
        } else {
            fs::rename(&temporary, link)
        }
        .map_err(|error| problem(format!("Could not connect {}: {error}", link.display())))?;
        Ok(Removal::Changed)
    })();
    let cleanup = fs::remove_file(&temporary);
    if let Err(error) = cleanup
        && error.kind() != std::io::ErrorKind::NotFound
    {
        return Err(problem(format!(
            "Could not clean {}: {error}",
            temporary.display()
        )));
    }
    outcome
}

fn collision(place: &str, connecting: bool) -> Result<Removal> {
    if connecting {
        Err(problem(format!(
            "opencode has another Atlas plugin at {place}. Remove it yourself before connecting Atlas"
        )))
    } else {
        Ok(Removal::Kept(place.to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    #[test]
    fn opencode_preserves_regular_files_and_unrelated_links() {
        let temp = tempfile::tempdir().unwrap();
        let link = temp.path().join("atlas.js");
        fs::write(&link, "personal").unwrap();
        assert!(change_at(&link, Some(temp.path())).is_err());
        assert!(matches!(change_at(&link, None).unwrap(), Removal::Kept(_)));
        assert_eq!(fs::read_to_string(&link).unwrap(), "personal");
        fs::remove_file(&link).unwrap();
        symlink("elsewhere.js", &link).unwrap();
        assert!(change_at(&link, Some(temp.path())).is_err());
        assert!(matches!(change_at(&link, None).unwrap(), Removal::Kept(_)));
        assert_eq!(fs::read_link(&link).unwrap(), Path::new("elsewhere.js"));
    }

    #[test]
    fn opencode_resolves_relative_links_and_removes_a_dangling_old_atlas() {
        let temp = tempfile::tempdir().unwrap();
        let link = temp.path().join("atlas.js");
        symlink(
            "old/Atlas.app/Contents/Resources/agents/atlas/opencode/atlas.js",
            &link,
        )
        .unwrap();
        assert_eq!(
            source_at(&link).unwrap(),
            Source::Folder(temp.path().join("old/Atlas.app/Contents/Resources/agents"))
        );
        assert_eq!(change_at(&link, None).unwrap(), Removal::Changed);
        assert_eq!(change_at(&link, None).unwrap(), Removal::Nothing);
    }

    #[test]
    fn opencode_installs_atomically_and_replaces_only_its_own_old_link() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("agents");
        let plugin = root.join("atlas/opencode/atlas.js");
        fs::create_dir_all(plugin.parent().unwrap()).unwrap();
        fs::write(&plugin, "export default {}").unwrap();
        fs::create_dir_all(root.join("atlas/.codex-plugin")).unwrap();
        fs::write(
            root.join("atlas/.codex-plugin/plugin.json"),
            serde_json::json!({"name":"atlas", "homepage":super::super::HOMEPAGE}).to_string(),
        )
        .unwrap();
        let link = temp.path().join("config/plugins/atlas.js");
        assert_eq!(change_at(&link, Some(&root)).unwrap(), Removal::Changed);
        assert!(same_place(&fs::read_link(&link).unwrap(), &plugin));
        assert_eq!(change_at(&link, Some(&root)).unwrap(), Removal::Nothing);
        fs::remove_file(&link).unwrap();
        symlink(
            "/missing/Atlas.app/Contents/Resources/agents/atlas/opencode/atlas.js",
            &link,
        )
        .unwrap();
        assert_eq!(change_at(&link, Some(&root)).unwrap(), Removal::Changed);
        assert_eq!(fs::read_dir(link.parent().unwrap()).unwrap().count(), 1);
    }
}

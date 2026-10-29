//! The Claude app (Claude for Mac): Atlas's MCP server in its settings, so
//! its chats can show the helper's screen with Stop. The app has no plugin
//! command line, so Atlas edits the one entry it owns in the app's own
//! settings file, keeping every other setting and a copy of the file as it
//! was; the app reads it when it next starts.
use crate::error::{Code, Error, Result};
use serde_json::{Map, Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
};

/// The entry's name in the app's `mcpServers`.
const NAME: &str = "atlas";
/// The app's id in `atlas agents` results.
pub const ID: &str = "claude-app";

/// Where the app keeps its settings, under `home`.
fn settings_in(home: &Path) -> PathBuf {
    home.join("Library/Application Support/Claude/claude_desktop_config.json")
}

fn home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

/// Whether the app is on this Mac: installed, or its settings folder is.
pub fn installed() -> bool {
    Path::new("/Applications/Claude.app").exists()
        || home().is_some_and(|home| home.join("Applications/Claude.app").exists())
        || home().is_some_and(|home| settings_in(&home).parent().is_some_and(Path::is_dir))
}

/// The entry that runs `atlas` as the app's MCP server.
fn entry(atlas: &Path) -> Value {
    json!({"command": atlas, "args": ["mcp"]})
}

/// Whether `found`, the app's entry named atlas, is one Atlas wrote: it
/// runs an `atlas` executable's MCP server.
fn ours(found: &Value) -> bool {
    found["args"] == json!(["mcp"])
        && found["command"]
            .as_str()
            .and_then(|command| Path::new(command).file_name())
            .is_some_and(|name| name == "atlas")
}

/// The app's settings in `text`, a JSON object; an empty object when there
/// are none yet.
fn parse(text: Option<&str>) -> Result<Map<String, Value>> {
    let Some(text) = text.filter(|text| !text.trim().is_empty()) else {
        return Ok(Map::new());
    };

    match serde_json::from_str::<Value>(text) {
        Ok(Value::Object(settings)) => Ok(settings),
        _ => Err(Error::new(
            Code::Agents,
            "The Claude app's settings file is not valid JSON; fix it or move it aside, then try again",
        )),
    }
}

/// `settings` with Atlas's entry for `atlas` (`Some`) or without it (`None`);
/// whether that changed them. Another program's entry named atlas is
/// refused, never replaced.
fn changed(settings: &mut Map<String, Value>, atlas: Option<&Path>) -> Result<bool> {
    let servers = settings
        .entry("mcpServers")
        .or_insert_with(|| Value::Object(Map::new()));
    let Some(servers) = servers.as_object_mut() else {
        return Err(Error::new(
            Code::Agents,
            "The Claude app's mcpServers setting is not an object; fix it, then try again",
        ));
    };

    if let Some(found) = servers.get(NAME)
        && !ours(found)
    {
        if atlas.is_none() {
            return Ok(false);
        }
        return Err(Error::new(
            Code::Agents,
            "The Claude app already has an MCP server named atlas that is not Atlas's; remove it \
             in the app's settings, then try again",
        ));
    }

    Ok(match atlas {
        Some(atlas) => servers.insert(NAME.into(), entry(atlas)).as_ref() != Some(&entry(atlas)),
        None => servers.remove(NAME).is_some(),
    })
}

/// Whether the app's settings file in `home` runs Atlas's MCP server.
fn connected_in(home: &Path) -> bool {
    fs::read_to_string(settings_in(home))
        .ok()
        .and_then(|text| parse(Some(&text)).ok())
        .and_then(|settings| settings.get("mcpServers").cloned())
        .and_then(|servers| servers.get(NAME).cloned())
        .is_some_and(|found| ours(&found))
}

/// Add Atlas's MCP server (`atlas`, the executable it runs) to the app's
/// settings in `home`, or remove it (`None`): whether the file changed.
///
/// The file before Atlas first changed it is kept beside it, privately, and
/// never replaced. The new file replaces the old one whole, with its
/// permissions; a settings file that is a link is changed where it points.
/// Removing leaves another program's entry named atlas alone; invalid
/// settings are reported without changing the file. Registrations share
/// the connection lock in `locks`; tests use a directory of their own.
fn change_in(home: &Path, atlas: Option<&Path>, locks: &Path) -> Result<bool> {
    let _lock = crate::agents::connection_lock_in(locks)?;
    let named = settings_in(home);
    let (path, before) = read_settings(&named)?;
    let changing = parse(before.as_deref()).and_then(|mut settings| {
        changed(&mut settings, atlas).map(|changed| changed.then_some(settings))
    });
    let settings = match changing {
        Ok(Some(settings)) => settings,
        Ok(None) => return Ok(false),
        Err(error) => {
            return Err(Error::new(
                error.code,
                format!("{}: {}", named.display(), error.error),
            ));
        }
    };

    write_settings(home, &path, before.as_deref(), settings)?;
    Ok(true)
}

/// The resolved settings file and its original text, or its absent path.
fn read_settings(named: &Path) -> Result<(PathBuf, Option<String>)> {
    let path = match fs::symlink_metadata(named) {
        Ok(_) => fs::canonicalize(named).map_err(|error| {
            Error::new(
                Code::Agents,
                format!("Could not resolve {}: {error}", named.display()),
            )
        })?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => named.to_path_buf(),
        Err(error) => return Err(Error::writing(named, error)),
    };
    let before = match fs::read_to_string(&path) {
        Ok(text) => Some(text),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(Error::writing(&path, error)),
    };

    Ok((path, before))
}

/// Publish changed settings, keeping the first original privately beside them.
fn write_settings(
    home: &Path,
    path: &Path,
    before: Option<&str>,
    settings: Map<String, Value>,
) -> Result<()> {
    use std::{io::Write, os::unix::fs::PermissionsExt};

    let folder = path.parent().unwrap_or(home);
    fs::create_dir_all(folder).map_err(|error| Error::writing(folder, error))?;
    let kept = path.with_extension("json.before-atlas");
    if let Some(before) = before
        && !kept.exists()
    {
        crate::files::write_private(&kept, before.as_bytes())?;
    }

    let mode = fs::metadata(path).map_or(0o600, |metadata| metadata.permissions().mode() & 0o777);
    let mut text = serde_json::to_string_pretty(&Value::Object(settings))?;
    text.push('\n');
    crate::config::write_beside(
        path,
        0o600,
        |file| {
            file.write_all(text.as_bytes())?;
            file.set_permissions(fs::Permissions::from_mode(mode))?;
            Ok(())
        },
        |temporary| {
            let (current, text) = read_settings(&settings_in(home))?;
            if current != path || text.as_deref() != before {
                return Err(Error::new(
                    Code::Agents,
                    "The Claude app's settings changed while connecting; try again",
                ));
            }
            fs::rename(temporary, path).map_err(|error| Error::writing(path, error))
        },
    )
}

/// The executable the app should run: the one inside the installed
/// Atlas.app, which updates keep current, else this one.
fn executable() -> Result<PathBuf> {
    if let Ok(app) = crate::update::installed_app() {
        let inside = app.join("Contents/MacOS/atlas");
        if inside.is_file() {
            return Ok(inside);
        }
    }

    Ok(std::env::current_exe()?)
}

/// The app's row in `atlas agents`.
pub fn status() -> Value {
    let installed = installed();
    json!({
        "agent": ID,
        "installed": installed,
        "connected": installed && home().is_some_and(|home| connected_in(&home)),
        "version": Value::Null,
    })
}

/// Connect the app (`connect`) or disconnect it: its row in the results.
pub fn change(connect: bool) -> Value {
    let outcome = home()
        .ok_or_else(|| Error::new(Code::Agents, "HOME is not set"))
        .and_then(|home| {
            let atlas = if connect { Some(executable()?) } else { None };
            change_in(&home, atlas.as_deref(), &std::env::temp_dir())
        });

    match outcome {
        Ok(changed) => json!({
            "agent": ID,
            "ok": true,
            "connected": connect,
            "changed": changed,
            // The app reads its MCP servers when it starts.
            "restart": changed,
        }),
        Err(error) => json!({"agent": ID, "ok": false, "error": error.error}),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn change_in(home: &Path, atlas: Option<&Path>) -> Result<bool> {
        super::change_in(home, atlas, home)
    }

    fn written(home: &Path) -> Value {
        serde_json::from_str(&fs::read_to_string(settings_in(home)).unwrap()).unwrap()
    }

    fn mode(path: &Path) -> u32 {
        fs::metadata(path).unwrap().permissions().mode() & 0o777
    }

    #[test]
    fn settings_edited_after_reading_are_not_replaced() {
        for edited in [
            r#"{"theme":"light"}"#,
            r#"{"mcpServers":{"atlas":{"command":"someone-else"}}}"#,
        ] {
            let dir = tempfile::tempdir().unwrap();
            let named = settings_in(dir.path());
            fs::create_dir_all(named.parent().unwrap()).unwrap();
            fs::write(&named, "{}").unwrap();
            let (path, before) = read_settings(&named).unwrap();
            let mut settings = parse(before.as_deref()).unwrap();
            changed(&mut settings, Some(Path::new("/x/atlas"))).unwrap();

            fs::write(&named, edited).unwrap();
            let result = write_settings(dir.path(), &path, before.as_deref(), settings);

            assert!(result.unwrap_err().error.contains("settings changed"));
            assert_eq!(fs::read_to_string(&named).unwrap(), edited);
        }
    }

    #[test]
    fn a_retargeted_settings_link_keeps_both_files_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        let named = settings_in(dir.path());
        fs::create_dir_all(named.parent().unwrap()).unwrap();
        let original = dir.path().join("original.json");
        let replacement = dir.path().join("replacement.json");
        for path in [&original, &replacement] {
            fs::write(path, "{}").unwrap();
        }
        std::os::unix::fs::symlink(&original, &named).unwrap();
        let (path, before) = read_settings(&named).unwrap();
        let mut settings = parse(before.as_deref()).unwrap();
        changed(&mut settings, Some(Path::new("/x/atlas"))).unwrap();

        fs::remove_file(&named).unwrap();
        std::os::unix::fs::symlink(&replacement, &named).unwrap();
        let result = write_settings(dir.path(), &path, before.as_deref(), settings);

        assert!(result.unwrap_err().error.contains("settings changed"));
        assert_eq!(fs::read_link(&named).unwrap(), replacement);
        for path in [&original, &replacement] {
            assert_eq!(fs::read_to_string(path).unwrap(), "{}");
        }
    }

    #[test]
    fn settings_created_after_reading_are_not_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let named = settings_in(dir.path());
        let (path, before) = read_settings(&named).unwrap();
        let mut settings = parse(before.as_deref()).unwrap();
        changed(&mut settings, Some(Path::new("/x/atlas"))).unwrap();

        fs::create_dir_all(named.parent().unwrap()).unwrap();
        let edited = r#"{"theme":"light"}"#;
        fs::write(&named, edited).unwrap();
        let result = write_settings(dir.path(), &path, before.as_deref(), settings);

        assert!(result.unwrap_err().error.contains("settings changed"));
        assert_eq!(fs::read_to_string(&named).unwrap(), edited);
    }

    #[test]
    fn another_atlas_registration_keeps_settings_unchanged_until_it_finishes() {
        let dir = tempfile::tempdir().unwrap();
        let named = settings_in(dir.path());
        fs::create_dir_all(named.parent().unwrap()).unwrap();
        fs::write(&named, "{}").unwrap();
        let atlas = Path::new("/x/atlas");
        let held = crate::agents::connection_lock_in(dir.path()).unwrap();

        let refused = std::thread::scope(|scope| {
            scope
                .spawn(|| change_in(dir.path(), Some(atlas)))
                .join()
                .unwrap()
        });
        assert!(
            refused
                .unwrap_err()
                .error
                .contains("Another agent connection")
        );
        assert_eq!(fs::read_to_string(&named).unwrap(), "{}");
        assert!(!named.with_extension("json.before-atlas").exists());

        drop(held);
        assert!(change_in(dir.path(), Some(atlas)).unwrap());
        assert!(connected_in(dir.path()));
    }

    #[test]
    fn connecting_adds_atlas_and_keeps_every_other_setting_and_the_file_as_it_was() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path();
        let path = settings_in(home);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let before = r#"{"mcpServers": {"other": {"command": "x"}}, "theme": "dark"}"#;
        fs::write(&path, before).unwrap();
        let atlas = Path::new("/Applications/Atlas.app/Contents/MacOS/atlas");

        assert!(change_in(home, Some(atlas)).unwrap());
        let settings = written(home);
        assert_eq!(settings["theme"], "dark");
        assert_eq!(settings["mcpServers"]["other"]["command"], "x");
        assert_eq!(settings["mcpServers"]["atlas"], entry(atlas));
        assert!(connected_in(home));
        let kept = path.with_extension("json.before-atlas");
        assert_eq!(fs::read_to_string(&kept).unwrap(), before);

        assert!(
            !change_in(home, Some(atlas)).unwrap(),
            "nothing to change twice"
        );
        assert!(change_in(home, None).unwrap());
        assert!(!connected_in(home));
        assert_eq!(written(home)["mcpServers"]["other"]["command"], "x");
        assert!(!change_in(home, None).unwrap());
        assert_eq!(
            fs::read_to_string(&kept).unwrap(),
            before,
            "the copy is never replaced"
        );
    }

    #[test]
    fn a_missing_settings_file_is_made_privately() {
        let dir = tempfile::tempdir().unwrap();
        assert!(change_in(dir.path(), Some(Path::new("/usr/local/bin/atlas"))).unwrap());
        assert!(connected_in(dir.path()));
        let path = settings_in(dir.path());
        assert!(!path.with_extension("json.before-atlas").exists());
        assert_eq!(mode(&path), 0o600);
    }

    #[test]
    fn the_file_keeps_its_permissions_and_its_copy_is_private() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path();
        let path = settings_in(home);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(
            &path,
            r#"{"mcpServers": {"tokens": {"env": {"KEY": "secret"}}}}"#,
        )
        .unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();

        assert!(change_in(home, Some(Path::new("/x/atlas"))).unwrap());
        assert_eq!(mode(&path), 0o640);
        assert_eq!(mode(&path.with_extension("json.before-atlas")), 0o600);
    }

    #[test]
    fn a_linked_settings_file_is_changed_where_it_points() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path();
        let path = settings_in(home);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let real = dir.path().join("dotfiles.json");
        fs::write(&real, "{}").unwrap();
        std::os::unix::fs::symlink(&real, &path).unwrap();

        assert!(change_in(home, Some(Path::new("/x/atlas"))).unwrap());
        assert!(
            fs::symlink_metadata(&path)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert!(fs::read_to_string(&real).unwrap().contains("atlas"));
    }

    #[test]
    fn an_unresolved_settings_link_is_refused_and_kept() {
        for target in ["missing.json", "claude_desktop_config.json"] {
            let dir = tempfile::tempdir().unwrap();
            let home = dir.path();
            let path = settings_in(home);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::os::unix::fs::symlink(target, &path).unwrap();

            for atlas in [Some(Path::new("/x/atlas")), None] {
                let error = change_in(home, atlas).unwrap_err();
                assert!(error.error.contains(&path.display().to_string()));
                assert!(error.error.contains("Could not resolve"));
                assert_eq!(fs::read_link(&path).unwrap(), Path::new(target));
                assert!(!path.with_extension("json.before-atlas").exists());
                assert!(!path.with_extension("json.atlas-partial").exists());
            }
            assert!(!path.parent().unwrap().join("missing.json").exists());
        }
    }

    #[test]
    fn removing_reports_invalid_settings_and_keeps_the_file_as_it_was() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path();
        let path = settings_in(home);
        fs::create_dir_all(path.parent().unwrap()).unwrap();

        for before in [
            r#"{"mcpServers": {"atlas": {"command": "/x/atlas", "args": ["mcp"]}}"#,
            "[]",
            r#"{"mcpServers": null}"#,
            r#"{"mcpServers": []}"#,
            r#"{"mcpServers": "invalid"}"#,
        ] {
            fs::write(&path, before).unwrap();
            let error = change_in(home, None).unwrap_err();
            assert_eq!(error.code, "agents");
            assert!(error.error.contains(&path.display().to_string()));
            assert_eq!(fs::read_to_string(&path).unwrap(), before);
            assert!(!path.with_extension("json.before-atlas").exists());
        }
    }

    #[test]
    fn removing_an_absent_entry_keeps_other_settings_and_creates_no_file() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path();
        let path = settings_in(home);
        assert!(!change_in(home, None).unwrap());
        assert!(!path.exists());

        fs::create_dir_all(path.parent().unwrap()).unwrap();
        for before in [
            r#"{"theme": "dark"}"#,
            r#"{"mcpServers": {"other": {"command": "x"}}, "theme": "dark"}"#,
        ] {
            fs::write(&path, before).unwrap();
            assert!(!change_in(home, None).unwrap());
            assert_eq!(fs::read_to_string(&path).unwrap(), before);
            assert!(!path.with_extension("json.before-atlas").exists());
        }
    }

    #[test]
    fn another_programs_atlas_entry_and_a_broken_file_are_left_alone() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path();
        let path = settings_in(home);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let atlas = Path::new("/x/atlas");

        let theirs = r#"{"mcpServers": {"atlas": {"command": "npx", "args": ["atlas-maps"]}}}"#;
        fs::write(&path, theirs).unwrap();
        assert_eq!(change_in(home, Some(atlas)).unwrap_err().code, "agents");
        assert!(!change_in(home, None).unwrap(), "removing leaves it alone");
        assert_eq!(fs::read_to_string(&path).unwrap(), theirs);
        assert!(!connected_in(home));

        fs::write(&path, "{ not json").unwrap();
        assert_eq!(change_in(home, Some(atlas)).unwrap_err().code, "agents");
        assert_eq!(change_in(home, None).unwrap_err().code, "agents");
        assert_eq!(fs::read_to_string(&path).unwrap(), "{ not json");
    }
}

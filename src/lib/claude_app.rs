//! The Claude app (Claude for Mac): Atlas's MCP server in its settings, so
//! its chats can show the helper's screen with Stop. The app has no plugin
//! command line, so Atlas edits the one entry it owns in the app's own
//! settings file, keeping every other setting and a copy of the file as it
//! was; the app reads it when it next starts.
use crate::{
    agents::Changed,
    error::{Code, Error, Result},
};
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
        Some(atlas) => {
            let entry = entry(atlas);
            servers.insert(NAME.into(), entry.clone()).as_ref() != Some(&entry)
        }
        None => servers.remove(NAME).is_some(),
    })
}

/// Whether the app's settings file in `home` runs Atlas's MCP server.
fn connected_in(home: &Path) -> bool {
    fs::read_to_string(settings_in(home))
        .ok()
        .and_then(|text| parse(Some(&text)).ok())
        .is_some_and(|settings| {
            settings
                .get("mcpServers")
                .and_then(|servers| servers.get(NAME))
                .is_some_and(ours)
        })
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
    crate::files::write_beside(
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
    if let Ok(app) = crate::app_bundle::installed_app() {
        let inside = app.join("Contents/MacOS/atlas");
        if inside.is_file() {
            return Ok(inside);
        }
    }

    Ok(std::env::current_exe()?)
}

/// The app's row in `atlas agents`.
pub(crate) fn status() -> Value {
    let installed = installed();
    json!({
        "agent": ID,
        "installed": installed,
        "connected": installed && connected(),
        "version": Value::Null,
    })
}

/// Whether the app's settings here run Atlas's MCP server.
fn connected() -> bool {
    home().is_some_and(|home| connected_in(&home))
}

/// Whether the app is here and not connected.
pub(crate) fn unconnected() -> bool {
    installed() && !connected()
}

/// Connect the app (`connect`) or disconnect it: its row in the results.
pub(crate) fn change(connect: bool) -> Changed {
    let outcome = home()
        .ok_or_else(|| Error::new(Code::Agents, "HOME is not set"))
        .and_then(|home| {
            let atlas = if connect { Some(executable()?) } else { None };
            change_in(&home, atlas.as_deref(), &std::env::temp_dir())
        });

    match outcome {
        Ok(changed) => Changed {
            ok: true,
            report: json!({
                "agent": ID,
                "ok": true,
                "connected": connect,
                "changed": changed,
                // The app reads its MCP servers when it starts.
                "restart": changed,
            }),
        },
        Err(error) => Changed {
            ok: false,
            report: json!({"agent": ID, "ok": false, "error": error.error}),
        },
    }
}

#[cfg(test)]
mod tests;

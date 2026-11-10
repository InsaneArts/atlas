//! The terminal command that links to the installed app.
use super::account::problem;
use atlas_core::{
    app_bundle::{INSTALL_COMMAND, app_executable, developer_id},
    config,
    error::Result,
};
use serde_json::{Value, json};
use std::{fs, path::Path};

/// `~/.local/bin/atlas` → the app, so `atlas` works in a terminal.
///
/// # Errors
///
/// Returns an error when this account's home is unavailable or the link can't
/// be made.
pub fn link_command(executable: &Path) -> Result<Value> {
    link_command_in(&config::home()?.join(".local/bin"), executable)
}

fn link_command_in(bin: &Path, executable: &Path) -> Result<Value> {
    fs::create_dir_all(bin)?;

    let link = bin.join("atlas");
    if fs::canonicalize(&link).ok().as_deref() == Some(executable) {
        return Ok(json!({"linked": link, "changed": false}));
    }

    if !replaceable(&link) {
        return Err(problem(format!(
            "{} is another program, so it was left alone. Move it aside, then try again.",
            atlas_core::config::tilde(&link)
        )));
    }

    let temporary = bin.join(format!(".atlas.{}", atlas_core::files::token()));
    std::os::unix::fs::symlink(executable, &temporary)?;
    fs::rename(&temporary, &link)?;

    Ok(json!({"linked": link, "changed": true}))
}

/// Whether `link` may become Atlas's command: nothing there, a link to
/// nothing, or a link into an Atlas.app.
fn replaceable(link: &Path) -> bool {
    let Ok(metadata) = fs::symlink_metadata(link) else {
        return true;
    };

    if !metadata.file_type().is_symlink() {
        return false;
    }

    match fs::canonicalize(link) {
        Err(_) => true,
        Ok(target) => target
            .ancestors()
            .any(|folder| folder.file_name().is_some_and(|name| name == "Atlas.app")),
    }
}

/// Verify the installed app and link its command for attended setup.
/// Returns the existing init refusal if the location, signature or link is unsuitable.
///
/// # Errors
///
/// Rejects an unavailable or unsuitable installed Atlas app and returns command-link
/// filesystem failures.
pub fn link_installed_command() -> Result<Value> {
    let executable = app_executable()?;
    if !developer_id(&executable) {
        return Err(problem(format!(
            "This Atlas.app is not signed by Atlas's developer. Install Atlas with: {} (or atlas update), then run atlas init again.",
            INSTALL_COMMAND
        )));
    }
    link_command(&executable)
}

#[cfg(test)]
#[path = "installed_app_tests.rs"]
mod tests;

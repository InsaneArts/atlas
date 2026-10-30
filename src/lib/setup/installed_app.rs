//! Shared installed executable discovery and terminal command linking.
use crate::{
    config,
    error::{Code, Error, Result},
};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
};

fn problem(message: impl Into<String>) -> Error {
    Error::new(Code::Init, message)
}

/// The stable installed identity, including when a temporary worker calls us.
pub(crate) fn app_executable() -> Result<PathBuf> {
    app_executable_using(crate::update::reachable_by_others)
}

pub(crate) fn app_executable_using(reachable: impl FnOnce(&Path) -> bool) -> Result<PathBuf> {
    let executable = PathBuf::from("/Applications/Atlas.app/Contents/MacOS/atlas");
    if !reachable(&executable) {
        return Err(problem(
            "Atlas.app must be in /Applications so the helper account can run it",
        ));
    }

    Ok(executable)
}

/// `~/.local/bin/atlas` → the app, so `atlas` works in a terminal.
pub(crate) fn link_command(executable: &Path) -> Result<Value> {
    link_command_in(&config::home()?.join(".local/bin"), executable)
}

/// [`link_command`] into the folder `bin`.
fn link_command_in(bin: &Path, executable: &Path) -> Result<Value> {
    fs::create_dir_all(bin)?;

    let link = bin.join("atlas");
    if fs::canonicalize(&link).ok().as_deref() == Some(executable) {
        return Ok(json!({"linked": link, "changed": false}));
    }

    if !replaceable(&link) {
        return Err(problem(format!(
            "{} is another program, so it was left alone. Move it aside, then try again.",
            crate::ui::text::tilde(&link)
        )));
    }

    let temporary = bin.join(format!(".atlas.{}", crate::files::token()));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installed_application_identity_uses_the_fixed_path_and_refuses_unreachable_builds() {
        let expected = Path::new("/Applications/Atlas.app/Contents/MacOS/atlas");
        let executable = app_executable_using(|path| {
            assert_eq!(path, expected);
            true
        })
        .unwrap();
        assert_eq!(executable, expected);

        let error = app_executable_using(|path| {
            assert_eq!(path, expected);
            false
        })
        .unwrap_err();
        assert_eq!(error.code, "init");
        assert_eq!(
            error.error,
            "Atlas.app must be in /Applications so the helper account can run it"
        );
        assert!(!error.uncertain);
    }

    #[test]
    fn the_command_replaces_only_a_missing_dead_or_atlas_link() {
        let dir = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(dir.path()).unwrap();
        let bin = root.join("bin");
        let executable = root.join("Applications/Atlas.app/Contents/MacOS/atlas");
        let older = root.join("Old/Atlas.app/Contents/MacOS/atlas");
        for path in [&executable, &older] {
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, b"").unwrap();
        }
        let link = bin.join("atlas");

        assert_eq!(link_command_in(&bin, &executable).unwrap()["changed"], true);
        assert_eq!(
            link_command_in(&bin, &executable).unwrap()["changed"],
            false
        );

        for target in [older, root.join("gone")] {
            fs::remove_file(&link).unwrap();
            std::os::unix::fs::symlink(&target, &link).unwrap();
            assert_eq!(link_command_in(&bin, &executable).unwrap()["changed"], true);
            assert_eq!(fs::canonicalize(&link).unwrap(), executable);
        }

        let other = root.join("other-atlas");
        fs::write(&other, b"another program").unwrap();
        fs::remove_file(&link).unwrap();
        std::os::unix::fs::symlink(&other, &link).unwrap();
        assert!(link_command_in(&bin, &executable).is_err());
        assert_eq!(fs::canonicalize(&link).unwrap(), other);

        fs::remove_file(&link).unwrap();
        fs::write(&link, b"a script").unwrap();
        assert!(link_command_in(&bin, &executable).is_err());
        assert_eq!(fs::read(&link).unwrap(), b"a script");
    }
}

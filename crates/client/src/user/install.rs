//! The shared Atlas command on this disk, as the user desktop runs it:
//! linked to Atlas.app or copied into its shared folder, the build installed
//! there, and the folder beside it where builds and environment plans are
//! staged.
use crate::client::{BuildKey, Client};
use atlas_core::{
    digest::file_sha256,
    error::{Code, Error, Result},
};
use serde_json::{Value, json};
use std::{
    fs,
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::Path,
};

/// The folder holding the shared command: made (or repaired) so the helper
/// can pass through it, whatever this terminal's umask. Only a folder of
/// this account is changed; anyone else's is left for the checks to refuse.
fn shared_folder(folder: &Path) -> Result<()> {
    match fs::symlink_metadata(folder) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir_all(folder)?;
        }
        Err(error) => return Err(error.into()),
        Ok(_) => {}
    }

    let metadata = fs::symlink_metadata(folder)?;
    let me = nix::unistd::getuid().as_raw();
    // Any account can make folders in /Users/Shared: one another account
    // made, or that others can change, is never used.
    let foreign = metadata.uid() != me && metadata.uid() != 0;
    let open = metadata.uid() == 0 && metadata.mode() & 0o022 != 0;
    if !metadata.file_type().is_dir() || foreign || open {
        return Err(Error::new(
            Code::Install,
            format!(
                "{} belongs to another account, or others can change it. Atlas is set up for \
                 one person per Mac; if no one else uses it, remove the folder (an \
                 administrator can) and try again.",
                folder.display()
            ),
        ));
    }

    if metadata.uid() == me && metadata.mode() & 0o777 != 0o755 {
        fs::set_permissions(folder, fs::Permissions::from_mode(0o755))?;
    }

    Ok(())
}

/// [`Client::writable`]'s check for the second login `login`: builds and
/// environment plans are staged beside the shared Atlas command for it to
/// read; a missing folder is setup's concern, not this check's.
pub(crate) fn staging_writable_in(client: &Client, _login: &atlas_core::user::User) -> Result<()> {
    if let Some(shared) = client.config.identity_probe.parent()
        && shared.is_dir()
    {
        atlas_core::files::probe_write(shared)?;
    }

    Ok(())
}

/// [`Client::install_probe`] for a second login, which runs the shared
/// Atlas command from this disk.
pub(crate) fn install_on_this_disk(
    client: &Client,
    _login: &atlas_core::user::User,
    recovery: bool,
) -> Result<Value> {
    let source = fs::canonicalize(std::env::current_exe()?)?;
    let target = &client.config.identity_probe;

    let shared = target
        .parent()
        .filter(|parent| Path::new(crate::user::copies::ROOT).parent() == Some(*parent));
    let folder_ready = shared.is_none_or(|parent| {
        fs::symlink_metadata(parent).is_ok_and(|metadata| {
            metadata.is_dir()
                && (metadata.uid() == nix::unistd::getuid().as_raw()
                    && metadata.mode() & 0o777 == 0o755
                    || metadata.uid() == 0 && metadata.mode() & 0o022 == 0)
        })
    });
    let linked = || fs::canonicalize(target).is_ok_and(|path| path == source);
    let unchanged = json!({ "ok": true, "path": target, "alreadyInstalled": true });
    // An unchanged link and folder need no mutation or exclusion.
    // A copy is always replaced, using a fresh inode.
    if folder_ready && linked() {
        return Ok(unchanged);
    }

    let _ownership = client.controller.acquire(recovery)?;
    if let Some(parent) = shared {
        shared_folder(parent)?;
    }
    if linked() {
        return Ok(unchanged);
    }

    let parent = target
        .parent()
        .ok_or_else(|| Error::new(Code::Config, "Shared executable needs a parent directory"))?;
    fs::create_dir_all(parent)?;

    // Atlas.app's executable is sealed with its bundle: a copy would
    // fail its signature, so the shared path links to it instead, and
    // follows the app through updates.
    if atlas_core::app_bundle::app_of(&source).is_some() {
        if !atlas_core::app_bundle::reachable_by_others(&source) {
            return Err(Error::new(
                Code::Install,
                "The helper account cannot reach Atlas.app here; install it in /Applications",
            ));
        }

        let temporary = target.with_extension(format!("{}.new", atlas_core::files::token()));
        let linked = std::os::unix::fs::symlink(&source, &temporary)
            .and_then(|()| fs::rename(&temporary, target));
        if linked.is_err() {
            let _ = fs::remove_file(&temporary);
        }

        linked?;
        return Ok(json!({ "ok": true, "path": target, "linkedTo": source }));
    }

    atlas_core::files::write_beside(
        target,
        0o755,
        |file| {
            std::io::copy(&mut fs::File::open(&source)?, file)?;
            file.set_permissions(fs::Permissions::from_mode(0o755))?;
            Ok(())
        },
        // A running helper may still map the previous executable. Renaming
        // a fresh inode avoids modifying its code-signed pages in place.
        |temporary| Ok(fs::rename(temporary, target)?),
    )?;

    Ok(json!({ "ok": true, "path": target }))
}

/// The identity of a second login's shared Atlas command, a file on this
/// disk: the executable itself (through a link to the app), not the link.
pub(crate) fn build_key_on_this_disk(
    client: &Client,
    _login: &atlas_core::user::User,
) -> Option<BuildKey> {
    let metadata = fs::metadata(&client.config.identity_probe).ok()?;
    Some((
        metadata.dev(),
        metadata.ino(),
        metadata.size(),
        metadata.mtime(),
        metadata.mtime_nsec(),
    ))
}

/// The build of a second login's shared Atlas command: the SHA-256 of
/// the executable it runs.
pub(crate) fn build_on_this_disk(
    client: &Client,
    _login: &atlas_core::user::User,
) -> Option<String> {
    atlas_core::digest::probe_executable(&client.config.identity_probe)
        .and_then(|path| file_sha256(&path))
}

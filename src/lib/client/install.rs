//! The shared Atlas command the helper runs: linked to Atlas.app or copied
//! into its shared folder, and the build installed there, hashed only when
//! it changes.
use super::Client;
use crate::{
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

impl Client {
    pub fn install(&self) -> Result<Value> {
        self.install_probe(false)
    }

    /// Repair the probe under recovery exclusion without settling previous work.
    pub(crate) fn bootstrap_probe(&self) -> Result<Value> {
        self.install_probe(true)
    }

    fn install_probe(&self, recovery: bool) -> Result<Value> {
        let source = fs::canonicalize(std::env::current_exe()?)?;
        let target = &self.config.identity_probe;

        let shared = target
            .parent()
            .filter(|parent| Path::new(crate::handoff::ROOT).parent() == Some(*parent));
        let folder_ready = shared.is_none_or(|parent| {
            fs::symlink_metadata(parent).is_ok_and(|metadata| {
                metadata.is_dir()
                    && (metadata.uid() == nix::unistd::getuid().as_raw()
                        && metadata.mode() & 0o777 == 0o755
                        || metadata.uid() == 0 && metadata.mode() & 0o022 == 0)
            })
        });
        // An unchanged link and folder need no mutation or exclusion.
        // A copy is always replaced, using a fresh inode.
        if folder_ready && fs::canonicalize(target).ok().as_ref() == Some(&source) {
            return Ok(json!({ "ok": true, "path": target, "alreadyInstalled": true }));
        }

        let _ownership = self.controller.acquire(recovery)?;
        if let Some(parent) = shared {
            shared_folder(parent)?;
        }
        if fs::canonicalize(target).ok().as_ref() == Some(&source) {
            return Ok(json!({ "ok": true, "path": target, "alreadyInstalled": true }));
        }

        let parent = target.parent().ok_or_else(|| {
            Error::new(Code::Config, "Shared executable needs a parent directory")
        })?;
        fs::create_dir_all(parent)?;

        // Atlas.app's executable is sealed with its bundle: a copy would
        // fail its signature, so the shared path links to it instead, and
        // follows the app through updates.
        if crate::update::app_of(&source).is_some() {
            if !crate::update::reachable_by_others(&source) {
                return Err(Error::new(
                    Code::Install,
                    "The helper account cannot reach Atlas.app here; install it in /Applications",
                ));
            }

            let temporary = target.with_extension(format!("{}.new", crate::files::token()));
            let linked = std::os::unix::fs::symlink(&source, &temporary)
                .and_then(|()| fs::rename(&temporary, target));
            if linked.is_err() {
                let _ = fs::remove_file(&temporary);
            }

            linked?;
            return Ok(json!({ "ok": true, "path": target, "linkedTo": source }));
        }

        crate::config::write_beside(
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

    /// SHA-256 of the installed shared executable, rehashed only when its
    /// device, inode, size or modification time changes.
    pub fn installed_build(&self) -> Option<String> {
        let path = &self.config.identity_probe;
        // The executable itself (through a link to the app), not the link.
        let metadata = fs::metadata(path).ok()?;
        let key = (
            metadata.dev(),
            metadata.ino(),
            metadata.size(),
            metadata.mtime(),
            metadata.mtime_nsec(),
        );
        if let Some((cached, build)) = self.installed.lock().ok()?.as_ref()
            && *cached == key
        {
            return Some(build.clone());
        }

        // Hashed without the lock, so other callers are not held up. A
        // failed read is not cached: permissions may be repaired without
        // changing the identity key.
        let build = crate::digest::probe_executable(path).and_then(|path| file_sha256(&path))?;

        if let Ok(mut cache) = self.installed.lock() {
            *cache = Some((key, build.clone()));
        }

        Some(build)
    }
}

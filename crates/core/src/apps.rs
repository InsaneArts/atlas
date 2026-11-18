//! Which apps a machine desktop has installed, as it says of itself: this
//! Mac's Applications folder and its account's own, where `install-app`
//! puts an app. Each folder is opened once, and each `<name>.app` in it is
//! looked at beside that folder without following a link, so only a real
//! folder there counts. A link, even to an app elsewhere, a file, or
//! anything else of that name is not an installed app.
use crate::{error::Code, protocol::failure};
use nix::{
    errno::Errno,
    fcntl::{AtFlags, OFlag},
    sys::stat::{Mode, SFlag, fstatat},
};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    ffi::CString,
    os::fd::OwnedFd,
    path::{Path, PathBuf},
};

/// Where a Mac's apps are installed.
const APPLICATIONS: &str = "/Applications";

/// The most apps one look asks after.
pub(crate) const MOST_NAMES: usize = 16;

/// The folders an account whose home is `home` has its apps installed in:
/// this Mac's Applications folder, then its own `~/Applications`.
#[must_use]
pub fn folders(home: Option<&Path>) -> Vec<PathBuf> {
    std::iter::once(PathBuf::from(APPLICATIONS))
        .chain(home.map(|home| home.join("Applications")))
        .collect()
}

/// Whether `name` may name an app as its Applications folder lists it: one
/// entry's name before `.app`, never empty, `.` or `..`, and holding no
/// slash or NUL, so it never reaches outside the folder.
#[must_use]
pub(crate) fn plain(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".." && !name.contains(['/', '\0'])
}

/// Whether each of `names` is installed in any of `folders`: its
/// `<name>.app` a real folder in one of them, never a link. A folder that
/// is not there, or is a link or no folder, holds nothing that counts. A
/// name that is not [`plain`] is never installed.
///
/// # Errors
///
/// Fails when a folder that is there cannot be opened, or an app not found
/// in any could not be looked for in one: whether it is there is not known.
fn in_applications(
    folders: &[PathBuf],
    names: &[String],
) -> std::io::Result<BTreeMap<String, bool>> {
    let mut opened = Vec::new();
    for folder in folders {
        match nix::fcntl::open(
            folder.as_path(),
            OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
            Mode::empty(),
        ) {
            Ok(folder) => opened.push(folder),
            Err(Errno::ENOENT | Errno::ELOOP | Errno::ENOTDIR) => {}
            Err(error) => return Err(error.into()),
        }
    }
    names
        .iter()
        .map(|name| Ok((name.clone(), plain(name) && in_any(&opened, name)?)))
        .collect()
}

/// Whether `<name>.app` is a real folder in any of `opened`: yes once one
/// has it, whatever another could not say.
fn in_any(opened: &[OwnedFd], name: &str) -> std::io::Result<bool> {
    let mut unknown = None;
    for folder in opened {
        match app_folder(folder, name) {
            Ok(true) => return Ok(true),
            Ok(false) => {}
            Err(error) => unknown = Some(error),
        }
    }
    unknown.map_or(Ok(false), Err)
}

/// Whether `<name>.app` in `folder` is a real folder: not when there is no
/// such entry.
///
/// # Errors
///
/// Fails when the entry cannot be looked at, as in a folder that may be
/// read but not searched.
fn app_folder(folder: &OwnedFd, name: &str) -> std::io::Result<bool> {
    let Ok(entry) = CString::new(format!("{name}.app")) else {
        return Ok(false);
    };
    match fstatat(folder, entry.as_c_str(), AtFlags::AT_SYMLINK_NOFOLLOW) {
        Ok(stat) => {
            Ok(SFlag::from_bits_truncate(stat.st_mode & SFlag::S_IFMT.bits()) == SFlag::S_IFDIR)
        }
        Err(Errno::ENOENT | Errno::ENOTDIR) => Ok(false),
        Err(error) => Err(error.into()),
    }
}

/// The reply to an `installed` read of `names` in `folders`: `installed`,
/// whether each is there by its name.
#[must_use]
pub fn answer(folders: &[PathBuf], names: &[String]) -> Value {
    match in_applications(folders, names) {
        Ok(installed) => json!({"ok": true, "op": "installed", "installed": installed}),
        Err(_) => failure(
            Code::Apps,
            "An Applications folder could not be read, so which apps are installed is not known.",
        ),
    }
}

#[cfg(test)]
#[path = "apps_tests.rs"]
mod tests;

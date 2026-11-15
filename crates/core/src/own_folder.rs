//! Atlas's own folder, `~/Library/Application Support/Atlas`: the VMs' shares
//! with their bootstrap secrets, the station's folders, keys and records.
//! Nothing in it, the folder itself, or a folder that holds it, is ever handed
//! to a desktop or read for an agent, whatever the person lets agents copy.
//! A path is judged as written and where its links lead, whether or not it
//! exists; a Finder alias is a plain file here, so a caller that hands one
//! over judges where it leads as well.
use crate::error::{Code, Error, Result};
use std::{
    fs,
    path::{Component, Path, PathBuf},
};

/// Where Atlas's own folder is in a home.
pub const FOLDER: &str = "Library/Application Support/Atlas";

/// The one refusal, for every file handed over and every file read.
pub const REFUSAL: &str = "Atlas won't hand over or read that: it is in Atlas's own folder, \
     or holds it, where Atlas keeps its keys and records";

/// Where the system's data volume shows the same folders under another path.
const DATA: &str = "/System/Volumes/Data";

/// Refuses `path` when it is in Atlas's own folder in this account's home,
/// is that folder, or holds it. Nothing is read.
///
/// # Errors
///
/// Returns the `credentials` refusal, or the failure to find this account's
/// home.
pub fn refuse(path: &Path) -> Result<()> {
    refuse_in(path, &crate::config::home()?)
}

/// [`refuse`] with `home` as the home folder.
///
/// # Errors
///
/// Returns the `credentials` refusal.
pub fn refuse_in(path: &Path, home: &Path) -> Result<()> {
    if touches(path, home) {
        return Err(refusal());
    }
    Ok(())
}

/// The refusal of a path in, or holding, Atlas's own folder.
fn refusal() -> Error {
    Error::new(Code::Credentials, REFUSAL)
}

/// Whether `path`, as written or where its links lead, lies in Atlas's own
/// folder in `home`, is it, or holds it. Names are compared as the Mac's disks
/// compare them, ignoring case, and through the data volume's other spelling.
fn touches(path: &Path, home: &Path) -> bool {
    let folders = folders(home);
    [written(path), resolved(path)]
        .into_iter()
        .flatten()
        .map(|candidate| plain(&candidate))
        .any(|candidate| {
            folders
                .iter()
                .any(|folder| under(&candidate, folder) || under(folder, &candidate))
        })
}

/// Atlas's own folder in `home` by every path that names it: as written,
/// and where it really is when it or a folder above it is a link.
fn folders(home: &Path) -> Vec<PathBuf> {
    let folder = home.join(FOLDER);
    let mut folders: Vec<PathBuf> = [written(&folder), resolved(&folder)]
        .into_iter()
        .flatten()
        .map(|folder| plain(&folder))
        .collect();
    folders.dedup();
    folders
}

/// `path` made absolute, with `.` dropped and each `..` taking away the name
/// before it, as written: no link is followed.
fn written(path: &Path) -> Option<PathBuf> {
    let mut kept = PathBuf::new();
    for part in std::path::absolute(path).ok()?.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                kept.pop();
            }
            other => kept.push(other),
        }
    }
    Some(kept)
}

/// Where `path` really is once every link on it is followed: the real path of
/// the deepest part of it that exists, with what does not exist yet after it.
fn resolved(path: &Path) -> Option<PathBuf> {
    let absolute = std::path::absolute(path).ok()?;
    let mut rest = Vec::new();
    let mut existing = absolute.as_path();
    loop {
        if let Ok(real) = fs::canonicalize(existing) {
            let mut real = real;
            for part in rest.iter().rev() {
                match part {
                    Component::ParentDir => {
                        real.pop();
                    }
                    Component::CurDir => {}
                    other => real.push(other),
                }
            }
            return Some(real);
        }
        rest.push(existing.components().next_back()?);
        existing = existing.parent()?;
    }
}

/// `path` without the data volume's prefix, which shows the same folders.
fn plain(path: &Path) -> PathBuf {
    match strip(path, Path::new(DATA)) {
        Some(inside) => Path::new("/").join(inside),
        None => path.to_path_buf(),
    }
}

/// Whether `path` is `base` or lies under it, ignoring case.
fn under(path: &Path, base: &Path) -> bool {
    strip(path, base).is_some()
}

/// What is left of `path` below `base`, comparing names ignoring case; none
/// when `path` is not under `base`.
fn strip<'a>(path: &'a Path, base: &Path) -> Option<&'a Path> {
    let mut rest = path.components();
    for part in base.components() {
        let next = rest.next()?;
        if !next
            .as_os_str()
            .to_string_lossy()
            .eq_ignore_ascii_case(&part.as_os_str().to_string_lossy())
        {
            return None;
        }
    }
    Some(rest.as_path())
}

#[cfg(test)]
#[path = "own_folder_tests.rs"]
mod tests;

//! Moving one entry from a folder to another without ever replacing what
//! is at the destination, between folders held open by descriptor: how
//! cleanup puts something it is about to delete into quarantine, and how
//! an install is put in place. Every rename Atlas makes goes through
//! here, as libc's `renameatx_np` has no safe binding.
use std::{
    ffi::{CStr, CString},
    fs, io,
    os::{
        fd::{AsRawFd, RawFd},
        unix::ffi::OsStrExt,
    },
    path::Path,
};

/// How a rename treats what is at its destination.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Replace {
    /// Never replaced: an existing destination fails the rename atomically,
    /// with `AlreadyExists`.
    Never,
    /// Replaced, when there is one.
    Allowed,
    /// Exchanged with the entry renamed, in one atomic step: both must
    /// exist, and a failure leaves both in place.
    Swap,
}

/// `renameatx_np`: rename `from`, named relative to the folder `from_dir`
/// or `AT_FDCWD`, to `to`, named relative to `to_dir`, replacing what is
/// at `to` only as `replace` allows.
fn rename_at(
    from_dir: RawFd,
    from: &CStr,
    to_dir: RawFd,
    to: &CStr,
    replace: Replace,
) -> io::Result<()> {
    let flags = match replace {
        Replace::Never => libc::RENAME_EXCL,
        Replace::Allowed => 0,
        Replace::Swap => libc::RENAME_SWAP,
    };
    // SAFETY: the descriptors are live ones borrowed for the call, or
    // AT_FDCWD, and both names are NUL-terminated strings borrowed for the
    // call too; the kernel only reads them. A descriptor that is not a
    // folder fails the call with ENOTDIR.
    let status = unsafe { libc::renameatx_np(from_dir, from.as_ptr(), to_dir, to.as_ptr(), flags) };
    if status != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// Rename `from` to `to` inside the folder `folder`, replacing what is at
/// `to` only as `replace` allows.
///
/// # Errors
///
/// Returns the I/O error of the rename.
pub fn rename_within(
    folder: &fs::File,
    from: &CStr,
    to: &CStr,
    replace: Replace,
) -> io::Result<()> {
    rename_at(folder.as_raw_fd(), from, folder.as_raw_fd(), to, replace)
}

/// Rename the path `from` to the path `to`, replacing what is at `to` only
/// as `replace` allows.
///
/// # Errors
///
/// Returns the I/O error of the rename.
pub fn rename_path(from: &CStr, to: &CStr, replace: Replace) -> io::Result<()> {
    rename_at(libc::AT_FDCWD, from, libc::AT_FDCWD, to, replace)
}

/// Exchange two existing paths in one atomic step; a failure leaves both in
/// place.
///
/// # Errors
///
/// Returns the failed exchange, or that a path holds a NUL.
pub fn exchange(first: &Path, second: &Path) -> io::Result<()> {
    let path = |path: &Path| CString::new(path.as_os_str().as_bytes()).map_err(io::Error::other);
    rename_path(&path(first)?, &path(second)?, Replace::Swap)
}

/// One plain file name: not empty, not `.` or `..`, without `/` or NUL, so
/// a call that takes it names exactly one entry of the folder it is given.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileName(CString);

impl FileName {
    /// `name` as a file name, when it is a plain one.
    pub fn new(name: &str) -> Option<Self> {
        let plain = !name.is_empty() && name != "." && name != ".." && !name.contains('/');
        plain.then(|| CString::new(name).ok()).flatten().map(Self)
    }

    pub(crate) fn as_c_str(&self) -> &CStr {
        &self.0
    }
}

/// How a move that happened ended.
#[derive(Debug)]
pub enum Moved {
    /// Moved, and both folders written through.
    Synced,
    /// Moved, but a folder could not be written through: the entry is at
    /// its new name, though a power cut might still lose that.
    Unsynced(io::Error),
}

/// Move `from` in the folder `from_dir` to `to` in the folder `to_dir`, on
/// the same volume, refusing an existing `to` atomically. An error means
/// nothing moved; an `AlreadyExists` error says something was there first.
///
/// # Errors
///
/// Returns the I/O error of the move.
pub fn move_between(
    from_dir: &fs::File,
    from: &FileName,
    to_dir: &fs::File,
    to: &FileName,
) -> io::Result<Moved> {
    // Both names are plain file names (`FileName` refuses `/`, `.`, `..`
    // and NUL), so each names exactly one entry of its folder.
    rename_at(
        from_dir.as_raw_fd(),
        from.as_c_str(),
        to_dir.as_raw_fd(),
        to.as_c_str(),
        Replace::Never,
    )?;

    match from_dir.sync_all().and_then(|()| to_dir.sync_all()) {
        Ok(()) => Ok(Moved::Synced),
        Err(error) => Ok(Moved::Unsynced(error)),
    }
}

#[cfg(test)]
mod tests;

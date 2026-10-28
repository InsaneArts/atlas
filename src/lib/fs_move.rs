//! Moving one entry from a folder to another without ever replacing what
//! is at the destination, between folders held open by descriptor: how
//! cleanup puts something it is about to delete into quarantine, and how
//! an install is put in place. libc's `renameatx_np` has no safe binding.
use std::{
    ffi::{CStr, CString},
    fs, io,
    os::fd::AsRawFd,
};

/// One plain file name: not empty, not `.` or `..`, without `/` or NUL, so
/// a call that takes it names exactly one entry of the folder it is given.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FileName(CString);

impl FileName {
    /// `name` as a file name, when it is a plain one.
    pub fn new(name: &str) -> Option<Self> {
        let plain = !name.is_empty() && name != "." && name != ".." && !name.contains('/');
        plain.then(|| CString::new(name).ok()).flatten().map(Self)
    }

    pub fn as_c_str(&self) -> &CStr {
        &self.0
    }
}

/// How a move that happened ended.
#[derive(Debug)]
pub(crate) enum Moved {
    /// Moved, and both folders written through.
    Synced,
    /// Moved, but a folder could not be written through: the entry is at
    /// its new name, though a power cut might still lose that.
    Unsynced(io::Error),
}

/// Move `from` in the folder `from_dir` to `to` in the folder `to_dir`, on
/// the same volume, refusing an existing `to` atomically. An error means
/// nothing moved; an `AlreadyExists` error says something was there first.
pub(crate) fn move_between(
    from_dir: &fs::File,
    from: &FileName,
    to_dir: &fs::File,
    to: &FileName,
) -> io::Result<Moved> {
    // SAFETY: both descriptors are open folders borrowed for the length of
    // the call, and both names are NUL-terminated plain file names
    // (`FileName` refuses `/`, `.`, `..` and NUL), each borrowed for the
    // call too; RENAME_EXCL makes the rename refuse an existing target
    // instead of replacing it.
    let status = unsafe {
        libc::renameatx_np(
            from_dir.as_raw_fd(),
            from.as_c_str().as_ptr(),
            to_dir.as_raw_fd(),
            to.as_c_str().as_ptr(),
            libc::RENAME_EXCL,
        )
    };
    if status != 0 {
        return Err(io::Error::last_os_error());
    }

    match from_dir.sync_all().and_then(|()| to_dir.sync_all()) {
        Ok(()) => Ok(Moved::Synced),
        Err(error) => Ok(Moved::Unsynced(error)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_plain_file_name_is_one() {
        for plain in ["Probe.app", ".atlas-removing-x", "a b"] {
            assert!(FileName::new(plain).is_some(), "{plain}");
        }
        for not in ["", ".", "..", "a/b", "/", "a\0b"] {
            assert!(FileName::new(not).is_none(), "{not:?}");
        }
    }

    #[test]
    fn a_move_never_replaces_what_is_there() {
        let directory = tempfile::tempdir().unwrap();
        let (from, to) = (directory.path().join("from"), directory.path().join("to"));
        fs::create_dir(&from).unwrap();
        fs::create_dir(&to).unwrap();
        fs::write(from.join("item"), "moved").unwrap();
        fs::write(to.join("item"), "kept").unwrap();
        let (from_dir, to_dir) = (fs::File::open(&from).unwrap(), fs::File::open(&to).unwrap());
        let name = FileName::new("item").unwrap();

        let error = move_between(&from_dir, &name, &to_dir, &name).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(fs::read_to_string(to.join("item")).unwrap(), "kept");

        let other = FileName::new("other").unwrap();
        assert!(matches!(
            move_between(&from_dir, &name, &to_dir, &other).unwrap(),
            Moved::Synced
        ));
        assert_eq!(fs::read_to_string(to.join("other")).unwrap(), "moved");
        assert!(!from.join("item").exists());
    }
}

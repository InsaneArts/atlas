//! Darwin extended ACLs (sys/acl.h) behind one owned, freed-on-drop handle.
// FFI boundary: macOS ACL functions have no safe binding.
#![allow(unsafe_code)]
use std::{
    ffi::{CStr, CString, c_char, c_int, c_void},
    fs::File,
    io,
    os::{fd::AsRawFd, unix::ffi::OsStrExt},
    path::Path,
};

const ACL_TYPE_EXTENDED: c_int = 0x100;
const ACL_FIRST_ENTRY: c_int = 0;
const ACL_NEXT_ENTRY: c_int = -1;
const ACL_EXTENDED_ALLOW: c_int = 1;

/// Data, append, delete, attribute, ACL and ownership writes.
pub const WRITE_RIGHTS: u64 =
    (1 << 2) | (1 << 4) | (1 << 5) | (1 << 6) | (1 << 8) | (1 << 10) | (1 << 12) | (1 << 13);

unsafe extern "C" {
    fn acl_get_file(path: *const c_char, kind: c_int) -> *mut c_void;
    fn acl_get_fd(fd: c_int) -> *mut c_void;
    fn acl_get_entry(acl: *mut c_void, index: c_int, entry: *mut *mut c_void) -> c_int;
    fn acl_get_tag_type(entry: *mut c_void, tag: *mut c_int) -> c_int;
    fn acl_get_permset_mask_np(entry: *mut c_void, mask: *mut u64) -> c_int;
    fn acl_valid(acl: *mut c_void) -> c_int;
    fn acl_init(count: c_int) -> *mut c_void;
    fn acl_set_fd(fd: c_int, acl: *mut c_void) -> c_int;
    fn acl_to_text(acl: *mut c_void, length: *mut isize) -> *mut c_char;
    fn acl_free(object: *mut c_void) -> c_int;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Entry {
    pub allow: bool,
    pub mask: u64,
}

pub struct Acl(*mut c_void);

impl Drop for Acl {
    fn drop(&mut self) {
        // SAFETY: the handle was returned by acl_get_* or acl_init and is freed once.
        unsafe { acl_free(self.0) };
    }
}

impl Acl {
    /// The extended ACL of `path` (following a final symlink), or None.
    pub fn of_path(path: &Path) -> io::Result<Option<Self>> {
        let path = CString::new(path.as_os_str().as_bytes())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "Path contains NUL"))?;
        // SAFETY: NUL-terminated path and the documented extended ACL type.
        Self::wrap(unsafe { acl_get_file(path.as_ptr(), ACL_TYPE_EXTENDED) })
    }
    /// The extended ACL of an open file, or None.
    pub fn of_file(file: &File) -> io::Result<Option<Self>> {
        // SAFETY: the descriptor stays open for the duration of the call.
        Self::wrap(unsafe { acl_get_fd(file.as_raw_fd()) })
    }
    fn wrap(acl: *mut c_void) -> io::Result<Option<Self>> {
        if !acl.is_null() {
            return Ok(Some(Self(acl)));
        }
        let error = io::Error::last_os_error();
        // macOS reports ENOENT for an existing object without an extended ACL.
        if error.raw_os_error() == Some(libc::ENOENT) {
            Ok(None)
        } else {
            Err(error)
        }
    }
    pub fn is_valid(&self) -> bool {
        // SAFETY: live ACL handle.
        unsafe { acl_valid(self.0) == 0 }
    }
    pub fn entries(&self) -> io::Result<Vec<Entry>> {
        let mut entries = Vec::new();
        let mut index = ACL_FIRST_ENTRY;
        let mut entry = std::ptr::null_mut();
        // SAFETY: live ACL handle; entries borrow from it and valid out pointers.
        while unsafe { acl_get_entry(self.0, index, &mut entry) } == 0 {
            index = ACL_NEXT_ENTRY;
            let (mut tag, mut mask) = (0, 0);
            // SAFETY: `entry` was just returned for this live ACL; both
            // outputs are valid for writes.
            if unsafe { acl_get_tag_type(entry, &mut tag) } != 0
                // SAFETY: as above.
                || unsafe { acl_get_permset_mask_np(entry, &mut mask) } != 0
            {
                return Err(io::Error::last_os_error());
            }
            entries.push(Entry {
                allow: tag == ACL_EXTENDED_ALLOW,
                mask,
            });
        }
        Ok(entries)
    }
    /// Text form, one `user:UUID:name:id:allow|deny:rights` line per entry.
    pub fn text(&self) -> io::Result<String> {
        // SAFETY: live ACL handle; the returned string is freed with acl_free.
        let text = unsafe { acl_to_text(self.0, std::ptr::null_mut()) };
        if text.is_null() {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: acl_to_text returned a NUL-terminated string (checked non-null).
        let owned = unsafe { CStr::from_ptr(text) }
            .to_string_lossy()
            .into_owned();
        // SAFETY: frees the string acl_to_text allocated, once, after its last use.
        unsafe { acl_free(text.cast()) };
        Ok(owned)
    }
}

/// Whether `path` carries any extended ACL entry; an invalid ACL is an error.
pub fn present(path: &Path) -> io::Result<bool> {
    match Acl::of_path(path)? {
        None => Ok(false),
        Some(acl) if acl.is_valid() => Ok(!acl.entries()?.is_empty()),
        Some(_) => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Invalid filesystem ACL",
        )),
    }
}

/// Whether any extended ACL entry on `path` allows a write; deny entries are
/// harmless. An invalid ACL is an error.
pub fn allows_write(path: &Path) -> io::Result<bool> {
    match Acl::of_path(path)? {
        None => Ok(false),
        Some(acl) if acl.is_valid() => Ok(acl
            .entries()?
            .iter()
            .any(|entry| entry.allow && entry.mask & WRITE_RIGHTS != 0)),
        Some(_) => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Invalid filesystem ACL",
        )),
    }
}

/// Replace an open file's extended ACL with an empty one.
pub fn clear(file: &File) -> io::Result<()> {
    // SAFETY: acl_init returns an owned handle, freed by Acl's Drop.
    let empty = Acl::wrap(unsafe { acl_init(0) })?
        .ok_or_else(|| io::Error::other("Could not allocate an ACL"))?;
    // SAFETY: live descriptor and ACL handle.
    if unsafe { acl_set_fd(file.as_raw_fd(), empty.0) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    #[test]
    fn entries_text_and_clear_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("file");
        std::fs::write(&path, b"x").unwrap();
        assert!(!present(&path).unwrap());
        let user = std::env::var("USER").unwrap();
        assert!(
            Command::new("/bin/chmod")
                .args(["+a", &format!("user:{user} allow write")])
                .arg(&path)
                .status()
                .unwrap()
                .success()
        );
        let acl = Acl::of_path(&path).unwrap().unwrap();
        let entries = acl.entries().unwrap();
        assert_eq!(entries.len(), 1);
        assert!(entries[0].allow && entries[0].mask & WRITE_RIGHTS != 0);
        assert!(acl.text().unwrap().contains(&format!(":{user}:")));
        let file = File::open(&path).unwrap();
        clear(&file).unwrap();
        assert!(!present(&path).unwrap());
        assert!(
            Acl::of_file(&file)
                .unwrap()
                .is_none_or(|acl| acl.entries().unwrap().is_empty())
        );
    }
}

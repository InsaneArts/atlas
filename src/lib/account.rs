//! The accounts of this Mac, as directory services know them, and where
//! their commands are looked for.
use crate::error::{Code, Error, Result};
use serde::Serialize;
use std::{
    ffi::{CStr, CString},
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Account {
    pub name: String,
    pub uid: u32,
    pub home: PathBuf,
}

enum Lookup {
    Name(CString),
    Uid(u32),
}

/// macOS numbers people's accounts from this user ID; lower ones belong to
/// the system.
pub const FIRST_USER_UID: u32 = 501;

#[allow(
    unsafe_code,
    reason = "FFI: getpw*_r, whose ERANGE nix misreads on macOS"
)]
fn lookup(key: Lookup) -> Result<Option<Account>> {
    // SAFETY: passwd is plain C data for which all-zero is a valid value.
    let mut entry: libc::passwd = unsafe { std::mem::zeroed() };
    let mut found = std::ptr::null_mut();
    let mut buffer = Vec::new();
    let mut status = libc::ERANGE;

    // Directory-service records can exceed any fixed buffer; grow on ERANGE.
    // The error is the return value: macOS need not set errno here.
    for size in [16 << 10, 64 << 10, 256 << 10, 1 << 20] {
        buffer.resize(size, 0);

        // SAFETY: valid entry, buffer and result pointers that outlive the call;
        // the strings in `entry` point into `buffer` and are copied before return.
        status = unsafe {
            match &key {
                Lookup::Name(name) => libc::getpwnam_r(
                    name.as_ptr(),
                    &mut entry,
                    buffer.as_mut_ptr(),
                    buffer.len(),
                    &mut found,
                ),
                Lookup::Uid(uid) => libc::getpwuid_r(
                    *uid,
                    &mut entry,
                    buffer.as_mut_ptr(),
                    buffer.len(),
                    &mut found,
                ),
            }
        };

        if status != libc::ERANGE {
            break;
        }
    }

    if status != 0 {
        return Err(std::io::Error::from_raw_os_error(status).into());
    }

    if found.is_null() {
        return Ok(None);
    }

    // SAFETY: getpw*_r succeeded, so both fields are NUL-terminated strings.
    let text = |field| {
        // SAFETY: see above.
        unsafe { CStr::from_ptr(field) }
            .to_string_lossy()
            .into_owned()
    };

    Ok(Some(Account {
        name: text(entry.pw_name),
        uid: entry.pw_uid,
        home: PathBuf::from(text(entry.pw_dir)),
    }))
}

/// Resolve an account through directory services by short name.
///
/// # Errors
///
/// Rejects a name containing NUL and returns directory-service lookup errors. A missing
/// account is represented by None.
pub fn account_named(name: &str) -> Result<Option<Account>> {
    let name = CString::new(name)
        .map_err(|_| Error::new(Code::Setup, "Account names cannot contain NUL"))?;
    lookup(Lookup::Name(name))
}

/// Resolve an account through directory services by UID.
///
/// # Errors
///
/// Returns directory-service lookup errors. A missing UID is represented by None rather
/// than an invented account.
pub fn account_with_uid(uid: u32) -> Result<Option<Account>> {
    lookup(Lookup::Uid(uid))
}

/// Where tools installed for every account live, in search order.
pub(crate) const SHARED_BIN: &[&str] = &[
    "/opt/homebrew/bin",
    "/usr/local/bin",
    "/usr/bin",
    "/bin",
    "/usr/sbin",
    "/sbin",
];

/// Where a command runs from inside the helper whose home is `home`: the
/// helper's own tools first, then the shared places.
pub(crate) fn helper_search_path(home: &Path) -> Vec<PathBuf> {
    let mut places = vec![home.join(".local/bin"), home.join(".cargo/bin")];
    places.extend(SHARED_BIN.iter().map(PathBuf::from));
    places
}

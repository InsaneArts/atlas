//! The accounts of this Mac, as directory services know them, and where
//! their commands are looked for.
use crate::error::{Code, Error, Result};
use serde::Serialize;
use std::{
    ffi::{CStr, CString},
    path::{Path, PathBuf},
    process::Command,
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
pub const SHARED_BIN: &[&str] = &[
    "/opt/homebrew/bin",
    "/usr/local/bin",
    "/usr/bin",
    "/bin",
    "/usr/sbin",
    "/sbin",
];

/// Where a command runs from inside the helper whose home is `home`: the
/// helper's own tools first, then the shared places.
pub fn helper_search_path(home: &Path) -> Vec<PathBuf> {
    let mut places = vec![home.join(".local/bin"), home.join(".cargo/bin")];
    places.extend(SHARED_BIN.iter().map(PathBuf::from));
    places
}

/// The full name of account `name`, as people know it, if it has one.
pub fn real_name(name: &str) -> Option<String> {
    asked_real_name(name).titled()
}

/// What directory services say of an account's full name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Title {
    /// They did not answer in time.
    Unknown,
    /// The account has none.
    Untitled,
    /// The account's full name.
    Titled(String),
}

impl Title {
    pub fn titled(self) -> Option<String> {
        match self {
            Self::Titled(title) => Some(title),
            Self::Unknown | Self::Untitled => None,
        }
    }
}

pub fn asked_real_name(name: &str) -> Title {
    let Ok(output) = crate::bounded::output(
        Command::new("/usr/bin/dscl").args([".", "-read", &format!("/Users/{name}"), "RealName"]),
        crate::bounded::LOOKUP,
    ) else {
        return Title::Unknown;
    };

    parse_real_name(&output.stdout).map_or(Title::Untitled, Title::Titled)
}

/// The full name in `dscl -read`'s answer for RealName, if it has one.
fn parse_real_name(answer: &[u8]) -> Option<String> {
    let text = std::str::from_utf8(answer).ok()?;
    let value = text.strip_prefix("RealName:")?.trim();
    (!value.is_empty()).then(|| value.to_owned())
}

/// Whether `name` administers this Mac (a helper must not); None when macOS
/// cannot say, which never counts as "no".
pub fn administrator(name: &str) -> Option<bool> {
    // An agent's sandbox may keep dseditgroup from the directory service;
    // the account's group list still answers there.
    directory_member(name).or_else(|| listed_member(name))
}

fn directory_member(name: &str) -> Option<bool> {
    let output = crate::bounded::output(
        Command::new("/usr/sbin/dseditgroup").args(["-o", "checkmember", "-m", name, "admin"]),
        crate::bounded::LOOKUP,
    )
    .ok()?;

    let answer = String::from_utf8_lossy(&output.stdout);
    if answer.starts_with("yes ") {
        Some(true)
    } else if answer.starts_with("no ") {
        Some(false)
    } else {
        None
    }
}

/// The admin group's ID, the same on every Mac.
const ADMIN_GID: &str = "80";

/// Whether the admin group is among the group IDs `id` lists for `name`.
/// IDs, not names: a name `id` cannot resolve is printed as its number.
fn listed_member(name: &str) -> Option<bool> {
    let output = crate::bounded::output(
        Command::new("/usr/bin/id").args(["-G", name]),
        crate::bounded::LOOKUP,
    )
    .ok()
    .filter(|output| output.status.success())?;

    parse_admin_listed(&String::from_utf8_lossy(&output.stdout))
}

/// Whether `id -G`'s list holds the admin group's ID; None for no list.
fn parse_admin_listed(groups: &str) -> Option<bool> {
    let mut groups = groups.split_whitespace().peekable();
    groups.peek()?;

    Some(groups.any(|group| group == ADMIN_GID))
}

#[cfg(test)]
#[path = "account_tests.rs"]
mod tests;

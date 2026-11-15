//! Who is at the other end of a local socket, as the kernel stamped it when
//! that process connected: its audit token. Checked against a code-signing
//! requirement and an executable through that token, so a process that
//! connected and then ran another program, or handed the socket to one,
//! never passes as the program it became.
#![allow(
    unsafe_code,
    reason = "FFI: getsockopt(LOCAL_PEERTOKEN) and Security.framework's SecCode calls have no safe binding"
)]

use objc2_core_foundation::{CFData, CFDictionary, CFRetained, CFString, CFType, CFURL};
use objc2_security::{
    SecCSFlags, SecCode, SecRequirement, SecStaticCode, kSecCodeInfoMainExecutable,
    kSecCodeInfoUnique, kSecGuestAttributeAudit,
};
use std::{
    os::fd::{AsRawFd, BorrowedFd},
    path::Path,
    ptr::{NonNull, null_mut},
};

/// The kernel's audit token: eight words, as `audit_token_t` holds them.
type AuditToken = [u32; 8];

/// The process that connected `socket`, when it runs `executable`, exactly
/// as the file is on disk, with running code that satisfies `requirement`.
pub fn verified(socket: BorrowedFd<'_>, executable: &Path, requirement: &str) -> Option<u32> {
    let token = peer_token(socket)?;
    let code = guest(&token)?;
    let runs = satisfies(&code, requirement)
        && path_of(&code).is_some_and(|path| same_file(&path, executable));
    // The token's sixth word is the process's ID.
    runs.then_some(token[5])
}

/// The process that connected a socket, as the kernel stamped it then.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Peer {
    pub pid: u32,
    /// Told apart from any later process given its PID.
    pub pid_version: u32,
    /// Its effective account.
    pub uid: u32,
    /// The code directory hash of the program it runs: which build it is.
    pub cdhash: Vec<u8>,
}

/// The process that connected `socket`, when its running code satisfies
/// `requirement`, wherever on disk it was started from: one whose bundle
/// moved since still runs the code it was started with.
pub fn running(socket: BorrowedFd<'_>, requirement: &str) -> Option<Peer> {
    let token = peer_token(socket)?;
    let code = guest(&token)?;
    if !satisfies(&code, requirement) {
        return None;
    }
    // The token's words: its euid second, its PID sixth, its PID version last.
    Some(Peer {
        pid: token[5],
        pid_version: token[7],
        uid: token[1],
        cdhash: cdhash_of(&code)?,
    })
}

/// The code directory hash of the program `path`, as its signature says,
/// when it is signed.
pub fn file_cdhash(path: &Path) -> Option<Vec<u8>> {
    let url = CFURL::from_file_path(path)?;
    let mut on_disk: *const SecStaticCode = std::ptr::null();
    // SAFETY: `on_disk` is a valid place for the call's result.
    let status = unsafe {
        SecStaticCode::create_with_path(&url, SecCSFlags::empty(), NonNull::from(&mut on_disk))
    };
    let on_disk = (status == 0)
        .then_some(NonNull::new(on_disk.cast_mut()))
        .flatten()?;
    // SAFETY: on success the call returned a retained static code object.
    let on_disk = unsafe { CFRetained::from_raw(on_disk) };
    unique(&on_disk)
}

/// The code directory hash of what `code` runs.
fn cdhash_of(code: &SecCode) -> Option<Vec<u8>> {
    let mut on_disk: *const SecStaticCode = std::ptr::null();
    // SAFETY: `on_disk` is a valid place for the call's result.
    let status = unsafe { code.copy_static_code(SecCSFlags::empty(), NonNull::from(&mut on_disk)) };
    let on_disk = (status == 0)
        .then_some(NonNull::new(on_disk.cast_mut()))
        .flatten()?;
    // SAFETY: on success the call returned a retained static code object.
    let on_disk = unsafe { CFRetained::from_raw(on_disk) };
    unique(&on_disk)
}

/// The code directory hash `on_disk`'s signature names.
fn unique(on_disk: &SecStaticCode) -> Option<Vec<u8>> {
    let information = signing_information(on_disk)?;
    // SAFETY: the key is the framework's own constant string.
    let key: &CFString = unsafe { kSecCodeInfoUnique };
    let unique = information.get(key)?.downcast::<CFData>().ok()?;
    Some(unique.to_vec())
}

/// The audit token of the process that connected `socket`.
fn peer_token(socket: BorrowedFd<'_>) -> Option<AuditToken> {
    let mut token: AuditToken = [0; 8];
    let mut length = libc::socklen_t::try_from(size_of::<AuditToken>()).ok()?;
    // SAFETY: the buffer and its length are valid for the call, which
    // writes at most `length` bytes into it.
    let status = unsafe {
        libc::getsockopt(
            socket.as_raw_fd(),
            libc::SOL_LOCAL,
            libc::LOCAL_PEERTOKEN,
            token.as_mut_ptr().cast(),
            &mut length,
        )
    };
    (status == 0 && usize::try_from(length).ok() == Some(size_of::<AuditToken>())).then_some(token)
}

/// The running code of the process `token` names; none when it ended or
/// ran another program since.
fn guest(token: &AuditToken) -> Option<CFRetained<SecCode>> {
    let bytes: Vec<u8> = token.iter().flat_map(|word| word.to_ne_bytes()).collect();
    let data = CFData::from_bytes(&bytes);
    // SAFETY: the key is the framework's own constant string.
    let key: &CFString = unsafe { kSecGuestAttributeAudit };
    let attributes = CFDictionary::<CFString, CFType>::from_slices(&[key], &[data.as_ref()]);
    let mut code: *mut SecCode = null_mut();
    // SAFETY: the dictionary maps the audit key to its data, as the call
    // asks; `code` is a valid place for its result.
    let status = unsafe {
        SecCode::copy_guest_with_attributes(
            None,
            Some(attributes.as_opaque()),
            SecCSFlags::empty(),
            NonNull::from(&mut code),
        )
    };
    // SAFETY: on success the call returned a retained code object.
    (status == 0).then(|| NonNull::new(code).map(|code| unsafe { CFRetained::from_raw(code) }))?
}

/// Whether `code`'s running code is valid and meets `requirement`.
fn satisfies(code: &SecCode, requirement: &str) -> bool {
    let text = CFString::from_str(requirement);
    let mut parsed: *mut SecRequirement = null_mut();
    // SAFETY: `parsed` is a valid place for the call's result.
    let status = unsafe {
        SecRequirement::create_with_string(&text, SecCSFlags::empty(), NonNull::from(&mut parsed))
    };
    let Some(parsed) = (status == 0).then_some(NonNull::new(parsed)).flatten() else {
        return false;
    };
    // SAFETY: on success the call returned a retained requirement.
    let parsed = unsafe { CFRetained::from_raw(parsed) };
    // SAFETY: both objects are valid for the call.
    unsafe { code.check_validity(SecCSFlags::empty(), Some(&parsed)) == 0 }
}

/// The file `code` runs: its main executable, which for an app is the
/// program inside the bundle rather than the bundle itself.
fn path_of(code: &SecCode) -> Option<std::path::PathBuf> {
    let mut on_disk: *const SecStaticCode = std::ptr::null();
    // SAFETY: `on_disk` is a valid place for the call's result.
    let status = unsafe { code.copy_static_code(SecCSFlags::empty(), NonNull::from(&mut on_disk)) };
    let on_disk = (status == 0)
        .then_some(NonNull::new(on_disk.cast_mut()))
        .flatten()?;
    // SAFETY: on success the call returned a retained static code object.
    let on_disk = unsafe { CFRetained::from_raw(on_disk) };
    main_executable(&on_disk)
}

/// The main executable `on_disk` names in its signature.
fn main_executable(on_disk: &SecStaticCode) -> Option<std::path::PathBuf> {
    let information = signing_information(on_disk)?;
    // SAFETY: the key is the framework's own constant string.
    let key: &CFString = unsafe { kSecCodeInfoMainExecutable };
    let url = information.get(key)?.downcast::<CFURL>().ok()?;
    url.to_file_path()
}

/// What `on_disk`'s signature says of it.
fn signing_information(
    on_disk: &SecStaticCode,
) -> Option<CFRetained<CFDictionary<CFString, CFType>>> {
    let mut information: *const CFDictionary = std::ptr::null();
    // SAFETY: `information` is a valid place for the call's result.
    let status = unsafe {
        SecCode::copy_signing_information(
            on_disk,
            SecCSFlags::empty(),
            NonNull::from(&mut information),
        )
    };
    let information = (status == 0)
        .then_some(NonNull::new(information.cast_mut()))
        .flatten()?;
    // SAFETY: on success the call returned a retained dictionary, whose keys
    // are the framework's strings and whose values are CF objects.
    Some(unsafe { CFRetained::cast_unchecked(CFRetained::from_raw(information)) })
}

/// Whether `found` and `expected` name the same file.
fn same_file(found: &Path, expected: &Path) -> bool {
    match (
        std::fs::canonicalize(found),
        std::fs::canonicalize(expected),
    ) {
        (Ok(found), Ok(expected)) => found == expected,
        _ => false,
    }
}

#[cfg(test)]
#[path = "peer_tests.rs"]
mod tests;

//! A file's extended attributes, read and written by descriptor within
//! bounds checked before anything is allocated for them.
#![allow(
    unsafe_code,
    reason = "FFI boundary: macOS extended-attribute calls have no safe binding"
)]
use crate::error::{Code, Error, Result};
use std::{
    ffi::CString,
    io,
    os::fd::{AsFd, AsRawFd},
};

/// The longest attribute name macOS allows (`XATTR_MAXNAMELEN`).
pub const NAME: usize = 127;

/// How much of a file's extended attributes may be read.
#[derive(Clone, Copy, Debug)]
pub struct Bounds {
    /// The most attributes.
    pub count: usize,
    /// The largest one value.
    pub value: usize,
    /// The most bytes of names and values together.
    pub total: usize,
}

/// One extended attribute, its value written as base64.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Attribute {
    pub name: String,
    #[serde(with = "base64_value")]
    pub value: Vec<u8>,
}

mod base64_value {
    use base64::{Engine, engine::general_purpose::STANDARD};
    use serde::{Deserialize, Deserializer, Serializer, de::Error};

    pub(super) fn serialize<S: Serializer>(value: &[u8], to: S) -> Result<S::Ok, S::Error> {
        to.serialize_str(&STANDARD.encode(value))
    }

    pub(super) fn deserialize<'de, D: Deserializer<'de>>(from: D) -> Result<Vec<u8>, D::Error> {
        STANDARD
            .decode(String::deserialize(from)?)
            .map_err(D::Error::custom)
    }
}

fn too_large(what: &str) -> Error {
    Error::new(
        Code::BadRequest,
        format!("The file's extended attributes are {what}"),
    )
}

/// Every extended attribute of the open file `file` macOS shows, sorted by
/// name.
///
/// # Errors
///
/// Fails when they cannot be read, go past `bounds`, have a name that is
/// not UTF-8, or change while being read.
pub fn of(file: &impl AsFd, bounds: Bounds) -> Result<Vec<Attribute>> {
    let fd = file.as_fd().as_raw_fd();
    // SAFETY: a live descriptor; a null buffer of length 0 asks for the size.
    let size = unsafe { libc::flistxattr(fd, std::ptr::null_mut(), 0, 0) };
    let size = usize::try_from(size).map_err(|_| io::Error::last_os_error())?;
    if size > bounds.total {
        return Err(too_large("too many"));
    }
    let mut names = vec![0_u8; size];
    if size > 0 {
        // SAFETY: the buffer is writable for the length passed.
        let read = unsafe { libc::flistxattr(fd, names.as_mut_ptr().cast(), names.len(), 0) };
        if usize::try_from(read).ok() != Some(size) {
            return Err(Error::new(
                Code::Io,
                "The file's extended attributes changed while being read",
            ));
        }
    }
    let mut total = names.len();
    let mut attributes = Vec::new();
    for name in names
        .split(|byte| *byte == 0)
        .filter(|name| !name.is_empty())
    {
        if attributes.len() == bounds.count {
            return Err(too_large("too many"));
        }
        let text = std::str::from_utf8(name).map_err(|_| {
            Error::new(
                Code::BadRequest,
                "An extended attribute's name is not UTF-8",
            )
        })?;
        let value = value(fd, text, bounds.value, &mut total, bounds.total)?;
        attributes.push(Attribute {
            name: text.to_owned(),
            value,
        });
    }
    attributes.sort();
    Ok(attributes)
}

/// The value of `name` on `fd`, counted into `total`.
fn value(fd: i32, name: &str, largest: usize, total: &mut usize, most: usize) -> Result<Vec<u8>> {
    let named = c_name(name)?;
    // SAFETY: a live descriptor and a NUL-terminated name; a null buffer
    // asks for the size.
    let size = unsafe { libc::fgetxattr(fd, named.as_ptr(), std::ptr::null_mut(), 0, 0, 0) };
    let size = usize::try_from(size).map_err(|_| io::Error::last_os_error())?;
    *total = total.saturating_add(size);
    if size > largest || *total > most {
        return Err(too_large("too large"));
    }
    let mut value = vec![0_u8; size];
    // SAFETY: the buffer is writable for the length passed. A short read
    // (as resource forks allow) is refused below, never kept truncated.
    let read = unsafe {
        libc::fgetxattr(
            fd,
            named.as_ptr(),
            value.as_mut_ptr().cast(),
            value.len(),
            0,
            0,
        )
    };
    if usize::try_from(read).ok() != Some(size) {
        return Err(Error::new(
            Code::Io,
            "An extended attribute changed while being read",
        ));
    }
    Ok(value)
}

/// Set `attribute` on the open file `file`, creating it or replacing it.
///
/// # Errors
///
/// Fails when the name is not one macOS allows, or the write fails.
pub fn set(file: &impl AsFd, attribute: &Attribute) -> Result<()> {
    let named = c_name(&attribute.name)?;
    // SAFETY: a live descriptor, a NUL-terminated name, and a buffer
    // readable for the length passed.
    let set = unsafe {
        libc::fsetxattr(
            file.as_fd().as_raw_fd(),
            named.as_ptr(),
            attribute.value.as_ptr().cast(),
            attribute.value.len(),
            0,
            0,
        )
    };
    if set != 0 {
        return Err(io::Error::last_os_error().into());
    }
    Ok(())
}

fn c_name(name: &str) -> Result<CString> {
    if name.is_empty() || name.len() > NAME {
        return Err(Error::new(
            Code::BadRequest,
            "An extended attribute's name is empty or too long",
        ));
    }
    CString::new(name)
        .map_err(|_| Error::new(Code::BadRequest, "An extended attribute's name holds NUL"))
}

#[cfg(test)]
#[path = "xattr_tests.rs"]
mod tests;

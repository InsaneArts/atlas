//! A file's extended attributes, read and restored by descriptor within a
//! bound, leaving alone those macOS keeps itself.
use super::{Result, open_regular, problem};
use base64::{Engine, engine::general_purpose::STANDARD};
use std::{ffi::CString, fs, os::fd::AsRawFd, path::Path};

fn attribute_name(name: &[u8]) -> Result<CString> {
    CString::new(name).map_err(|_| problem("Invalid attribute name"))
}

/// The most extended-attribute data (names and values) kept for one file.
const MAX_ATTRIBUTES: usize = 16 * 1024;

#[allow(unsafe_code, reason = "FFI: bounded extended-attribute reads")]
pub(super) fn attributes(file: &fs::File) -> Result<std::collections::BTreeMap<String, String>> {
    // Sizes are read first so the limit holds before anything is allocated,
    // and each read must return exactly that size: a short read (as resource
    // forks allow) is refused, never kept as a truncated value.
    // SAFETY: a live descriptor; a null buffer of length 0 asks for the size.
    let size = unsafe { libc::flistxattr(file.as_raw_fd(), std::ptr::null_mut(), 0, 0) };
    if size < 0 {
        return Err(std::io::Error::last_os_error().into());
    }

    if size as usize > MAX_ATTRIBUTES {
        return Err(problem(
            "Extended attributes exceed the supported 16 KiB limit",
        ));
    }

    let mut names = vec![0_u8; size as usize];

    if size > 0 {
        // SAFETY: the buffer is writable for the length passed.
        let read = unsafe {
            libc::flistxattr(file.as_raw_fd(), names.as_mut_ptr().cast(), names.len(), 0)
        };
        if read != size {
            return Err(problem("Extended attributes changed while being read"));
        }
    }

    let mut result = std::collections::BTreeMap::new();
    let mut total = names.len();

    for name in names
        .split(|byte| *byte == 0)
        .filter(|name| !name.is_empty())
    {
        let text = std::str::from_utf8(name)
            .map_err(|_| problem("Unsupported attribute name encoding"))?;
        let name = attribute_name(name)?;

        // SAFETY: a live descriptor and NUL-terminated name; null buffer, size only.
        let size = unsafe {
            libc::fgetxattr(
                file.as_raw_fd(),
                name.as_ptr(),
                std::ptr::null_mut(),
                0,
                0,
                0,
            )
        };

        if size < 0 {
            return Err(std::io::Error::last_os_error().into());
        }

        total += size as usize;

        if total > MAX_ATTRIBUTES {
            return Err(problem(
                "Extended attributes exceed the supported 16 KiB limit",
            ));
        }

        let mut value = vec![0_u8; size as usize];

        // SAFETY: the buffer is writable for the length passed.
        let read = unsafe {
            libc::fgetxattr(
                file.as_raw_fd(),
                name.as_ptr(),
                value.as_mut_ptr().cast(),
                value.len(),
                0,
                0,
            )
        };

        if read != size {
            return Err(problem("Extended attribute changed while being read"));
        }

        result.insert(text.into(), STANDARD.encode(value));
    }

    Ok(result)
}

/// Extended attributes macOS keeps itself. Setting or removing one is
/// silently ignored (a new file takes the provenance of the process that
/// made it), so they are never restored and never compared.
const SYSTEM_ATTRIBUTES: &[&str] = &["com.apple.provenance"];

/// Whether macOS, not the file's owner, decides the attribute `name`.
fn system_attribute(name: &str) -> bool {
    SYSTEM_ATTRIBUTES.contains(&name)
}

/// The attributes of `all` a file's owner can set, which restores and
/// comparisons look at.
pub(in crate::environment) fn owned_attributes(
    all: &std::collections::BTreeMap<String, String>,
) -> impl Iterator<Item = (&String, &String)> {
    all.iter().filter(|(name, _)| !system_attribute(name))
}

/// Where macOS marks a file as downloaded. It describes the bytes it came
/// with, so new contents never take it over from the file they replace.
pub(in crate::environment) const QUARANTINE: &str = "com.apple.quarantine";

#[allow(unsafe_code, reason = "FFI: extended-attribute writes by descriptor")]
pub(in crate::environment) fn restore_attributes(
    path: &Path,
    expected: &std::collections::BTreeMap<String, String>,
) -> Result<()> {
    let file = open_regular(path, false)?.ok_or_else(|| problem("Prepared restore is missing"))?;
    let current = attributes(&file)?;

    for key in current
        .keys()
        .filter(|key| !expected.contains_key(*key) && !system_attribute(key))
    {
        let name = attribute_name(key.as_bytes())?;
        // SAFETY: a live descriptor and NUL-terminated name.
        let status = unsafe { libc::fremovexattr(file.as_raw_fd(), name.as_ptr(), 0) };
        if status != 0 {
            return Err(attribute_refused());
        }
    }

    let mut total = 0;

    for (key, value) in expected {
        let name = attribute_name(key.as_bytes())?;
        let value = STANDARD
            .decode(value)
            .map_err(|_| problem("Invalid stored attribute"))?;
        total += key.len() + value.len() + 1;

        if total > MAX_ATTRIBUTES {
            return Err(problem("Stored attributes exceed their limit"));
        }

        if current.get(key) == expected.get(key) || system_attribute(key) {
            continue;
        }

        // SAFETY: a live descriptor, NUL-terminated name, and a value buffer
        // readable for the length passed.
        let status = unsafe {
            libc::fsetxattr(
                file.as_raw_fd(),
                name.as_ptr(),
                value.as_ptr().cast(),
                value.len(),
                0,
                0,
            )
        };

        if status != 0 {
            return Err(attribute_refused());
        }
    }

    file.sync_all()?;

    if !owned_attributes(&attributes(&file)?).eq(owned_attributes(expected)) {
        return Err(problem("Extended attributes could not be restored exactly"));
    }

    Ok(())
}

/// The refusal of an attribute call that just failed, with its errno.
fn attribute_refused() -> atlas_core::error::Error {
    let cause = std::io::Error::last_os_error();
    problem("Cannot restore an extended attribute; destination was preserved").with_source(cause)
}

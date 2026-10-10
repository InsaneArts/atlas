//! Bounded file reads, private staging and descriptor-relative installation.
// FFI boundary: bounded extended-attribute reads and renameatx_np.
#![allow(unsafe_code)]
use super::{
    Result, STATE,
    model::{Fingerprint, Node},
    problem,
};
use base64::{Engine, engine::general_purpose::STANDARD};
use nix::{
    errno::Errno,
    fcntl::{AtFlags, OFlag, openat, renameat},
    sys::stat::{Mode, mkdirat},
    unistd::{Gid, UnlinkatFlags, fchown, linkat, unlinkat},
};
use sha2::{Digest, Sha256};
use std::os::macos::fs::MetadataExt as _;
use std::{
    ffi::{CStr, CString},
    fs,
    io::{Read, Write},
    os::{
        fd::AsRawFd,
        unix::{
            ffi::OsStrExt,
            fs::{MetadataExt, PermissionsExt},
        },
    },
    path::{Component, Path, PathBuf},
    process::Command,
};
pub(super) fn read_limited(path: &Path, limit: u64) -> Result<Vec<u8>> {
    let file = open_regular(path, false)?.ok_or_else(|| problem("Environment file is missing"))?;
    read_file(file, limit)
}

pub(super) fn read_file(file: fs::File, limit: u64) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    if !file.metadata()?.is_file() {
        return Err(problem(
            "Environment files must be regular files, not links or devices",
        ));
    }
    file.take(limit + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(problem("Environment file exceeds its size limit"));
    }
    Ok(bytes)
}

pub(super) fn open_regular(path: &Path, create: bool) -> Result<Option<fs::File>> {
    let Some(parent) = directory(
        path.parent().ok_or_else(|| problem("Missing parent"))?,
        false,
    )?
    else {
        return Ok(None);
    };
    let name = component(
        path.file_name()
            .ok_or_else(|| problem("Missing file name"))?,
    )?;
    let flags = if create {
        OFlag::O_RDWR | OFlag::O_CREAT
    } else {
        OFlag::O_RDONLY
    };
    let opened = openat(
        &parent,
        name.as_c_str(),
        flags | OFlag::O_NOFOLLOW | OFlag::O_NONBLOCK | OFlag::O_CLOEXEC,
        Mode::from_bits_truncate(0o600),
    );
    let file = match opened {
        Ok(fd) => fs::File::from(fd),
        Err(Errno::ENOENT) => return Ok(None),
        Err(error) => return Err(std::io::Error::from(error).into()),
    };
    if !file.metadata()?.is_file() {
        return Err(problem("Expected a regular environment file"));
    }
    Ok(Some(file))
}

pub(super) fn snapshot(path: &Path) -> Result<Option<(Fingerprint, Vec<u8>)>> {
    let Some(file) = open_regular(path, false)? else {
        return Ok(None);
    };
    let metadata = file.metadata()?;
    let acl = descriptor_acl(&file)?;
    let xattrs = attributes(&file)?;
    let flags = metadata.st_flags();
    // Replacing these files could silently discard information or affect aliases.
    if metadata.uid() != nix::unistd::getuid().as_raw()
        || metadata.nlink() != 1
        || acl
        || flags != 0
        || metadata.mode() & 0o7000 != 0
    {
        return Err(problem(format!(
            "Unsupported destination metadata (owner={}, links={}, ACL={}, flags={}, mode={:o}); it was not changed",
            metadata.uid(),
            metadata.nlink(),
            acl,
            flags,
            metadata.mode() & 0o7777
        )));
    }
    let bytes = read_file(file.try_clone()?, super::MAX_FILE)?;
    let after = file.metadata()?;
    if metadata.len() != after.len()
        || metadata.mtime() != after.mtime()
        || metadata.mtime_nsec() != after.mtime_nsec()
        || metadata.ctime() != after.ctime()
        || metadata.ctime_nsec() != after.ctime_nsec()
    {
        return Err(problem("Environment file changed while being read"));
    }
    Ok(Some((
        Fingerprint {
            node: node(&metadata),
            sha256: digest(&bytes),
            size: bytes.len() as u64,
            mode: metadata.mode() & 0o7777,
            uid: metadata.uid(),
            gid: metadata.gid(),
            acl,
            xattrs,
            flags,
        },
        bytes,
    )))
}

pub(super) fn fingerprint(path: &Path) -> Result<Option<Fingerprint>> {
    Ok(snapshot(path)?.map(|(fingerprint, _)| fingerprint))
}

pub(super) fn node(metadata: &fs::Metadata) -> Node {
    Node {
        device: metadata.dev(),
        inode: metadata.ino(),
    }
}

fn component(name: &std::ffi::OsStr) -> Result<std::ffi::CString> {
    std::ffi::CString::new(name.as_bytes()).map_err(|_| problem("Invalid filesystem component"))
}

fn attributes(file: &fs::File) -> Result<std::collections::BTreeMap<String, String>> {
    const LIMIT: usize = 16384;
    // Sizes are read first so the limit holds before anything is allocated,
    // and each read must return exactly that size: a short read (as resource
    // forks allow) is refused, never kept as a truncated value.
    // SAFETY: a live descriptor; a null buffer of length 0 asks for the size.
    let size = unsafe { libc::flistxattr(file.as_raw_fd(), std::ptr::null_mut(), 0, 0) };
    if size < 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    if size as usize > LIMIT {
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
        let name = std::ffi::CString::new(name).unwrap();
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
        if total > LIMIT {
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

pub(super) fn restore_attributes(
    path: &Path,
    expected: &std::collections::BTreeMap<String, String>,
) -> Result<()> {
    let file = open_regular(path, false)?.ok_or_else(|| problem("Prepared restore is missing"))?;
    let current = attributes(&file)?;
    for key in current.keys().filter(|key| !expected.contains_key(*key)) {
        let name = std::ffi::CString::new(key.as_bytes())
            .map_err(|_| problem("Invalid attribute name"))?;
        // SAFETY: a live descriptor and NUL-terminated name.
        let status = unsafe { libc::fremovexattr(file.as_raw_fd(), name.as_ptr(), 0) };
        if status != 0 {
            return Err(problem(
                "Cannot restore an extended attribute; destination was preserved",
            ));
        }
    }
    let mut total = 0;
    for (key, value) in expected {
        let name = std::ffi::CString::new(key.as_bytes())
            .map_err(|_| problem("Invalid attribute name"))?;
        let value = STANDARD
            .decode(value)
            .map_err(|_| problem("Invalid stored attribute"))?;
        total += key.len() + value.len() + 1;
        if total > 16384 {
            return Err(problem("Stored attributes exceed their limit"));
        }
        if current.get(key) == expected.get(key) {
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
            return Err(problem(
                "Cannot restore an extended attribute; destination was preserved",
            ));
        }
    }
    file.sync_all()?;
    if attributes(&file)? != *expected {
        return Err(problem("Extended attributes could not be restored exactly"));
    }
    Ok(())
}
pub(super) fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
pub(super) fn relative_target(path: &Path) -> Result<()> {
    relative_path(path, false)
}

pub(super) fn relative_directory(path: &Path) -> Result<()> {
    relative_path(path, true)
}

fn relative_path(path: &Path, directory: bool) -> Result<()> {
    let normalized = path
        .components()
        .map(|c| c.as_os_str().to_string_lossy().to_ascii_lowercase())
        .collect::<Vec<_>>()
        .join("/");
    if path.as_os_str().is_empty()
        || path
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
        || normalized == STATE
        || normalized.starts_with(&format!("{STATE}/"))
        || (!directory && STATE.starts_with(&format!("{normalized}/")))
    {
        return Err(problem(
            "Targets must be relative files inside the helper home, outside provisioning state",
        ));
    }
    Ok(())
}
pub(super) fn unique_targets<'a>(targets: impl Iterator<Item = &'a Path>) -> Result<()> {
    let mut seen = Vec::<String>::new();
    for path in targets {
        relative_target(path)?;
        // Configuration destinations use portable ASCII names. Conservatively
        // reject case aliases even on case-sensitive volumes.
        let text = path
            .to_str()
            .filter(|s| s.is_ascii())
            .ok_or_else(|| problem("Environment target names must be ASCII"))?;
        let key = Path::new(text)
            .components()
            .map(|c| c.as_os_str().to_string_lossy().to_ascii_lowercase())
            .collect::<Vec<_>>()
            .join("/");
        if seen.iter().any(|s| {
            s == &key || s.starts_with(&(key.clone() + "/")) || key.starts_with(&(s.clone() + "/"))
        }) {
            return Err(problem(
                "Environment targets overlap or differ only by case",
            ));
        }
        seen.push(key);
    }
    Ok(())
}
pub(super) fn has_acl(path: &Path) -> Result<bool> {
    crate::acl::present(path).map_err(|error| match error.kind() {
        std::io::ErrorKind::InvalidData => problem("Invalid filesystem ACL"),
        _ => error.into(),
    })
}

pub(super) fn remove_inherited_acl(path: &Path) -> Result<()> {
    if !Command::new("/bin/chmod")
        .arg("-N")
        .arg(path)
        .status()?
        .success()
    {
        return Err(problem("Could not make provisioning data private"));
    }
    Ok(())
}
pub(super) fn destination(home: &Path, relative: &Path) -> Result<PathBuf> {
    relative_target(relative)?;
    checked_path(home, relative)
}

pub(super) fn directory_destination(home: &Path, relative: &Path) -> Result<PathBuf> {
    relative_directory(relative)?;
    checked_path(home, relative)
}

fn checked_path(home: &Path, relative: &Path) -> Result<PathBuf> {
    let mut path = home.to_owned();
    for part in relative.components() {
        path.push(part);
        match fs::symlink_metadata(&path) {
            Ok(meta) if meta.file_type().is_symlink() => {
                return Err(problem(format!("Refusing symlink: {}", path.display())));
            }
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
    }
    Ok(path)
}
pub(super) fn open_directory(path: &Path) -> Result<fs::File> {
    directory(path, true)?.ok_or_else(|| problem("Missing provisioning directory"))
}

pub(super) fn directory(path: &Path, create: bool) -> Result<Option<fs::File>> {
    if !path.is_absolute() {
        return Err(problem("Provisioning paths must be absolute"));
    }
    let open = |directory: &fs::File, name: &CStr| {
        openat(
            directory,
            name,
            OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
            Mode::empty(),
        )
    };
    let mut directory = fs::File::open("/")?;
    for component in path.components() {
        let Component::Normal(name) = component else {
            if component == Component::RootDir {
                continue;
            }
            return Err(problem("Invalid provisioning directory"));
        };
        let name = CString::new(name.as_bytes()).map_err(|_| problem("Invalid path"))?;
        let mut opened = open(&directory, &name);
        if create && matches!(opened, Err(Errno::ENOENT)) {
            match mkdirat(&directory, name.as_c_str(), Mode::from_bits_truncate(0o700)) {
                Ok(()) | Err(Errno::EEXIST) => {}
                Err(error) => return Err(std::io::Error::from(error).into()),
            }
            opened = open(&directory, &name);
        }
        directory = match opened {
            Ok(fd) => fs::File::from(fd),
            Err(Errno::ENOENT) => return Ok(None),
            Err(error) => return Err(std::io::Error::from(error).into()),
        };
    }
    Ok(Some(directory))
}
pub(super) fn atomic_write(path: &Path, bytes: &[u8], mode: u32) -> Result<()> {
    atomic_install(path, bytes, mode, false)
}
pub(super) fn atomic_install(
    path: &Path,
    bytes: &[u8],
    mode: u32,
    create_only: bool,
) -> Result<()> {
    let parent = open_directory(
        path.parent()
            .ok_or_else(|| problem("Missing parent directory"))?,
    )?;
    let metadata = parent.metadata()?;
    if metadata.uid() != nix::unistd::getuid().as_raw()
        || metadata.mode() & 0o022 != 0
        || writable_acl(&parent)?
    {
        return Err(problem(
            "Installation parent must be owned by the current user and have no shared write access",
        ));
    }
    let name = CString::new(
        path.file_name()
            .ok_or_else(|| problem("Missing file name"))?
            .as_bytes(),
    )
    .map_err(|_| problem("Invalid path"))?;
    let temporary = CString::new(format!(".understudy-{}", crate::control::token())).unwrap();
    let mut file = fs::File::from(
        openat(
            &parent,
            temporary.as_c_str(),
            OFlag::O_WRONLY | OFlag::O_CREAT | OFlag::O_EXCL | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
            Mode::from_bits_truncate(mode as libc::mode_t),
        )
        .map_err(std::io::Error::from)?,
    );
    let discard = || unlinkat(&parent, temporary.as_c_str(), UnlinkatFlags::NoRemoveDir);
    let result = (|| -> Result<()> {
        clear_acl(&file)?;
        file.set_permissions(fs::Permissions::from_mode(mode))?;
        file.write_all(bytes)?;
        file.sync_all()?;
        if create_only {
            // Linking the new inode refuses an existing destination atomically.
            linkat(
                &parent,
                temporary.as_c_str(),
                &parent,
                name.as_c_str(),
                AtFlags::empty(),
            )
            .map_err(std::io::Error::from)?;
            let _ = discard();
        } else {
            renameat(&parent, temporary.as_c_str(), &parent, name.as_c_str())
                .map_err(std::io::Error::from)?;
        }
        parent.sync_all()?;
        Ok(())
    })();
    if result.is_err() {
        let _ = discard();
    }
    result
}

/// Move a recorded prepared inode into place. No-replace is atomic for creates.
pub(super) fn install_prepared(source: &Path, target: &Path, create_only: bool) -> Result<()> {
    if source.parent() != target.parent() {
        return Err(problem(
            "Prepared replacement must share its destination directory",
        ));
    }
    let parent = directory(target.parent().unwrap(), false)?
        .ok_or_else(|| problem("Destination parent disappeared"))?;
    installation_parent(&parent)?;
    let source = component(source.file_name().unwrap())?;
    let target = component(target.file_name().unwrap())?;
    // SAFETY: a live directory descriptor and two NUL-terminated single
    // components; RENAME_EXCL makes the rename refuse an existing target.
    let status = unsafe {
        unsafe extern "C" {
            fn renameatx_np(
                from_fd: libc::c_int,
                from: *const libc::c_char,
                to_fd: libc::c_int,
                to: *const libc::c_char,
                flags: libc::c_uint,
            ) -> libc::c_int;
        }
        renameatx_np(
            parent.as_raw_fd(),
            source.as_ptr(),
            parent.as_raw_fd(),
            target.as_ptr(),
            if create_only { libc::RENAME_EXCL } else { 0 },
        )
    };
    if status != 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    parent.sync_all()?;
    Ok(())
}

pub(super) fn set_group(path: &Path, gid: u32) -> Result<()> {
    let file = open_regular(path, false)?.ok_or_else(|| problem("Prepared file is missing"))?;
    if file.metadata()?.gid() != gid {
        fchown(&file, None, Some(Gid::from_raw(gid))).map_err(std::io::Error::from)?;
        file.sync_all()?;
    }
    Ok(())
}

pub(super) fn unlink(path: &Path, is_directory: bool) -> Result<()> {
    let parent = directory(
        path.parent().ok_or_else(|| problem("Missing parent"))?,
        false,
    )?
    .ok_or_else(|| problem("Parent directory disappeared"))?;
    installation_parent(&parent)?;
    let name = component(path.file_name().ok_or_else(|| problem("Missing name"))?)?;
    let kind = if is_directory {
        UnlinkatFlags::RemoveDir
    } else {
        UnlinkatFlags::NoRemoveDir
    };
    unlinkat(&parent, name.as_c_str(), kind).map_err(std::io::Error::from)?;
    parent.sync_all()?;
    Ok(())
}

pub(super) fn private_directory(path: &Path) -> Result<fs::File> {
    if let Some(dir) = directory(path, false)? {
        validate_private(&dir, true)?;
        return Ok(dir);
    }
    open_directory(path.parent().ok_or_else(|| problem("Missing parent"))?)?;
    create_private_directory(path)
}

/// Unlike the idempotent state-directory opener, this never claims an existing
/// user directory as one created by the transaction.
pub(super) fn create_private_directory(path: &Path) -> Result<fs::File> {
    let parent = directory(
        path.parent().ok_or_else(|| problem("Missing parent"))?,
        false,
    )?
    .ok_or_else(|| problem("Parent directory disappeared"))?;
    installation_parent(&parent)?;
    let name = component(path.file_name().ok_or_else(|| problem("Missing name"))?)?;
    mkdirat(&parent, name.as_c_str(), Mode::from_bits_truncate(0o700))
        .map_err(std::io::Error::from)?;
    let dir = directory(path, false)?.ok_or_else(|| problem("New directory disappeared"))?;
    clear_acl(&dir)?;
    dir.set_permissions(fs::Permissions::from_mode(0o700))?;
    validate_private(&dir, true)?;
    dir.sync_all()?;
    parent.sync_all()?;
    Ok(dir)
}

pub(super) fn validate_private(file: &fs::File, directory: bool) -> Result<()> {
    let metadata = file.metadata()?;
    if metadata.uid() != nix::unistd::getuid().as_raw()
        || (directory && !metadata.is_dir())
        || (!directory && !metadata.is_file())
        || metadata.mode() & 0o7777 != if directory { 0o700 } else { 0o600 }
        || (!directory && metadata.nlink() != 1)
        || descriptor_acl(file)?
    {
        return Err(problem(
            "Provisioning state must be private and owned by the current user",
        ));
    }
    Ok(())
}

pub(super) fn installation_parent(directory: &fs::File) -> Result<()> {
    let metadata = directory.metadata()?;
    if metadata.uid() != nix::unistd::getuid().as_raw()
        || metadata.mode() & 0o022 != 0
        || writable_acl(directory)?
    {
        return Err(problem(
            "Installation parent must be owned by the current user and have no shared write access",
        ));
    }
    Ok(())
}

/// Only called for an inode this operation just created. Existing selected
/// files with ACLs are refused; private files never inherit broader access.
fn clear_acl(file: &fs::File) -> Result<()> {
    Ok(crate::acl::clear(file)?)
}

fn descriptor_acl(file: &fs::File) -> Result<bool> {
    Ok(crate::acl::Acl::of_file(file)?
        .map(|acl| acl.entries())
        .transpose()?
        .is_some_and(|entries| !entries.is_empty()))
}
pub(super) fn allow_helper(path: &Path, uid: u32, executable: bool) -> Result<()> {
    let identity = Command::new("/usr/bin/id")
        .args(["-un", &uid.to_string()])
        .output()?;
    let name =
        String::from_utf8(identity.stdout).map_err(|_| problem("Invalid helper short name"))?;
    let name = name.trim();
    if !identity.status.success()
        || name.is_empty()
        || !name
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
    {
        return Err(problem("Could not resolve helper UID to an ACL identity"));
    }
    let rights = if executable {
        "read,execute,readattr,readsecurity"
    } else {
        "read,readattr,readsecurity"
    };
    let output = Command::new("/bin/chmod")
        .args(["+a", &format!("user:{name} allow {rights}")])
        .arg(path)
        .output()?;
    if !output.status.success() {
        return Err(problem("Could not grant the helper access to staged files"));
    }
    Ok(())
}

fn writable_acl(directory: &fs::File) -> Result<bool> {
    Ok(crate::acl::Acl::of_file(directory)?
        .map(|acl| acl.entries())
        .transpose()?
        .is_some_and(|entries| {
            entries
                .iter()
                .any(|entry| entry.allow && entry.mask & crate::acl::WRITE_RIGHTS != 0)
        }))
}

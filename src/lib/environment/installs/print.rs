//! What a bundle is, as far as removing it needs: its folder's own node,
//! mode and owner, and a digest over every entry inside, walked through
//! folders held open by descriptor and never through a link, so what is
//! printed is exactly what was opened.
use super::super::{model::Node, problem};
use nix::{
    dir::Dir,
    fcntl::{AtFlags, OFlag, openat},
    sys::stat::{FileStat, Mode, SFlag, fstat, fstatat},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    ffi::CStr,
    fs,
    io::Read,
    os::{fd::AsFd, unix::ffi::OsStrExt},
    time::Instant,
};

/// The most entries and bytes a bundle may have and still be printed.
pub(super) const MAX_ENTRIES: u64 = 100_000;
pub(super) const MAX_BYTES: u64 = 8 * 1024 * 1024 * 1024;

/// What a bundle is, as far as removing it needs.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(in crate::environment) struct BundlePrint {
    /// The bundle folder itself.
    pub root: Node,
    /// The bundle folder's own permissions and owner.
    pub mode: u32,
    pub uid: u32,
    pub gid: u32,
    /// SHA-256 over every entry inside, sorted by path: its path, type,
    /// permissions, owner and inode, and for files and links their size,
    /// modification time and a link's target. Directory times and extended
    /// attributes are left out: macOS changes those itself when an app is
    /// opened.
    pub tree: String,
    pub entries: u64,
    pub bytes: u64,
    /// SHA-256 of Contents/Info.plist and of the main executable.
    pub info: String,
    pub executable: Option<String>,
}

/// How large a bundle may be, and how long printing it may take, and still
/// be printed.
#[derive(Clone, Copy)]
pub(in crate::environment) struct Limits {
    pub entries: u64,
    pub bytes: u64,
    /// When printing must have finished by; none for no deadline.
    pub until: Option<Instant>,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            entries: MAX_ENTRIES,
            bytes: MAX_BYTES,
            until: None,
        }
    }
}

/// Why a bundle could not be printed.
#[derive(Debug)]
pub(in crate::environment) enum Unprinted {
    /// Larger than the limits allow.
    TooLarge,
    /// Not done by the deadline.
    TooSlow,
    /// It changed while it was read, or could not be read.
    Unreadable(String),
}

impl Unprinted {
    fn read(error: impl std::fmt::Display) -> Self {
        Self::Unreadable(error.to_string())
    }
}

/// The print of the bundle folder open as `root`, whose main executable is
/// called `executable`.
pub(in crate::environment) fn print_at(
    root: &fs::File,
    executable: Option<&str>,
    limits: Limits,
) -> std::result::Result<BundlePrint, Unprinted> {
    let metadata = fstat(root).map_err(Unprinted::read)?;
    if SFlag::from_bits_truncate(metadata.st_mode) & SFlag::S_IFMT != SFlag::S_IFDIR {
        return Err(Unprinted::Unreadable("An app bundle is a folder".into()));
    }

    let mut lines: Vec<(Vec<u8>, Vec<u8>)> = Vec::new();
    let (mut count, mut bytes) = (0u64, 0u64);
    let opened =
        Dir::from_fd(root.try_clone().map_err(Unprinted::read)?.into()).map_err(Unprinted::read)?;
    let mut folders = vec![(Vec::new(), opened)];
    while let Some((relative, mut folder)) = folders.pop() {
        if limits.until.is_some_and(|until| Instant::now() >= until) {
            return Err(Unprinted::TooSlow);
        }
        // A folder that cannot be read to its end is never printed: a walk
        // that skipped an entry would print less than is there.
        let mut names: Vec<std::ffi::CString> = Vec::new();
        for entry in folder.iter() {
            let name = entry.map_err(Unprinted::read)?.file_name().to_owned();
            if name.as_bytes() != b"." && name.as_bytes() != b".." {
                names.push(name);
            }
        }
        let mut looked: Vec<(std::ffi::CString, FileStat)> = Vec::new();
        for name in names {
            count += 1;
            if count > limits.entries {
                return Err(Unprinted::TooLarge);
            }
            let stat = fstatat(
                folder.as_fd(),
                name.as_c_str(),
                AtFlags::AT_SYMLINK_NOFOLLOW,
            )
            .map_err(Unprinted::read)?;
            let mut path = relative.clone();
            if !path.is_empty() {
                path.push(b'/');
            }
            path.extend_from_slice(name.as_bytes());

            let kind = SFlag::from_bits_truncate(stat.st_mode) & SFlag::S_IFMT;
            let mut line = format!(
                "{}:{:o}:{}:{}:{}",
                match kind {
                    SFlag::S_IFDIR => 'd',
                    SFlag::S_IFLNK => 'l',
                    SFlag::S_IFREG => 'f',
                    _ => 'o',
                },
                u32::from(stat.st_mode) & 0o7777,
                stat.st_uid,
                stat.st_gid,
                stat.st_ino,
            )
            .into_bytes();
            if kind == SFlag::S_IFREG || kind == SFlag::S_IFLNK {
                line.extend(
                    format!(":{}:{}.{}", stat.st_size, stat.st_mtime, stat.st_mtime_nsec).bytes(),
                );
            }
            if kind == SFlag::S_IFLNK {
                let target = nix::fcntl::readlinkat(folder.as_fd(), name.as_c_str())
                    .map_err(Unprinted::read)?;
                line.push(b':');
                line.extend(target.as_bytes());
            }
            if kind == SFlag::S_IFREG {
                bytes += u64::try_from(stat.st_size).unwrap_or(u64::MAX);
                if bytes > limits.bytes {
                    return Err(Unprinted::TooLarge);
                }
            }
            if kind == SFlag::S_IFDIR {
                let inner = Dir::openat(
                    folder.as_fd(),
                    name.as_c_str(),
                    OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
                    Mode::empty(),
                )
                .map_err(Unprinted::read)?;
                // The folder opened must be the one looked at.
                same(&stat, &fstat(inner.as_fd()).map_err(Unprinted::read)?)?;
                folders.push((path.clone(), inner));
            }
            lines.push((path, line));
            looked.push((name, stat));
        }
        // Every entry is looked at again once its folder was read: one that
        // was swapped, moved away or changed meanwhile leaves the print
        // unreadable, never taken for the bundle as it was.
        for (name, before) in &looked {
            let after = fstatat(
                folder.as_fd(),
                name.as_c_str(),
                AtFlags::AT_SYMLINK_NOFOLLOW,
            )
            .map_err(Unprinted::read)?;
            let unchanged = after.st_ino == before.st_ino
                && after.st_dev == before.st_dev
                && after.st_mode == before.st_mode
                && after.st_size == before.st_size
                && after.st_mtime == before.st_mtime
                && after.st_mtime_nsec == before.st_mtime_nsec;
            if !unchanged {
                return Err(Unprinted::Unreadable(
                    "The app changed while it was read".into(),
                ));
            }
        }
    }
    lines.sort();

    let mut tree = Sha256::new();
    for (path, line) in &lines {
        tree.update(path);
        tree.update([0]);
        tree.update(line);
        tree.update([b'\n']);
    }

    let contents = open_folder(root, c"Contents")?;
    let info = file_digest(&contents, c"Info.plist")?
        .ok_or_else(|| Unprinted::Unreadable("The app has no Contents/Info.plist".into()))?;
    let executable = match executable.and_then(|name| std::ffi::CString::new(name).ok()) {
        Some(name) => match open_folder(&contents, c"MacOS") {
            Ok(folder) => file_digest(&folder, &name)?,
            Err(_) => None,
        },
        None => None,
    };

    Ok(BundlePrint {
        root: Node {
            device: u64::try_from(metadata.st_dev).unwrap_or(0),
            inode: metadata.st_ino,
        },
        mode: u32::from(metadata.st_mode) & 0o7777,
        uid: metadata.st_uid,
        gid: metadata.st_gid,
        tree: hex(&tree.finalize()),
        entries: count,
        bytes,
        info,
        executable,
    })
}

/// Refuse a folder opened that is not the entry looked at.
fn same(looked: &FileStat, opened: &FileStat) -> std::result::Result<(), Unprinted> {
    if looked.st_ino == opened.st_ino && looked.st_dev == opened.st_dev {
        Ok(())
    } else {
        Err(Unprinted::Unreadable(
            "The app changed while it was read".into(),
        ))
    }
}

/// The folder `name` in `parent`, opened without following a link.
fn open_folder(parent: &impl AsFd, name: &CStr) -> std::result::Result<fs::File, Unprinted> {
    openat(
        parent.as_fd(),
        name,
        OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
        Mode::empty(),
    )
    .map(fs::File::from)
    .map_err(Unprinted::read)
}

/// The SHA-256 of the plain file `name` in `folder`, opened without
/// following a link; none when nothing plain is there.
fn file_digest(folder: &fs::File, name: &CStr) -> std::result::Result<Option<String>, Unprinted> {
    let opened = openat(
        folder.as_fd(),
        name,
        OFlag::O_RDONLY | OFlag::O_NOFOLLOW | OFlag::O_NONBLOCK | OFlag::O_CLOEXEC,
        Mode::empty(),
    );
    let mut file = match opened {
        Ok(fd) => fs::File::from(fd),
        Err(nix::errno::Errno::ENOENT | nix::errno::Errno::ELOOP) => return Ok(None),
        Err(error) => return Err(Unprinted::read(error)),
    };
    if !file.metadata().map_err(Unprinted::read)?.is_file() {
        return Ok(None);
    }
    let mut hasher = Sha256::new();
    let mut buffer = vec![0; 1 << 16];
    loop {
        let read = file.read(&mut buffer).map_err(Unprinted::read)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(Some(hex(&hasher.finalize())))
}

#[cfg(test)]
/// The print of the bundle folder at `path`, opened from `/` without
/// following a link anywhere on the way.
pub(in crate::environment) fn print(
    path: &std::path::Path,
    executable: Option<&str>,
    limits: Limits,
) -> std::result::Result<BundlePrint, Unprinted> {
    let root = super::super::files::directory(path, false)
        .map_err(|error| Unprinted::Unreadable(error.error))?
        .ok_or_else(|| Unprinted::Unreadable("The app is missing".into()))?;
    print_at(&root, executable, limits)
}

/// Why `unprinted` refuses an install, as the person reads it.
pub(super) fn refusal(unprinted: Unprinted) -> crate::error::Error {
    match unprinted {
        Unprinted::TooLarge | Unprinted::TooSlow => crate::error::Error::new(
            "input",
            "That app is too large for Atlas to check it later, so it was not installed",
        ),
        Unprinted::Unreadable(said) => problem(said),
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

//! The new bundle made by the installer itself, entry by entry from the
//! daemon's snapshot: each file read from the daemon's staged copy as
//! `atlas_core::stage::open_file` opens one and checked against its size
//! and digest, each written by the installer under a folder it holds open,
//! with modes it chooses. Nothing is extracted, so nothing lands outside.
use atlas_core::{
    error::{Code, Error, Result},
    fs_move::FileName,
    stage::{Kind, Snapshot},
};
use nix::{
    dir::Dir,
    fcntl::{OFlag, openat},
    sys::stat::{Mode, fchmod, mkdirat},
    unistd::{fchown, symlinkat},
};
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::{Read, Write},
    os::fd::{AsFd, OwnedFd},
    path::Path,
};

/// The bundle a snapshot must name.
pub(in crate::machine) const BUNDLE: &str = "Atlas.app";

/// The refusal of what cannot be installed.
fn refused(why: &str) -> Error {
    Error::new(
        Code::BadRequest,
        format!("This cannot be installed: {why}."),
    )
}

fn io(error: impl std::error::Error + Send + Sync + 'static) -> Error {
    Error::new(Code::Io, "The new bundle could not be copied in.").with_source(error)
}

/// Make `manifest`'s bundle as `name` in `into`, from the staged copy at
/// `source`. The caller holds `into`, a root-owned folder nothing else
/// writes; `root` says whether to make every entry root's own.
pub(in crate::machine) fn copy_in(
    manifest: &Snapshot,
    source: &Path,
    into: &File,
    name: &FileName,
    root: bool,
) -> Result<()> {
    manifest.check()?;
    if manifest.name != BUNDLE
        || !matches!(
            manifest.entries.first().map(|entry| &entry.kind),
            Some(Kind::Folder)
        )
    {
        return Err(refused("it is not one Atlas.app folder"));
    }
    mkdirat(
        into,
        name.as_c_str(),
        Mode::S_IRWXU | Mode::S_IRGRP | Mode::S_IXGRP | Mode::S_IROTH | Mode::S_IXOTH,
    )
    .map_err(io)?;
    let bundle = Dir::openat(into, name.as_c_str(), folder_flags(), Mode::empty()).map_err(io)?;
    settle(&bundle, 0o755, root)?;
    for entry in manifest.entries.iter().skip(1) {
        let Some((last, parents)) = entry.path.split_last() else {
            return Err(refused("an entry has no name"));
        };
        let parent = open_parent(&bundle, parents)?;
        let entry_name =
            FileName::new(last).ok_or_else(|| refused("a name is not a plain name"))?;
        match &entry.kind {
            Kind::Folder => {
                mkdirat(
                    &parent,
                    entry_name.as_c_str(),
                    Mode::S_IRWXU | Mode::S_IRGRP | Mode::S_IXGRP | Mode::S_IROTH | Mode::S_IXOTH,
                )
                .map_err(io)?;
                let made = Dir::openat(
                    &parent,
                    entry_name.as_c_str(),
                    folder_flags(),
                    Mode::empty(),
                )
                .map_err(io)?;
                settle(&made, 0o755, root)?;
            }
            Kind::File { size, sha256 } => {
                let mode: u16 = if entry.mode & 0o111 == 0 {
                    0o644
                } else {
                    0o755
                };
                let mut from = atlas_core::stage::open_file(source, &entry.path)?;
                let to = openat(
                    &parent,
                    entry_name.as_c_str(),
                    OFlag::O_WRONLY
                        | OFlag::O_CREAT
                        | OFlag::O_EXCL
                        | OFlag::O_NOFOLLOW
                        | OFlag::O_CLOEXEC,
                    Mode::from_bits_truncate(0o600),
                )
                .map_err(io)?;
                copy_checked(&mut from, &to, *size, sha256)?;
                settle(&to, mode, root)?;
            }
            Kind::Symlink { target } => {
                if !inside(target) {
                    return Err(refused("a link points outside the bundle"));
                }
                symlinkat(target.as_str(), &parent, entry_name.as_c_str()).map_err(io)?;
            }
        }
    }
    // Every folder written through, deepest first, the bundle last.
    for entry in manifest.entries.iter().rev() {
        if matches!(entry.kind, Kind::Folder) {
            let folder = open_parent(&bundle, &entry.path)?;
            nix::unistd::fsync(&folder).map_err(io)?;
        }
    }
    nix::unistd::fsync(into).map_err(io)
}

fn folder_flags() -> OFlag {
    OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC
}

/// The folder `path` below `bundle`, each level opened from the one above
/// it, never through a link.
fn open_parent(bundle: &Dir, path: &[String]) -> Result<Dir> {
    let mut folder = Dir::openat(bundle, ".", folder_flags(), Mode::empty()).map_err(io)?;
    for part in path {
        let name = FileName::new(part).ok_or_else(|| refused("a name is not a plain name"))?;
        folder =
            Dir::openat(&folder, name.as_c_str(), folder_flags(), Mode::empty()).map_err(io)?;
    }
    Ok(folder)
}

/// Give `entry` the mode `mode`, and make it root's when `root`.
fn settle(entry: &impl AsFd, mode: u16, root: bool) -> Result<()> {
    if root {
        fchown(entry, Some(0.into()), Some(0.into())).map_err(io)?;
    }
    fchmod(entry, Mode::from_bits_truncate(mode)).map_err(io)
}

/// Copy `from` to `to`, refusing anything but exactly `size` bytes with the
/// SHA-256 `digest`, then write it through.
fn copy_checked(from: &mut File, to: &OwnedFd, size: u64, digest: &str) -> Result<()> {
    let mut to = File::from(to.try_clone().map_err(io)?);
    let mut hasher = Sha256::new();
    let mut copied = 0_u64;
    let mut buffer = vec![0; 1 << 20];
    // One byte past its size is enough to know it grew.
    let mut limited = from.take(size + 1);
    loop {
        let read = limited.read(&mut buffer).map_err(io)?;
        if read == 0 {
            break;
        }
        copied += read as u64;
        if copied > size {
            return Err(refused("a file changed after it was staged"));
        }
        hasher.update(&buffer[..read]);
        to.write_all(&buffer[..read]).map_err(io)?;
    }
    if copied != size || format!("{:x}", hasher.finalize()) != digest {
        return Err(refused("a file changed after it was staged"));
    }
    to.sync_all().map_err(io)
}

/// Whether the link `target` stays inside the bundle: relative, and never
/// climbing at all. A climb that stays inside by its own count can still
/// leave through another link (`A -> .`, `B -> A/../outside`), and a
/// bundle's links never climb.
fn inside(target: &str) -> bool {
    !target.is_empty() && !target.starts_with('/') && target.split('/').all(|part| part != "..")
}

#[cfg(test)]
#[path = "copy_tests.rs"]
mod tests;

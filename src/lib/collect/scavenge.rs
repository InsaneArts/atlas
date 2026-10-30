//! Clearing what collections killed partway left in the collected folder:
//! partial files, and the folders of collections that never wrote their
//! receipt. Every level is opened from the one above it without following
//! a link, and only private folders of this account are touched.
use super::{ReceiptId, UNLABELLED};
use crate::host_session::Key;
use nix::{
    dir::{Dir, Type},
    fcntl::{AtFlags, OFlag},
    sys::stat::{FileStat, Mode, SFlag, fstat, fstatat},
    unistd::{UnlinkatFlags, unlinkat},
};
use std::{
    ffi::{CStr, CString},
    path::Path,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

/// A partial file older than this was left by a collection that was killed.
const ABANDONED: Duration = Duration::from_secs(60 * 60);

/// Remove what collections killed partway left under `root`, the collected
/// folder, once it is an hour old: the partial files in each receipt's
/// folder, and the folder of a collection that never wrote its receipt,
/// whose copies were never reported. Every level is opened from the one
/// above it without following a link, and must be a private folder of
/// this account named as Atlas names them: anything else is left alone.
pub(super) fn remove_abandoned(root: &Path) {
    let Some(mut root) = private_dir(Dir::open(root, directory_flags(), Mode::empty())) else {
        return;
    };
    for owner in names(&mut root) {
        let known = owner
            .to_str()
            .is_ok_and(|name| name == UNLABELLED || Key::parse(name).is_some());
        if !known {
            continue;
        }
        let Some(mut receipts) = private_dir(Dir::openat(
            &root,
            owner.as_c_str(),
            directory_flags(),
            Mode::empty(),
        )) else {
            continue;
        };
        for receipt in names(&mut receipts) {
            let named = receipt
                .to_str()
                .is_ok_and(|name| ReceiptId::parse(name).is_some());
            if named {
                scavenge(&receipts, &receipt);
            }
        }
    }
}

/// Clear what a collection killed partway left in the receipt folder
/// `name` of `parent`.
fn scavenge(parent: &Dir, name: &CStr) {
    let Ok(stat) = fstatat(parent, name, AtFlags::AT_SYMLINK_NOFOLLOW) else {
        return;
    };
    let stale = old(&stat);
    let Some(mut folder) = private_dir(Dir::openat(parent, name, directory_flags(), Mode::empty()))
    else {
        return;
    };
    let entries = names(&mut folder);
    let receipted = entries
        .iter()
        .any(|entry| entry.as_bytes() == b"receipt.json");
    for entry in &entries {
        let partial = entry
            .to_str()
            .is_ok_and(|entry| entry.starts_with('.') && entry.ends_with(".partial"));
        if partial || (stale && !receipted) {
            remove_file_at(&folder, entry);
        }
    }
    if stale && !receipted {
        if let Some(mut files) = private_dir(Dir::openat(
            &folder,
            c"files",
            directory_flags(),
            Mode::empty(),
        )) {
            for entry in names(&mut files) {
                remove_file_at(&files, &entry);
            }
            drop(files);
            let _ = unlinkat(&folder, c"files", UnlinkatFlags::RemoveDir);
        }
        drop(folder);
        let _ = unlinkat(parent, name, UnlinkatFlags::RemoveDir);
    }
}

/// Remove the regular file `name` of `folder` once it is an hour old:
/// never a link or a folder.
fn remove_file_at(folder: &Dir, name: &CStr) {
    if stale_file(folder, name) {
        let _ = unlinkat(folder, name, UnlinkatFlags::NoRemoveDir);
    }
}

/// Whether `name` of `folder` is a regular file, itself and not a link,
/// that last changed over an hour ago.
fn stale_file(folder: &Dir, name: &CStr) -> bool {
    fstatat(folder, name, AtFlags::AT_SYMLINK_NOFOLLOW).is_ok_and(|stat| {
        SFlag::from_bits_truncate(stat.st_mode & SFlag::S_IFMT.bits()) == SFlag::S_IFREG
            && old(&stat)
    })
}

/// Whether what `stat` describes last changed over an hour ago.
fn old(stat: &FileStat) -> bool {
    let changed = UNIX_EPOCH + Duration::from_secs(u64::try_from(stat.st_mtime).unwrap_or(0));
    SystemTime::now()
        .duration_since(changed)
        .is_ok_and(|age| age > ABANDONED)
}

/// How each level of the collected folder is opened: a folder, never
/// through a link.
fn directory_flags() -> OFlag {
    OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC
}

/// The folder `opened`, when it is a private folder of this account.
fn private_dir(opened: nix::Result<Dir>) -> Option<Dir> {
    let folder = opened.ok()?;
    let stat = fstat(&folder).ok()?;
    let directory =
        SFlag::from_bits_truncate(stat.st_mode & SFlag::S_IFMT.bits()) == SFlag::S_IFDIR;
    let private = stat.st_uid == nix::unistd::getuid().as_raw() && stat.st_mode & 0o077 == 0;
    (directory && private).then_some(folder)
}

/// The names in `folder`, but `.` and `..`, and links, which are never
/// followed.
fn names(folder: &mut Dir) -> Vec<CString> {
    folder
        .iter()
        .flatten()
        .filter(|entry| !matches!(entry.file_name().to_bytes(), b"." | b".."))
        .filter(|entry| entry.file_type() != Some(Type::Symlink))
        .map(|entry| entry.file_name().to_owned())
        .collect()
}

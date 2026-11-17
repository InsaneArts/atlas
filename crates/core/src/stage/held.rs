//! A snapshot held to be sent: every file it read stays open, so what is
//! sent is what was read and checked, never what a path names later. Each
//! file's state as it was read (its device, inode, size, and the times it
//! was last modified and changed) is kept, and checked again before and
//! after it is sent; and the SHA-256 of each of its [`CHUNK`]s as it was
//! read, so each chunk is sent only while it still reads the same.
use super::{Entry, Snapshot, refused};
use crate::error::Result;
use nix::sys::stat::{FileStat, fstat, lstat};
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    os::unix::fs::FileExt,
    path::{Path, PathBuf},
};

/// What a file or folder is, whatever names it: its device and inode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Identity {
    device: i64,
    inode: u64,
}

impl Identity {
    pub(super) fn of(stat: &FileStat) -> Self {
        Self {
            device: i64::from(stat.st_dev),
            inode: stat.st_ino,
        }
    }
}

/// A file's state as it was read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Seen {
    identity: Identity,
    size: i64,
    modified: (i64, i64),
    changed: (i64, i64),
}

impl Seen {
    fn of(stat: &FileStat) -> Self {
        Self {
            identity: Identity::of(stat),
            size: stat.st_size,
            modified: (stat.st_mtime, stat.st_mtime_nsec),
            changed: (stat.st_ctime, stat.st_ctime_nsec),
        }
    }
}

/// How much of a held file one chunk is: what is read, checked and sent at
/// once.
pub const CHUNK: usize = 64 * 1024;

/// The SHA-256 of each [`CHUNK`] of a file as it is read, in order: at most
/// 32 bytes for every 64 KiB a snapshot holds.
#[derive(Default)]
pub(super) struct Chunks {
    done: Vec<[u8; 32]>,
    hash: Sha256,
    filled: usize,
}

impl Chunks {
    /// Take `bytes`, the next of the file.
    pub(super) fn update(&mut self, mut bytes: &[u8]) {
        while !bytes.is_empty() {
            let taken = bytes.len().min(CHUNK - self.filled);
            self.hash.update(&bytes[..taken]);
            self.filled += taken;
            bytes = &bytes[taken..];
            if self.filled == CHUNK {
                self.done
                    .push(std::mem::take(&mut self.hash).finalize().into());
                self.filled = 0;
            }
        }
    }

    /// Every chunk's hash, the last one short when the file is.
    pub(super) fn finish(mut self) -> Vec<[u8; 32]> {
        if self.filled > 0 {
            self.done.push(self.hash.finalize().into());
        }
        self.done
    }
}

/// One file of a held snapshot, open as it was read.
#[derive(Debug)]
pub struct HeldFile {
    file: File,
    seen: Seen,
    /// The SHA-256 of each of its chunks as it was read.
    chunks: std::sync::Arc<[[u8; 32]]>,
}

impl HeldFile {
    /// `file`, as `stat` says it was once read, its chunks' hashes
    /// `chunks`.
    pub(super) fn new(file: File, stat: &FileStat, chunks: Vec<[u8; 32]>) -> Self {
        Self {
            file,
            seen: Seen::of(stat),
            chunks: chunks.into(),
        }
    }

    /// Its chunk `index` (from `index` × [`CHUNK`]), read from the file
    /// held, and only while it is the very bytes read for the snapshot.
    ///
    /// # Errors
    ///
    /// Refuses a chunk that reads otherwise now, or is not one of its own;
    /// returns the I/O error of reading it.
    pub fn chunk(&self, index: usize, size: u64) -> Result<Vec<u8>> {
        let changed = || refused("a file changed after it was read");
        let expected = self.chunks.get(index).ok_or_else(changed)?;
        let offset = (index as u64).saturating_mul(CHUNK as u64);
        let wanted = usize::try_from(size.saturating_sub(offset))
            .unwrap_or(CHUNK)
            .min(CHUNK);
        let mut chunk = vec![0_u8; wanted];
        let mut read = 0;
        while read < wanted {
            let got = self
                .file
                .read_at(&mut chunk[read..], offset + read as u64)?;
            if got == 0 {
                return Err(changed());
            }
            read += got;
        }
        if <[u8; 32]>::from(Sha256::digest(&chunk)) != *expected {
            return Err(changed());
        }
        Ok(chunk)
    }

    /// Refuse it once it is not the file as it was read: another file, or
    /// one changed, renamed or unlinked since.
    ///
    /// # Errors
    ///
    /// Refuses a file changed since it was read, or one that cannot be
    /// looked at.
    pub fn unchanged(&self) -> Result<()> {
        let now = fstat(&self.file).map_err(std::io::Error::from)?;
        if Seen::of(&now) != self.seen {
            return Err(refused("a file changed after it was read"));
        }
        Ok(())
    }

    /// The same file held again, for a reader of its own.
    ///
    /// # Errors
    ///
    /// Returns the I/O error of holding it again.
    pub fn try_clone(&self) -> Result<Self> {
        Ok(Self {
            file: self.file.try_clone()?,
            seen: self.seen,
            chunks: self.chunks.clone(),
        })
    }
}

/// A snapshot, every file it read held open in order, and what its root
/// was.
#[derive(Debug)]
pub struct Held {
    snapshot: Snapshot,
    files: Vec<HeldFile>,
    root: Identity,
}

impl Held {
    pub(super) fn new(snapshot: Snapshot, files: Vec<HeldFile>, root: Identity) -> Self {
        Self {
            snapshot,
            files,
            root,
        }
    }

    /// What it holds.
    pub fn snapshot(&self) -> &Snapshot {
        &self.snapshot
    }

    /// Each file's entry, its size and the file held for it, in order.
    pub fn files(&self) -> impl Iterator<Item = (&Entry, u64, &HeldFile)> {
        self.snapshot
            .files()
            .zip(&self.files)
            .map(|((entry, size, _), held)| (entry, size, held))
    }

    /// The file held at place `at` of [`Held::files`].
    pub fn file(&self, at: usize) -> Option<&HeldFile> {
        self.files.get(at)
    }

    /// Where each of its entries is below `source`, the path it was taken at.
    pub fn paths(&self, source: &Path) -> Vec<PathBuf> {
        self.snapshot
            .entries
            .iter()
            .map(|entry| {
                entry
                    .path
                    .iter()
                    .fold(source.to_owned(), |path, part| path.join(part))
            })
            .collect()
    }

    /// Refuse it once anything held changed, or `source`, the path it was
    /// taken at, no longer names what was held: its root, and each file by
    /// its path, the last name looked at without following a link. A check
    /// of a path is bound by this to the very file that is sent.
    ///
    /// # Errors
    ///
    /// Refuses a held file that changed, or a path that names something
    /// else now or cannot be looked at.
    pub fn still(&self, source: &Path) -> Result<()> {
        let moved = || refused("a path changed after it was read");
        let root = lstat(source).map_err(|_| moved())?;
        if Identity::of(&root) != self.root {
            return Err(moved());
        }
        for ((_, _, held), path) in self.files().zip(self.file_paths(source)) {
            held.unchanged()?;
            let named = lstat(&path).map_err(|_| moved())?;
            if Identity::of(&named) != held.seen.identity {
                return Err(moved());
            }
        }
        Ok(())
    }

    /// Where each file is below `source`, in order.
    fn file_paths(&self, source: &Path) -> Vec<PathBuf> {
        self.snapshot
            .files()
            .map(|(entry, _, _)| {
                entry
                    .path
                    .iter()
                    .fold(source.to_owned(), |path, part| path.join(part))
            })
            .collect()
    }
}

/// Make room for `files` more open at once in this process, as far as its
/// hard limit lets: a held snapshot keeps each of its files open.
pub(super) fn room_for(files: usize) {
    use nix::sys::resource::{Resource, getrlimit, setrlimit};
    let wanted = u64::try_from(files).unwrap_or(u64::MAX).saturating_add(512);
    if let Ok((soft, hard)) = getrlimit(Resource::RLIMIT_NOFILE)
        && soft < wanted
    {
        let _ = setrlimit(Resource::RLIMIT_NOFILE, wanted.min(hard), hard);
    }
}

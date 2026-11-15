//! Making a snapshot of a file or a folder on this disk, and opening its
//! files again to send them. Every level is opened from the one above it
//! without following a link, and every open is non-blocking, so nothing
//! raced into the tree is ever followed or waited on.
use super::{
    ATTRIBUTES, DEEPEST, Entry, Kind, LONGEST_TARGET, MOST_CONTENTS, MOST_ENTRIES, MOST_MANIFEST,
    PROVENANCE, Snapshot, refused,
};
use crate::{
    error::Result,
    xattr::{self, Attribute},
};
use nix::{
    dir::Dir,
    fcntl::{AtFlags, FcntlArg, OFlag, fcntl, openat, readlinkat},
    sys::stat::{FileStat, Mode, SFlag, fstat, fstatat},
};
use sha2::{Digest, Sha256};
use std::{
    ffi::{CStr, CString, OsStr},
    fs::File,
    io::Read,
    os::{
        fd::{AsFd, OwnedFd},
        unix::ffi::OsStrExt,
    },
    path::Path,
    time::{Duration, Instant},
};

/// The longest a snapshot may take to make.
const WALK: Duration = Duration::from_secs(10 * 60);

/// The only flag a snapshot carries past: compression is how a file is
/// stored, and its contents read the same.
const COMPRESSED: u32 = libc::UF_COMPRESSED;

/// A system call's failure as this crate's error.
trait Io<T> {
    fn io(self) -> Result<T>;
}

impl<T> Io<T> for nix::Result<T> {
    fn io(self) -> Result<T> {
        self.map_err(|errno| std::io::Error::from(errno).into())
    }
}

/// What a walk has found so far, and its bounds.
struct Walk {
    entries: Vec<Entry>,
    /// Names listed in folders above and not yet taken: they count against
    /// the entries the walk may hold, so no level can queue past it.
    queued: usize,
    contents: u64,
    /// The manifest's size so far, as an estimate that never falls short.
    manifest: usize,
    deadline: Instant,
}

impl Snapshot {
    /// The snapshot of the file or folder at `source`, which itself is
    /// never a link.
    ///
    /// # Errors
    ///
    /// Refuses what a snapshot cannot hold: a link at the root, a hard
    /// link, a device, a FIFO or a socket, a flag, setuid, setgid or sticky
    /// bits, a name that is not UTF-8, attributes on a link, anything past
    /// its bounds, or a file that changes while it is read.
    pub fn of(source: &Path) -> Result<Self> {
        let (parent, name) = parent_and_name(source)?;
        let parent = Dir::open(parent, parent_flags(), Mode::empty()).io()?;
        let named = source
            .file_name()
            .and_then(OsStr::to_str)
            .ok_or_else(|| refused("its name is not UTF-8"))?
            .to_owned();
        let mut walk = Walk {
            entries: Vec::new(),
            queued: 0,
            contents: 0,
            manifest: 0,
            deadline: Instant::now() + WALK,
        };
        let stat = fstatat(&parent, name.as_c_str(), AtFlags::AT_SYMLINK_NOFOLLOW).io()?;
        if kind(&stat) == SFlag::S_IFLNK {
            return Err(refused("it is a link"));
        }
        walk.entry(&parent, &name, Vec::new())?;
        let snapshot = Self::new(named, walk.entries)?;
        snapshot.check()?;
        Ok(snapshot)
    }
}

impl Walk {
    /// Add the entry `name` of `parent`, at `path`, and all it holds.
    fn entry(&mut self, parent: &impl AsFd, name: &CStr, path: Vec<String>) -> Result<()> {
        if Instant::now() > self.deadline {
            return Err(refused("it took too long to read"));
        }
        if self.entries.len() == MOST_ENTRIES {
            return Err(refused("it holds too many entries"));
        }
        if path.len() > DEEPEST {
            return Err(refused("an entry lies too deep"));
        }
        let seen = fstatat(parent, name, AtFlags::AT_SYMLINK_NOFOLLOW).io()?;
        match kind(&seen) {
            SFlag::S_IFLNK => self.link(parent, name, &seen, path),
            SFlag::S_IFDIR => self.folder(parent, name, &seen, path),
            SFlag::S_IFREG => self.file(parent, name, &seen, path),
            _ => Err(refused("it holds a device, a FIFO or a socket")),
        }
    }

    fn link(
        &mut self,
        parent: &impl AsFd,
        name: &CStr,
        seen: &FileStat,
        path: Vec<String>,
    ) -> Result<()> {
        // The link itself, never what it points at.
        let opened = openat(parent, name, link_flags(), Mode::empty()).io()?;
        same(&fstat(&opened).io()?, seen, SFlag::S_IFLNK)?;
        if !attributes(&opened)?.is_empty() {
            return Err(refused("a link carries extended attributes"));
        }
        let target = readlinkat(parent, name).io()?;
        // The target read by name must be the very link looked at.
        same(
            &fstatat(parent, name, AtFlags::AT_SYMLINK_NOFOLLOW).io()?,
            seen,
            SFlag::S_IFLNK,
        )?;
        let target = target
            .to_str()
            .filter(|target| target.len() <= LONGEST_TARGET)
            .ok_or_else(|| refused("a link's target cannot be kept"))?
            .to_owned();
        self.push(Entry {
            path,
            kind: Kind::Symlink { target },
            mode: 0,
            attributes: Vec::new(),
        })
    }

    fn folder(
        &mut self,
        parent: &impl AsFd,
        name: &CStr,
        seen: &FileStat,
        path: Vec<String>,
    ) -> Result<()> {
        let mut folder = Dir::openat(parent, name, folder_flags(), Mode::empty()).io()?;
        let stat = fstat(&folder).io()?;
        same(&stat, seen, SFlag::S_IFDIR)?;
        plain(&stat)?;
        let attributes = attributes(&folder)?;
        self.push(Entry {
            path: path.clone(),
            kind: Kind::Folder,
            mode: mode(&stat),
            attributes,
        })?;
        let mut names = Vec::new();
        for listed in folder.iter() {
            let listed = listed.io()?;
            let child = listed.file_name();
            if [&b"."[..], b".."].contains(&child.to_bytes()) {
                continue;
            }
            if self.entries.len() + self.queued + names.len() >= MOST_ENTRIES {
                return Err(refused("it holds too many entries"));
            }
            if Instant::now() > self.deadline {
                return Err(refused("it took too long to read"));
            }
            let text = child
                .to_str()
                .map_err(|_| refused("a name in it is not UTF-8"))?
                .to_owned();
            names.push((text, child.to_owned()));
        }
        names.sort();
        self.queued += names.len();
        for (text, child) in names {
            self.queued -= 1;
            let mut below = path.clone();
            below.push(text);
            self.entry(&folder, &child, below)?;
        }
        Ok(())
    }

    fn file(
        &mut self,
        parent: &impl AsFd,
        name: &CStr,
        seen: &FileStat,
        path: Vec<String>,
    ) -> Result<()> {
        let file = open_regular(parent, name, seen)?;
        // Its attributes and its contents are read between the same two
        // looks: a change to either changes what the second one sees.
        let before = fstat(&file).io()?;
        let attributes = attributes(&file)?;
        let mut file = File::from(file);
        let mut hash = Sha256::new();
        let mut buffer = vec![0_u8; 1 << 20];
        let mut read = 0_u64;
        // Never read past what it said it holds, nor past what is left.
        let size = u64::try_from(before.st_size).unwrap_or(u64::MAX);
        let most = size.min(MOST_CONTENTS - self.contents).saturating_add(1);
        loop {
            if Instant::now() > self.deadline {
                return Err(refused("it took too long to read"));
            }
            let room = usize::try_from(most - read)
                .unwrap_or(usize::MAX)
                .min(buffer.len());
            let got = file.read(&mut buffer[..room])?;
            if got == 0 {
                break;
            }
            hash.update(&buffer[..got]);
            read += got as u64;
            if read == most {
                return Err(refused("a file changed while it was read"));
            }
        }
        let after = fstat(&file).io()?;
        let unchanged = read == u64::try_from(before.st_size).unwrap_or(u64::MAX)
            && (before.st_size, before.st_mtime, before.st_mtime_nsec)
                == (after.st_size, after.st_mtime, after.st_mtime_nsec)
            && (before.st_ctime, before.st_ctime_nsec) == (after.st_ctime, after.st_ctime_nsec);
        if !unchanged {
            return Err(refused("a file changed while it was read"));
        }
        self.contents = self
            .contents
            .checked_add(read)
            .filter(|contents| *contents <= MOST_CONTENTS)
            .ok_or_else(|| refused("its files are too large"))?;
        self.push(Entry {
            path,
            kind: Kind::File {
                size: read,
                sha256: format!("{:x}", hash.finalize()),
            },
            mode: mode(&before),
            attributes,
        })
    }

    /// Keep `entry`, counting what it adds to the manifest.
    fn push(&mut self, entry: Entry) -> Result<()> {
        // Every byte of a name or a target may take six once escaped.
        let names: usize = entry.path.iter().map(|part| part.len() * 6 + 3).sum();
        let target = match &entry.kind {
            Kind::Symlink { target } => target.len() * 6,
            Kind::File { .. } | Kind::Folder => 0,
        };
        let attributes: usize = entry
            .attributes
            .iter()
            .map(|attribute| attribute.name.len() * 6 + attribute.value.len().div_ceil(3) * 4 + 32)
            .sum();
        // Room for the kind, digest and mode, more than they take.
        self.manifest = self
            .manifest
            .saturating_add(names + target + attributes + 256);
        if self.manifest > MOST_MANIFEST {
            return Err(refused("its manifest is too large"));
        }
        self.entries.push(entry);
        Ok(())
    }
}

/// Open the file at `path` below the snapshot's root `source` again, to
/// send it: each level from the one above it, never through a link, and
/// only when it is still a regular file.
///
/// # Errors
///
/// Fails when any level is not what it was, or cannot be opened.
pub fn open_file(source: &Path, path: &[String]) -> Result<File> {
    let (parent, name) = parent_and_name(source)?;
    let mut folder = Dir::open(parent, parent_flags(), Mode::empty()).io()?;
    let mut name = name;
    for part in path {
        folder = Dir::openat(&folder, name.as_c_str(), folder_flags(), Mode::empty()).io()?;
        name = CString::new(part.as_bytes()).map_err(|_| refused("a name holds NUL"))?;
    }
    let seen = fstatat(&folder, name.as_c_str(), AtFlags::AT_SYMLINK_NOFOLLOW).io()?;
    Ok(File::from(open_regular(&folder, &name, &seen)?))
}

/// Open the regular file `name` of `parent` that `seen` describes, never
/// blocking, then make it block for reading.
fn open_regular(parent: &impl AsFd, name: &CStr, seen: &FileStat) -> Result<OwnedFd> {
    if kind(seen) != SFlag::S_IFREG {
        return Err(refused("an entry is no longer a file"));
    }
    let file = openat(parent, name, file_flags(), Mode::empty()).io()?;
    let stat = fstat(&file).io()?;
    same(&stat, seen, SFlag::S_IFREG)?;
    plain(&stat)?;
    if stat.st_nlink != 1 {
        return Err(refused("it holds a hard link"));
    }
    let flags = OFlag::from_bits_retain(fcntl(&file, FcntlArg::F_GETFL).io()?);
    fcntl(&file, FcntlArg::F_SETFL(flags - OFlag::O_NONBLOCK)).io()?;
    Ok(file)
}

/// The parent folder of `source`, and its name there.
fn parent_and_name(source: &Path) -> Result<(&Path, CString)> {
    let name = source
        .file_name()
        .ok_or_else(|| refused("it has no name"))?;
    let parent = source
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    Ok((parent, c_name(name)?))
}

/// `name` as a C string.
fn c_name(name: &OsStr) -> Result<CString> {
    CString::new(name.as_bytes()).map_err(|_| refused("its name holds NUL"))
}

/// The extended attributes of `file` a snapshot carries.
fn attributes(file: &impl AsFd) -> Result<Vec<Attribute>> {
    Ok(xattr::of(file, ATTRIBUTES)?
        .into_iter()
        .filter(|attribute| attribute.name != PROVENANCE)
        .collect())
}

/// Refuse what `stat` describes when it has setuid, setgid or sticky bits,
/// or a flag other than compression.
fn plain(stat: &FileStat) -> Result<()> {
    if stat.st_mode & 0o7000 != 0 {
        return Err(refused("an entry is setuid, setgid or sticky"));
    }
    if stat.st_flags & !COMPRESSED != 0 {
        return Err(refused("an entry carries a flag"));
    }
    Ok(())
}

/// Refuse an open entry that is not the one looked at, or not `kind`.
fn same(opened: &FileStat, seen: &FileStat, expected: SFlag) -> Result<()> {
    if kind(opened) != expected || (opened.st_dev, opened.st_ino) != (seen.st_dev, seen.st_ino) {
        return Err(refused("an entry changed while it was read"));
    }
    Ok(())
}

fn kind(stat: &FileStat) -> SFlag {
    SFlag::from_bits_truncate(stat.st_mode & SFlag::S_IFMT.bits())
}

fn mode(stat: &FileStat) -> u32 {
    u32::from(stat.st_mode) & 0o777
}

/// The folder holding the source: a path on this disk the station chose,
/// which may pass through links.
fn parent_flags() -> OFlag {
    OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_CLOEXEC
}

fn folder_flags() -> OFlag {
    OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_NONBLOCK | OFlag::O_CLOEXEC
}

fn file_flags() -> OFlag {
    OFlag::O_RDONLY | OFlag::O_NOFOLLOW | OFlag::O_NONBLOCK | OFlag::O_CLOEXEC
}

/// The link itself, opened for its metadata only.
fn link_flags() -> OFlag {
    OFlag::O_RDONLY
        | OFlag::from_bits_retain(libc::O_SYMLINK)
        | OFlag::O_NONBLOCK
        | OFlag::O_CLOEXEC
}

//! Writing a stage's entries into its staging folder, and removing a tree
//! Atlas wrote. Everything is made relative to its parent's descriptor,
//! created exclusively and never through a link, so nothing already there
//! is written into and nothing a link points at is touched.
use super::{EVIDENCE, Evidence, Root, staging};
use atlas_core::{
    error::{Code, Error, Result},
    files::Token,
    stage::{DEEPEST, Entry, Kind, PROVENANCE, Snapshot},
    xattr,
};
use nix::{
    dir::Dir,
    fcntl::{AtFlags, OFlag, openat},
    sys::stat::{FchmodatFlags, FileStat, Mode, SFlag, fchmod, fchmodat, fstatat, mkdirat},
    unistd::{UnlinkatFlags, symlinkat, unlinkat},
};
use sha2::{Digest, Sha256};
use std::{
    ffi::{CStr, CString},
    fs::File,
    io::{self, Read, Write},
    os::fd::{AsFd, OwnedFd},
};

/// Where a stage's file contents come from, a chunk at a time, in the
/// snapshot's order; none once they end.
pub(crate) trait Contents {
    fn next_chunk(&mut self) -> Option<Vec<u8>>;
}

/// The contents not yet written, the chunk under way first.
struct Stream<'a> {
    contents: &'a mut dyn Contents,
    chunk: Vec<u8>,
    at: usize,
}

impl Stream<'_> {
    /// Up to `most` more bytes, none once the contents end.
    fn take(&mut self, most: usize) -> Option<&[u8]> {
        while self.at == self.chunk.len() {
            self.chunk = self.contents.next_chunk()?;
            self.at = 0;
        }
        let end = self.chunk.len().min(self.at + most);
        let taken = &self.chunk[self.at..end];
        self.at = end;
        Some(taken)
    }

    /// Whether the contents ended with the last file: nothing left of the
    /// chunk under way, and nothing more to come.
    fn ended(&mut self) -> bool {
        if self.at < self.chunk.len() {
            return false;
        }
        loop {
            match self.contents.next_chunk() {
                None => return true,
                Some(chunk) if chunk.is_empty() => {}
                Some(_) => return false,
            }
        }
    }
}

fn refused(why: &str) -> Error {
    Error::new(Code::BadRequest, format!("Nothing was staged: {why}"))
}

fn sys<T>(result: nix::Result<T>) -> Result<T> {
    result.map_err(|errno| io::Error::from(errno).into())
}

/// Write `snapshot`, asked for with `request`, its file contents from
/// `contents`, into `change`'s staging folder in `root`, as long as
/// `going` says it may go on. Every
/// file's contents and final metadata are synced together, every folder's
/// after its mode, and the evidence last, with the folder holding it.
///
/// # Errors
///
/// Refuses a snapshot that does not check, contents that are not the
/// snapshot's, and attributes or an ACL that are not exactly the
/// snapshot's once written; fails when anything cannot be written. A
/// staging folder it made is left for the caller to discard.
pub(crate) fn write(
    root: &Root,
    change: &Token,
    snapshot: &Snapshot,
    request: &serde_json::Value,
    contents: &mut dyn Contents,
    going: &dyn Fn() -> bool,
) -> Result<()> {
    snapshot.check()?;
    if snapshot.name == EVIDENCE {
        return Err(refused("its name is the one Atlas keeps for itself"));
    }
    let name = staging(change);
    sys(mkdirat(&root.folder, name.as_c_str(), Mode::S_IRWXU))?;
    let staged = open_folder(&root.folder, &name)?;
    let mut stream = Stream {
        contents,
        chunk: Vec::new(),
        at: 0,
    };
    let mut folders = vec![staged.try_clone()?];
    for entry in &snapshot.entries {
        if !going() {
            return Err(Error::new(Code::Cancelled, "The stage was ended"));
        }
        // The folder holding an entry is the one made last at its depth.
        folders.truncate(entry.path.len() + 1);
        let parent = &folders[entry.path.len()];
        let named = c_name(entry.path.last().unwrap_or(&snapshot.name))?;
        if let Some(made) = make(parent, &named, entry, &mut stream)? {
            folders.push(made);
        }
    }
    if !stream.ended() {
        return Err(refused("more bytes came than its files hold"));
    }
    drop(folders);
    // Deepest first, so a folder that takes away its own writing is set
    // only once everything inside it is.
    for entry in snapshot.entries.iter().rev() {
        if entry.kind == Kind::Folder {
            let folder = open_path(&staged, &snapshot.name, &entry.path)?;
            sys(fchmod(&folder, mode(entry.mode)))?;
            folder.sync_all()?;
        }
    }
    let evidence = serde_json::to_vec(&Evidence {
        change: change.clone(),
        request: request.clone(),
    })?;
    let mut kept = File::from(sys(openat(
        &staged,
        c_name(EVIDENCE)?.as_c_str(),
        created(),
        Mode::S_IRUSR | Mode::S_IWUSR,
    ))?);
    kept.write_all(&evidence)?;
    kept.sync_all()?;
    staged.sync_all()?;
    Ok(())
}

/// Make `entry`, named `name` in `parent`: a folder made is handed back,
/// still writable, for what goes into it.
fn make(
    parent: &File,
    name: &CStr,
    entry: &Entry,
    stream: &mut Stream<'_>,
) -> Result<Option<File>> {
    match &entry.kind {
        Kind::Folder => {
            sys(mkdirat(parent, name, Mode::S_IRWXU))?;
            let folder = open_folder(parent, name)?;
            attributes(&folder, entry)?;
            Ok(Some(folder))
        }
        Kind::File { size, sha256 } => {
            let mut file = File::from(sys(openat(
                parent,
                name,
                created(),
                Mode::S_IRUSR | Mode::S_IWUSR,
            ))?);
            let mut hash = Sha256::new();
            let mut left = *size;
            while left > 0 {
                let most = usize::try_from(left.min(1 << 20)).unwrap_or(1 << 20);
                let bytes = stream
                    .take(most)
                    .ok_or_else(|| refused("its contents ended before its files did"))?;
                hash.update(bytes);
                file.write_all(bytes)?;
                left -= bytes.len() as u64;
            }
            if format!("{:x}", hash.finalize()) != *sha256 {
                return Err(refused("a file's contents are not the ones named"));
            }
            attributes(&file, entry)?;
            sys(fchmod(&file, mode(entry.mode)))?;
            file.sync_all()?;
            Ok(None)
        }
        Kind::Symlink { target } => {
            sys(symlinkat(target.as_str(), parent, name))?;
            // The link itself, never what it points at: it carries nothing.
            let link = File::from(sys(openat(parent, name, link(), Mode::empty()))?);
            if !carried(&link)?.is_empty() {
                return Err(refused("a link took on attributes"));
            }
            Ok(None)
        }
    }
}

/// Set `entry`'s attributes on `made`, then read them back: exactly those,
/// none missing, none added, none changed, and no ACL.
fn attributes(made: &File, entry: &Entry) -> Result<()> {
    for attribute in &entry.attributes {
        xattr::set(made, attribute)?;
    }
    if carried(made)? != entry.attributes {
        return Err(refused(
            "its attributes are not the ones named once written",
        ));
    }
    if atlas_core::acl::present_on(made)? {
        return Err(refused("an entry took on an ACL"));
    }
    Ok(())
}

/// The attributes of `file` a stage carries: all but the system's own.
fn carried(file: &File) -> Result<Vec<xattr::Attribute>> {
    Ok(xattr::of(file, atlas_core::stage::ATTRIBUTES)?
        .into_iter()
        .filter(|attribute| attribute.name != PROVENANCE)
        .collect())
}

/// Remove `name` in `parent` and everything below it, never through a
/// link: a link is removed itself. Nothing there is nothing to do.
///
/// # Errors
///
/// Fails when anything in it cannot be removed, or it lies too deep.
pub(crate) fn discard(parent: &File, name: &CStr) -> io::Result<()> {
    remove(parent, name, 0, None)
}

/// Remove the folder `name` in `parent` and everything below it, as
/// [`discard`] does, but only while it is the folder `node` names
/// (`dev:ino`): it is opened, checked and emptied through that one
/// descriptor, so one swapped in meanwhile is never touched.
///
/// # Errors
///
/// Fails when it is not that folder, or anything in it cannot be removed.
pub(crate) fn discard_as(parent: &File, name: &CStr, node: &str) -> io::Result<()> {
    remove(parent, name, 0, Some(node))
}

fn remove(parent: &File, name: &CStr, depth: usize, node: Option<&str>) -> io::Result<()> {
    let stat = match fstatat(parent, name, AtFlags::AT_SYMLINK_NOFOLLOW) {
        Ok(stat) => stat,
        Err(nix::errno::Errno::ENOENT) => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    if kind(&stat) != SFlag::S_IFDIR {
        if node.is_some() {
            return Err(io::Error::other(
                "What is to be removed is not the folder named",
            ));
        }
        return unlinkat(parent, name, UnlinkatFlags::NoRemoveDir).map_err(io::Error::from);
    }
    if depth > DEEPEST + 1 {
        return Err(io::Error::other("A folder to remove lies too deep"));
    }
    // Atlas made it: whatever mode it was given, it may empty it. Opened
    // and checked first, its mode then set through its descriptor. Only a
    // folder that cannot be opened at all has its mode set by name, never
    // through a link: one swapped in at that moment gains its owner's
    // access, and is then refused below, never removed.
    let folder = match open_folder(parent, name) {
        Ok(folder) => folder,
        Err(_) => {
            fchmodat(parent, name, Mode::S_IRWXU, FchmodatFlags::NoFollowSymlink)?;
            open_folder(parent, name).map_err(io::Error::other)?
        }
    };
    let opened = nix::sys::stat::fstat(&folder)?;
    let named = node.is_none_or(|node| node == format!("{}:{}", opened.st_dev, opened.st_ino));
    if (opened.st_dev, opened.st_ino) != (stat.st_dev, stat.st_ino) || !named {
        return Err(io::Error::other(
            "A folder to remove changed while it was removed",
        ));
    }
    fchmod(&folder, Mode::S_IRWXU)?;
    let mut names = Vec::new();
    for listed in Dir::from_fd(OwnedFd::from(folder.try_clone()?))?.iter() {
        let listed = listed?;
        let child = listed.file_name();
        if ![&b"."[..], b".."].contains(&child.to_bytes()) {
            names.push(child.to_owned());
        }
    }
    for child in names {
        remove(&folder, &child, depth + 1, None)?;
    }
    unlinkat(parent, name, UnlinkatFlags::RemoveDir).map_err(io::Error::from)
}

/// Whether `name` is in `parent`, itself and not followed.
///
/// # Errors
///
/// Fails when that cannot be told: only its absence is no.
pub(crate) fn exists(parent: &File, name: &CStr) -> io::Result<bool> {
    match fstatat(parent, name, AtFlags::AT_SYMLINK_NOFOLLOW) {
        Ok(_) => Ok(true),
        Err(nix::errno::Errno::ENOENT) => Ok(false),
        Err(error) => Err(error.into()),
    }
}

/// The folder `name` of `parent`, never through a link.
pub(crate) fn open_folder(parent: &impl AsFd, name: &CStr) -> Result<File> {
    let flags = OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC;
    Ok(File::from(sys(openat(parent, name, flags, Mode::empty()))?))
}

/// The folder at `path` below the item `name` in `staged`.
fn open_path(staged: &File, name: &str, path: &[String]) -> Result<File> {
    let mut folder = open_folder(staged, &c_name(name)?)?;
    for part in path {
        folder = open_folder(&folder, &c_name(part)?)?;
    }
    Ok(folder)
}

/// Read the small regular file `name` of `folder`, never through a link.
pub(crate) fn read_small(folder: &File, name: &str) -> Result<Vec<u8>> {
    let flags = OFlag::O_RDONLY | OFlag::O_NOFOLLOW | OFlag::O_NONBLOCK | OFlag::O_CLOEXEC;
    let file = File::from(sys(openat(
        folder,
        c_name(name)?.as_c_str(),
        flags,
        Mode::empty(),
    ))?);
    if !file.metadata()?.is_file() {
        return Err(Error::new(Code::State, "Not a file"));
    }
    let mut read = Vec::new();
    file.take(64 * 1024).read_to_end(&mut read)?;
    Ok(read)
}

fn c_name(name: &str) -> Result<CString> {
    CString::new(name).map_err(|_| refused("a name holds NUL"))
}

fn created() -> OFlag {
    OFlag::O_WRONLY | OFlag::O_CREAT | OFlag::O_EXCL | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC
}

fn link() -> OFlag {
    OFlag::O_RDONLY
        | OFlag::from_bits_retain(libc::O_SYMLINK)
        | OFlag::O_NONBLOCK
        | OFlag::O_CLOEXEC
}

fn mode(bits: u32) -> Mode {
    Mode::from_bits_truncate(u16::try_from(bits & 0o777).unwrap_or(0o700))
}

fn kind(stat: &FileStat) -> SFlag {
    SFlag::from_bits_truncate(stat.st_mode & SFlag::S_IFMT.bits())
}

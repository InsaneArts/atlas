//! Opening a file or folder a command named, proven to be one no account but
//! this one or root can change behind Atlas's back. Every guarded reader,
//! writer and handoff goes through here instead of checking a path and then
//! opening it by the same name, which another account could change in
//! between.
//!
//! The path is stabilized once (its folder's real path, the last component
//! kept as named), refused when it is Atlas's own folder, then walked from
//! the root with each component opened without following a link: a link that
//! appears now means the path changed since, and is refused. Every folder on
//! the path must be this account's or root's, unwritable by any other
//! account by its mode or its ACL; a sticky folder like `/tmp` is allowed
//! because the next entry on the path is itself owner-checked. A reader then
//! reads the returned descriptor and never reopens by name; a writer places a
//! file in the returned folder descriptor and renames it there; a handoff
//! runs on the resolved path, safe because no other account can change a name
//! on it.
use crate::{
    acl,
    error::{Code, Error, Result},
};
use nix::{
    dir::Dir,
    fcntl::{AtFlags, FcntlArg, OFlag, fcntl, openat, renameat},
    sys::stat::{FileStat, Mode, fstat, fstatat},
};
use std::{
    ffi::{CString, OsStr},
    fs::File,
    io::Write,
    os::{
        fd::{AsFd, OwnedFd},
        unix::ffi::OsStrExt,
    },
    path::{Component, Path, PathBuf},
};

/// Groups whose members a file's owner trusts not to change it behind their
/// back: the system's (`wheel`) and the administrators' (`admin`). Another
/// account, the helper among them, is never in them for this purpose.
const TRUSTED_GROUPS: [u32; 2] = [0, 80];

/// What a guarded open is for, so the refusal reads plainly.
#[derive(Clone, Copy)]
enum Intent {
    Read,
    Write,
    HandOver,
}

impl Intent {
    fn verb(self) -> &'static str {
        match self {
            Self::Read => "reads",
            Self::Write => "writes into",
            Self::HandOver => "hands over",
        }
    }
}

/// A file opened through the guard: its descriptor, which the reader reads,
/// and the real path it was proven at.
#[derive(Debug)]
pub struct Reading {
    file: File,
    path: PathBuf,
}

impl Reading {
    /// The real path it was proven at.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The file's bytes, up to `limit`, read through the descriptor.
    ///
    /// # Errors
    ///
    /// Returns the I/O error of reading it.
    pub fn read_up_to(self, limit: u64) -> Result<Vec<u8>> {
        crate::files::read_up_to(self.file, limit).map_err(Error::from)
    }

    /// The file read and its bytes, up to `limit`, for a writer that locks
    /// it, compares it and replaces it: the descriptor stays open.
    ///
    /// # Errors
    ///
    /// Returns the I/O error of reading it.
    pub fn read_keeping(self, limit: u64) -> Result<(File, Vec<u8>)> {
        let reader = self.file.try_clone()?;
        let bytes = crate::files::read_up_to(reader, limit)?;
        Ok((self.file, bytes))
    }

    /// The opened file itself, for a writer that locks it before it reads.
    pub fn into_file(self) -> File {
        self.file
    }

    /// Which file was read, as its device and inode: what a later step
    /// compares before it acts on the name again.
    ///
    /// # Errors
    ///
    /// Returns the I/O error of reading the descriptor.
    pub fn identity(&self) -> Result<Identity> {
        identity_of(&self.file)
    }
}

/// A file's device and inode: the file itself, whatever names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Identity {
    device: u64,
    inode: u64,
}

fn identity_of(fd: &impl AsFd) -> Result<Identity> {
    let stat = fstat(fd).map_err(|error| problem(format!("Read a descriptor: {error}")))?;
    Ok(Identity::of(&stat))
}

impl Identity {
    fn of(stat: &FileStat) -> Self {
        Self {
            // The device number's sign carries no meaning here.
            #[allow(clippy::cast_sign_loss)]
            device: stat.st_dev as u64,
            inode: stat.st_ino,
        }
    }
}

/// A folder opened through the guard: a writer places a file in it and
/// renames it there, so nothing is written through a link another account
/// put in its place.
#[derive(Debug)]
pub struct Folder {
    dir: Dir,
    path: PathBuf,
}

impl Folder {
    /// The real path it was proven at.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Which file the entry `name` is now, without following a link; none
    /// when there is no such entry.
    ///
    /// # Errors
    ///
    /// Returns the I/O error of reading the entry.
    pub fn entry_identity(&self, name: &OsStr) -> Result<Option<Identity>> {
        match fstatat(
            &self.dir,
            c_name(name)?.as_c_str(),
            AtFlags::AT_SYMLINK_NOFOLLOW,
        ) {
            Ok(stat) => Ok(Some(Identity::of(&stat))),
            Err(nix::errno::Errno::ENOENT) => Ok(None),
            Err(error) => Err(problem(format!(
                "Could not read {} in {}: {error}",
                name.to_string_lossy(),
                self.path.display()
            ))),
        }
    }

    /// The plain file `name` directly in this folder, read through the
    /// folder's own descriptor with no link followed, up to `limit` bytes;
    /// none when no entry has that name. Whatever now holds this folder's
    /// path is never read: the entry is the one in the folder opened.
    ///
    /// # Errors
    ///
    /// Refuses a link, a FIFO, a socket, a device or a folder in its place,
    /// a file another account owns or may change, and one larger than
    /// `limit`; returns the I/O error of reading it.
    pub fn read_entry(&self, name: &OsStr, limit: u64) -> Result<Option<Vec<u8>>> {
        let opened = match openat(
            &self.dir,
            c_name(name)?.as_c_str(),
            OFlag::O_RDONLY | OFlag::O_NOFOLLOW | OFlag::O_NONBLOCK | OFlag::O_CLOEXEC,
            Mode::empty(),
        ) {
            Ok(opened) => opened,
            Err(nix::errno::Errno::ENOENT) => return Ok(None),
            Err(error) => return Err(open_error(Intent::Read, &self.path, error, "in its place")),
        };
        let facts = facts_of(&opened)?;
        if !facts.is_file() {
            return Err(refused(Intent::Read, "it is not a plain file"));
        }
        if !facts.owned_by(this_uid()) {
            return Err(refused(Intent::Read, "it belongs to another account"));
        }
        terminal_holds(&opened, facts, Intent::Read)?;
        let bytes = crate::files::read_up_to(File::from(opened), limit.saturating_add(1))?;
        if bytes.len() as u64 > limit {
            return Err(problem(format!(
                "{} in {} holds more than {limit} bytes",
                name.to_string_lossy(),
                self.path.display()
            )));
        }
        Ok(Some(bytes))
    }

    /// Whether the entry `name` in this folder is a plain file now, looked
    /// at beside the folder's descriptor without following a link: a link,
    /// a folder or anything else of that name is not, nor is no entry.
    ///
    /// # Errors
    ///
    /// Returns the I/O error of looking at it.
    pub fn holds_file(&self, name: &OsStr) -> Result<bool> {
        match fstatat(
            &self.dir,
            c_name(name)?.as_c_str(),
            AtFlags::AT_SYMLINK_NOFOLLOW,
        ) {
            Ok(stat) => Ok(Facts::from_stat(&stat).is_file()),
            Err(nix::errno::Errno::ENOENT) => Ok(false),
            Err(error) => Err(problem(format!(
                "Could not look at {} in {}: {error}",
                name.to_string_lossy(),
                self.path.display()
            ))),
        }
    }

    /// Remove the entry `name` from this folder, through its descriptor; an
    /// entry that is not there is already gone.
    ///
    /// # Errors
    ///
    /// Returns the I/O error of removing it.
    pub fn remove(&self, name: &OsStr) -> Result<()> {
        match nix::unistd::unlinkat(
            &self.dir,
            c_name(name)?.as_c_str(),
            nix::unistd::UnlinkatFlags::NoRemoveDir,
        ) {
            Ok(()) | Err(nix::errno::Errno::ENOENT) => Ok(()),
            Err(error) => Err(problem(format!(
                "Could not remove {} from {}: {error}",
                name.to_string_lossy(),
                self.path.display()
            ))),
        }
    }

    /// Remove the entry `name` only while it is still the file `expected`,
    /// through this folder's descriptor: whether it was removed. Nobody but
    /// this account or root can change the folder, so the entry looked at is
    /// the one removed.
    ///
    /// # Errors
    ///
    /// Returns the I/O error of reading or removing it.
    pub fn remove_if_same(&self, name: &OsStr, expected: Identity) -> Result<bool> {
        if self.entry_identity(name)? != Some(expected) {
            return Ok(false);
        }
        self.remove(name)?;
        Ok(true)
    }

    /// Replace the entry `name` in this folder with `bytes`, mode `mode`,
    /// through the folder's own descriptor: a temporary file made there with
    /// no link followed, then renamed over `name`.
    ///
    /// # Errors
    ///
    /// Returns the I/O error of writing or renaming it.
    pub fn replace(&self, name: &OsStr, bytes: &[u8], mode: u32) -> Result<()> {
        let final_name = c_name(name)?;
        let temp = self.written(name, bytes, mode)?;
        renameat(&self.dir, temp.as_c_str(), &self.dir, final_name.as_c_str()).map_err(|error| {
            self.remove_temp(&temp);
            problem(format!(
                "Could not place {} in {}: {error}",
                name.to_string_lossy(),
                self.path.display()
            ))
        })
    }

    /// Make the entry `name` in this folder with `bytes`, mode `mode`, only
    /// while there is none, through the folder's own descriptor: written
    /// whole beside it first, then linked in place, which fails when the
    /// name is taken. Whether it was made; an entry already there is never
    /// replaced.
    ///
    /// # Errors
    ///
    /// Returns the I/O error of writing or linking it.
    pub fn create_new(&self, name: &OsStr, bytes: &[u8], mode: u32) -> Result<bool> {
        let final_name = c_name(name)?;
        let temp = self.written(name, bytes, mode)?;
        let linked = nix::unistd::linkat(
            &self.dir,
            temp.as_c_str(),
            &self.dir,
            final_name.as_c_str(),
            AtFlags::empty(),
        );
        self.remove_temp(&temp);
        match linked {
            Ok(()) => Ok(true),
            Err(nix::errno::Errno::EEXIST) => Ok(false),
            Err(error) => Err(problem(format!(
                "Could not place {} in {}: {error}",
                name.to_string_lossy(),
                self.path.display()
            ))),
        }
    }

    /// A temporary file made in this folder with no link followed, holding
    /// `bytes` with exactly `mode` and written to the disk, for `name`: its
    /// name.
    fn written(&self, name: &OsStr, bytes: &[u8], mode: u32) -> Result<CString> {
        let temp = CString::new(format!(".atlas-{}.tmp", crate::files::token()))
            .map_err(|_| problem("A temporary name holds NUL"))?;
        let opened = openat(
            &self.dir,
            temp.as_c_str(),
            OFlag::O_WRONLY | OFlag::O_CREAT | OFlag::O_EXCL | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
            Mode::from_bits_truncate(mode as nix::sys::stat::mode_t),
        )
        .map_err(|error| {
            problem(format!(
                "Could not make a file in {}: {error}",
                self.path.display()
            ))
        })?;
        let placed = (|| -> std::io::Result<()> {
            // The exact mode, past the umask that masked the create.
            nix::sys::stat::fchmod(
                &opened,
                Mode::from_bits_truncate(mode as nix::sys::stat::mode_t),
            )?;
            let mut file = File::from(opened);
            file.write_all(bytes)?;
            file.sync_all()
        })();
        if let Err(error) = placed {
            self.remove_temp(&temp);
            return Err(problem(format!(
                "Could not write {} in {}: {error}",
                name.to_string_lossy(),
                self.path.display()
            )));
        }
        Ok(temp)
    }

    /// Remove the temporary file `temp`; one already gone is fine, and one
    /// left behind holds nothing a reader takes for the file.
    fn remove_temp(&self, temp: &CString) {
        let _ = nix::unistd::unlinkat(
            &self.dir,
            temp.as_c_str(),
            nix::unistd::UnlinkatFlags::NoRemoveDir,
        );
    }
}

/// Open the file `path` to read, proven unchangeable by any other account.
/// The reader reads the returned descriptor and never reopens by name. Only
/// a regular file is read: a FIFO, a socket or a device never blocks or
/// feeds the reader.
///
/// # Errors
///
/// Refuses Atlas's own folder, a path any other account can change, a file
/// another account may write, or a link that appeared since it was
/// stabilized; returns the I/O error of reading the path.
pub fn read(path: &Path) -> Result<Reading> {
    let (resolved, fd) = open_source(path, Intent::Read)?;
    let facts = facts_of(&fd)?;
    if facts.is_dir() {
        return Err(refused(Intent::Read, "it is a folder, not a file"));
    }
    if !facts.is_file() {
        return Err(refused(Intent::Read, "it is not a plain file"));
    }
    terminal_holds(&fd, facts, Intent::Read)?;
    // Opened without blocking, so nothing waited on it; reads block again.
    let flags = OFlag::from_bits_retain(
        fcntl(&fd, FcntlArg::F_GETFL)
            .map_err(|error| refused_io(Intent::Read, &resolved, error))?,
    );
    fcntl(&fd, FcntlArg::F_SETFL(flags - OFlag::O_NONBLOCK))
        .map_err(|error| refused_io(Intent::Read, &resolved, error))?;
    Ok(Reading {
        file: File::from(fd),
        path: resolved,
    })
}

/// Open the folder `path` to write into, proven unchangeable by any other
/// account, the folder itself included: the temporary entries a writer
/// places live there.
///
/// # Errors
///
/// As [`read`], and refuses a path that is not a folder.
pub fn folder(path: &Path) -> Result<Folder> {
    let (resolved, fd) = open_source(path, Intent::Write)?;
    let facts = facts_of(&fd)?;
    if !facts.is_dir() {
        return Err(refused(Intent::Write, "it is not a folder"));
    }
    terminal_holds(&fd, facts, Intent::Write)?;
    let dir = Dir::from_fd(fd).map_err(|error| refused_io(Intent::Write, &resolved, error))?;
    Ok(Folder {
        dir,
        path: resolved,
    })
}

/// Open the folder `path` to read from and write into, as [`folder`], and
/// only while it is this account's own, not root's, and neither its group
/// nor any other account may write to it by its mode: a folder another
/// process of this account made for its own files.
///
/// # Errors
///
/// As [`folder`], and refuses a folder another account owns, root among
/// them, or that its group or others may write to.
pub fn own_folder(path: &Path) -> Result<Folder> {
    let (resolved, fd) = open_source(path, Intent::Read)?;
    let facts = facts_of(&fd)?;
    if !facts.is_dir() {
        return Err(refused(Intent::Read, "it is not a folder"));
    }
    if facts.uid != this_uid() {
        return Err(refused(Intent::Read, "it belongs to another account"));
    }
    if facts.mode & 0o022 != 0 {
        return Err(refused(Intent::Read, "its group or others may change it"));
    }
    terminal_holds(&fd, facts, Intent::Read)?;
    let dir = Dir::from_fd(fd).map_err(|error| refused_io(Intent::Read, &resolved, error))?;
    Ok(Folder {
        dir,
        path: resolved,
    })
}

/// A private folder of this account's own at `path`, made 0700 when it is
/// not there, inside a parent proven on the way like any other path folder:
/// the folder is made and opened through that parent's descriptor, and must
/// be this account's alone. Whether it was made here, for its removal.
///
/// # Errors
///
/// As [`folder`], and refuses an existing folder that is not private to
/// this account.
pub fn private_folder(path: &Path) -> Result<Private> {
    let resolved = stabilize(path, Intent::Write)?;
    let (parent, name) = open_parent(&resolved, Intent::Write)?;
    let made = match nix::sys::stat::mkdirat(&parent, name.as_c_str(), Mode::S_IRWXU) {
        Ok(()) => true,
        Err(nix::errno::Errno::EEXIST) => false,
        Err(error) => return Err(refused_io(Intent::Write, &resolved, error)),
    };
    let fd = openat(
        &parent,
        name.as_c_str(),
        OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
        Mode::empty(),
    )
    .map_err(|error| open_error(Intent::Write, &resolved, error, "itself"))?;
    let facts = facts_of(&fd)?;
    if facts.uid != this_uid() || facts.mode & 0o077 != 0 {
        return Err(refused(
            Intent::Write,
            "it must be a private (0700) folder you own",
        ));
    }
    terminal_holds(&fd, facts, Intent::Write)?;
    let folder = Folder {
        dir: Dir::from_fd(fd).map_err(|error| refused_io(Intent::Write, &resolved, error))?,
        path: resolved,
    };
    let parent =
        Dir::from_fd(parent).map_err(|error| refused_io(Intent::Write, &folder.path, error))?;
    Ok(Private {
        folder,
        parent,
        name,
        made,
    })
}

/// A private folder opened through [`private_folder`], with the parent it
/// was made in, so it is removed through that parent's descriptor too.
#[derive(Debug)]
pub struct Private {
    folder: Folder,
    parent: Dir,
    name: CString,
    made: bool,
}

impl Private {
    /// The folder, to write into and clean.
    pub fn folder(&self) -> &Folder {
        &self.folder
    }

    /// Remove the folder when it was made here and is empty, through its
    /// parent's descriptor; one it did not make stays.
    pub fn remove_if_made(self) {
        if self.made {
            let _ = nix::unistd::unlinkat(
                &self.parent,
                self.name.as_c_str(),
                nix::unistd::UnlinkatFlags::RemoveDir,
            );
        }
    }
}

/// The real path of the source `path`, proven unchangeable by any other
/// account, a folder source's whole tree included, to hand to a copy.
///
/// # Errors
///
/// As [`read`], and refuses a folder source holding another account's entry,
/// one another account may change or add to, or anything but plain files,
/// folders and links.
pub fn handover_path(path: &Path) -> Result<PathBuf> {
    let (resolved, fd) = open_source(path, Intent::HandOver)?;
    let facts = facts_of(&fd)?;
    if !facts.is_dir() && !facts.is_file() {
        return Err(refused(
            Intent::HandOver,
            "it is not a plain file or folder",
        ));
    }
    terminal_holds(&fd, facts, Intent::HandOver)?;
    if facts.is_dir() {
        let dir =
            Dir::from_fd(fd).map_err(|error| refused_io(Intent::HandOver, &resolved, error))?;
        guard_tree(dir, this_uid(), Intent::HandOver, 0)?;
    }
    Ok(resolved)
}

/// Stabilize `path` and walk it from the root proving every folder on it and
/// the source itself cannot be changed by another account: the real path and
/// the opened source.
fn open_source(path: &Path, intent: Intent) -> Result<(PathBuf, OwnedFd)> {
    let resolved = stabilize(path, intent)?;
    let (parent, name) = open_parent(&resolved, intent)?;
    let source = openat(
        &parent,
        name.as_c_str(),
        OFlag::O_RDONLY | OFlag::O_NOFOLLOW | OFlag::O_NONBLOCK | OFlag::O_CLOEXEC,
        Mode::empty(),
    )
    .map_err(|error| open_error(intent, &resolved, error, "itself"))?;
    if !facts_of(&source)?.owned_by(this_uid()) {
        return Err(refused(intent, "it belongs to another account"));
    }
    Ok((resolved, source))
}

/// `path` made absolute and found once as it really is, Atlas's own folder
/// refused on the name given and on the real path.
fn stabilize(path: &Path, intent: Intent) -> Result<PathBuf> {
    // Atlas's own folder is refused on the named path, whether or not a file
    // is there, so a path into it is never read nor mistaken for missing.
    crate::own_folder::refuse(path)?;
    let path = std::path::absolute(path).map_err(|error| refused_io(intent, path, error))?;
    let resolved = match path.file_name() {
        // The folder as it really is, found once: a link on the way is
        // resolved here, never followed again, and the source's own last
        // component is kept as named so a link in its place is caught by
        // the walk.
        Some(name) => {
            let parent = path.parent().unwrap_or_else(|| Path::new("/"));
            std::fs::canonicalize(parent)
                .map_err(|error| refused_io(intent, &path, error))?
                .join(name)
        }
        // Ending in `..`, only a folder: resolved whole, then walked as any.
        None => std::fs::canonicalize(&path).map_err(|error| refused_io(intent, &path, error))?,
    };
    if resolved.file_name().is_none() {
        return Err(refused(intent, "it names no file"));
    }
    // Refused again on the real path: a link on the way that led into the
    // folder is caught here too.
    crate::own_folder::refuse(&resolved)?;
    Ok(resolved)
}

/// Walk the folders of `resolved` (an absolute real path) from the root,
/// each opened without following a link and proven unchangeable by another
/// account: the open last folder and the name of the entry in it.
fn open_parent(resolved: &Path, intent: Intent) -> Result<(OwnedFd, CString)> {
    let me = this_uid();
    let names: Vec<&OsStr> = resolved
        .components()
        .filter_map(|part| match part {
            Component::Normal(name) => Some(name),
            _ => None,
        })
        .collect();
    let Some((last, folders)) = names.split_last() else {
        return Err(refused(intent, "it names no file"));
    };

    let dir_flags = OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC;
    let mut parent = nix::fcntl::open(Path::new("/"), dir_flags, Mode::empty())
        .map_err(|error| refused_io(intent, resolved, error))?;
    folder_holds(&parent, me, intent)?;
    for name in folders {
        let child = openat(&parent, *name, dir_flags, Mode::empty())
            .map_err(|error| open_error(intent, resolved, error, "on its path"))?;
        folder_holds(&child, me, intent)?;
        parent = child;
    }
    Ok((parent, c_name(last)?))
}

/// Prove an open folder on the path keeps every account but this one and
/// root from changing which file its next name holds: its own, by mode and
/// ACL, except a sticky folder whose next entry is itself owner-checked.
fn folder_holds(fd: &OwnedFd, me: u32, intent: Intent) -> Result<()> {
    let facts = facts_of(fd)?;
    if !facts.owned_by(me) {
        return Err(refused(
            intent,
            "a folder on its path belongs to another account",
        ));
    }
    if facts.loosely_writable() && !facts.sticky() {
        return Err(refused(
            intent,
            "a folder on its path can be changed by another account",
        ));
    }
    if acl::allows_write_fd(fd.as_fd())
        .map_err(|error| refused_io(intent, Path::new("a folder"), error))?
    {
        return Err(refused(
            intent,
            "a folder on its path grants another account write access",
        ));
    }
    Ok(())
}

/// Prove the object itself (`fd`, of `facts`) no other account may change:
/// no group or other write by its mode, sticky or not (a sticky folder still
/// takes new entries), and no ACL entry granting a write.
fn terminal_holds(fd: &impl AsFd, facts: Facts, intent: Intent) -> Result<()> {
    if facts.loosely_writable() {
        return Err(refused(intent, "another account may change it"));
    }
    if acl::allows_write_fd(fd.as_fd())
        .map_err(|error| refused_io(intent, Path::new("it"), error))?
    {
        return Err(refused(intent, "it grants another account write access"));
    }
    Ok(())
}

/// Refuse a handover source folder (the open folder `dir`) when any entry
/// inside it, read without following a link, belongs to another account, is
/// a file or folder another account may change or add to (by mode or ACL),
/// or is anything but a plain file, a folder or a link; its subfolders the
/// same, as deep as [`crate::stage::DEEPEST`].
fn guard_tree(mut dir: Dir, me: u32, intent: Intent, depth: usize) -> Result<()> {
    if depth > crate::stage::DEEPEST {
        return Err(refused(intent, "it holds a folder too deep"));
    }
    let mut names = Vec::new();
    for entry in dir.iter() {
        let entry = entry.map_err(|error| refused_io(intent, Path::new("a folder"), error))?;
        let name = entry.file_name();
        if name.to_bytes() != b"." && name.to_bytes() != b".." {
            names.push(name.to_owned());
        }
    }
    for name in names {
        let stat = fstatat(&dir, name.as_c_str(), AtFlags::AT_SYMLINK_NOFOLLOW)
            .map_err(|error| refused_io(intent, Path::new("an entry"), error))?;
        let facts = Facts::from_stat(&stat);
        if !facts.owned_by(me) {
            return Err(refused(
                intent,
                "something inside it belongs to another account",
            ));
        }
        // A link inside is copied as a link, never followed, so only who
        // owns it matters.
        if facts.is_link() {
            continue;
        }
        if !facts.is_dir() && !facts.is_file() {
            return Err(refused(
                intent,
                "it holds something that is not a plain file, folder or link",
            ));
        }
        let flags = if facts.is_dir() {
            OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC
        } else {
            OFlag::O_RDONLY | OFlag::O_NOFOLLOW | OFlag::O_NONBLOCK | OFlag::O_CLOEXEC
        };
        let opened = openat(&dir, name.as_c_str(), flags, Mode::empty()).map_err(|error| {
            open_error(intent, Path::new("an entry inside it"), error, "inside it")
        })?;
        let opened_facts = facts_of(&opened)?;
        if Identity::of(
            &fstat(&opened).map_err(|error| refused_io(intent, Path::new("an entry"), error))?,
        ) != Identity::of(&stat)
        {
            return Err(refused(
                intent,
                "something inside it changed while it was read",
            ));
        }
        if opened_facts.loosely_writable() {
            return Err(refused(
                intent,
                "something inside it can be changed by another account",
            ));
        }
        if acl::allows_write_fd(opened.as_fd())
            .map_err(|error| refused_io(intent, Path::new("an entry inside it"), error))?
        {
            return Err(refused(
                intent,
                "something inside it grants another account write access",
            ));
        }
        if opened_facts.is_dir() {
            let sub = Dir::from_fd(opened)
                .map_err(|error| refused_io(intent, Path::new("a folder inside"), error))?;
            guard_tree(sub, me, intent, depth + 1)?;
        }
    }
    Ok(())
}

/// The owner, group and mode of a path entry.
#[derive(Clone, Copy, Debug)]
struct Facts {
    uid: u32,
    gid: u32,
    mode: u32,
}

impl Facts {
    fn from_stat(stat: &FileStat) -> Self {
        Self {
            uid: stat.st_uid,
            gid: stat.st_gid,
            mode: u32::from(stat.st_mode),
        }
    }

    fn kind(self) -> u32 {
        self.mode & u32::from(libc::S_IFMT)
    }

    fn is_dir(self) -> bool {
        self.kind() == u32::from(libc::S_IFDIR)
    }

    fn is_file(self) -> bool {
        self.kind() == u32::from(libc::S_IFREG)
    }

    fn is_link(self) -> bool {
        self.kind() == u32::from(libc::S_IFLNK)
    }

    fn owned_by(self, me: u32) -> bool {
        self.uid == me || self.uid == 0
    }

    fn loosely_writable(self) -> bool {
        self.mode & 0o002 != 0 || (self.mode & 0o020 != 0 && !TRUSTED_GROUPS.contains(&self.gid))
    }

    fn sticky(self) -> bool {
        self.mode & 0o1000 != 0
    }
}

fn facts_of(fd: &impl AsFd) -> Result<Facts> {
    Ok(Facts::from_stat(&fstat(fd).map_err(|error| {
        Error::new(Code::Io, format!("Read a descriptor: {error}"))
    })?))
}

fn this_uid() -> u32 {
    nix::unistd::getuid().as_raw()
}

fn c_name(name: &OsStr) -> Result<CString> {
    CString::new(name.as_bytes()).map_err(|_| problem("A name holds NUL"))
}

fn problem(message: impl Into<String>) -> Error {
    Error::new(Code::Io, message)
}

fn refused(intent: Intent, why: &str) -> Error {
    Error::new(
        Code::Credentials,
        format!(
            "Atlas only {} a file other accounts cannot change: {why}",
            intent.verb()
        ),
    )
    .with("next", serde_json::json!(NEXT))
}

/// What to do instead, for every refusal here.
const NEXT: &str = "Use a file in a folder only you can change, such as one in your home folder.";

fn refused_io(_intent: Intent, path: &Path, error: impl std::fmt::Display) -> Error {
    Error::new(
        Code::Credentials,
        format!("Atlas could not check {}: {error}", path.display()),
    )
}

/// Turn an open failure into the right refusal: a link that appeared since
/// the path was stabilized, something that is not a folder, or an I/O
/// failure.
fn open_error(intent: Intent, resolved: &Path, error: nix::errno::Errno, where_: &str) -> Error {
    match error {
        nix::errno::Errno::ELOOP => refused(intent, &format!("a link {where_}")),
        nix::errno::Errno::ENOTDIR => refused(intent, "something on its path is not a folder"),
        error => refused_io(intent, resolved, error),
    }
}

#[cfg(test)]
#[path = "checked_tests.rs"]
mod tests;

//! Pictures of the helper's screen taken around agents' actions, for the
//! person: one before each command, one after it when no next step's
//! picture serves, and the facts Atlas.app needs to mark them, kept in the
//! `before` folder of the private output folder for as many days as the
//! settings say (a week unless set) and 500 MB in all. A file's name says
//! when its picture was taken and whose action it came with, so keeping,
//! pruning and cleaning up never depend on the activity log. Only the
//! activity names them; no agent is given one.
//!
//! The folder is opened without following a link, and each file is read,
//! looked at, made and removed relative to it, never through a link either.
use crate::helper_info::FRAME_LIMIT;
use atlas_core::{
    error::{Code, Error, Result},
    files::{private_folder, read_up_to},
    host::Key,
    name::name_type,
};
use nix::{
    dir::{Dir, Type},
    fcntl::{AtFlags, OFlag, open, openat, renameat},
    sys::stat::{FileStat, Mode, SFlag, fstat, fstatat},
    unistd::{UnlinkatFlags, fsync, linkat, unlinkat},
};
use serde_json::{Map, Value};
use std::{
    cmp::Reverse,
    fs::File,
    io::Write,
    os::fd::{AsFd, OwnedFd},
    path::{Path, PathBuf},
    sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

/// The folder of the output folder they are kept in.
pub const FOLDER: &str = "before";
/// The longest side a picture is taken at, and the JPEG quality it is kept
/// at.
pub(crate) const SIDE: u32 = 1280;
const QUALITY: f64 = 0.6;
/// The most bytes they take in all, whatever the days they are kept.
const ROOM: u64 = 500 * 1024 * 1024;
/// What a file's temporary name adds while it is written.
const PARTIAL: &str = ".partial";
/// How long a temporary file is left to its writer before pruning takes it.
const ABANDONED: Duration = Duration::from_secs(60);
/// How a cleanup's mark ends ([`forget`]).
const FORGOTTEN: &str = ".forgotten";
/// The mark of the person's Delete All ([`delete_all`]): no session key
/// is ever this.
const EVERYTHING: &str = "all";
/// How soon a next step must begin for its picture to be this one's after.
pub(crate) const NEXT_WITHIN: Duration = Duration::from_secs(1);
/// The largest facts file read.
const FACTS_LIMIT: u64 = 1 << 20;

/// Which of an action's files a name is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Part {
    /// The picture right before it.
    Before,
    /// The picture a moment after it.
    After,
    /// What Atlas.app needs to mark it.
    Facts,
}

impl Part {
    const ALL: [Self; 3] = [Self::Before, Self::After, Self::Facts];

    fn suffix(self) -> &'static str {
        match self {
            Self::Before => "-before.jpg",
            Self::After => "-after.jpg",
            Self::Facts => "-facts.json",
        }
    }
}

/// What a file's name says: when its action's picture was taken, in
/// nanoseconds since 1970, the host session and named task whose action it
/// came with, and which of its files it is.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Named {
    at: u128,
    session: Option<Key>,
    task: Option<Key>,
    part: Part,
}

impl Named {
    /// `name` read, when it is one Atlas gives:
    /// `<nanoseconds>[-<session>[-<task>]]-before.jpg`, `-after.jpg` or
    /// `-facts.json`.
    fn read(name: &str) -> Option<Self> {
        let (part, stem) = Part::ALL
            .into_iter()
            .find_map(|part| Some((part, name.strip_suffix(part.suffix())?)))?;
        let mut parts = stem.split('-');
        let at = parts
            .next()
            .filter(|at| !at.is_empty() && at.bytes().all(|byte| byte.is_ascii_digit()))?;
        let mut key = || parts.next().map(Key::parse);
        let named = Self {
            at: at.parse().ok()?,
            session: key().map_or(Some(None), |key| key.map(Some))?,
            task: key().map_or(Some(None), |key| key.map(Some))?,
            part,
        };
        parts.next().is_none().then_some(named)
    }

    /// `name` read, when it names a picture, before or after an action.
    fn picture(name: &str) -> Option<Self> {
        Self::read(name).filter(|named| named.part != Part::Facts)
    }

    /// `name` read, when it names the picture before an action.
    fn before(name: &str) -> Option<Self> {
        Self::read(name).filter(|named| named.part == Part::Before)
    }

    fn name(&self) -> String {
        let keys: String = [&self.session, &self.task]
            .into_iter()
            .flatten()
            .map(|key| format!("-{key}"))
            .collect();
        format!("{}{keys}{}", self.at, self.part.suffix())
    }

    fn with(&self, part: Part) -> Self {
        Self {
            part,
            ..self.clone()
        }
    }
}

/// When the picture named `name`, before or after an action, was taken, in
/// nanoseconds since 1970: only names Atlas gives a picture have one.
pub fn time(name: &str) -> Option<u128> {
    Named::picture(name).map(|named| named.at)
}

/// The name of a picture taken at `at`, in nanoseconds since 1970, before an
/// action of `session` and, for a named task, of `task`.
pub(crate) fn name(at: u128, session: Option<&Key>, task: Option<&Key>) -> String {
    Named {
        at,
        session: session.cloned(),
        task: session.and(task).cloned(),
        part: Part::Before,
    }
    .name()
}

/// The name of `before`'s `part`, when `before` names a picture before an
/// action.
fn sibling(before: &str, part: Part) -> Option<String> {
    Named::before(before).map(|named| named.with(part).name())
}

/// The name of the picture taken a moment after the action `before` came
/// before.
pub(crate) fn after_name(before: &str) -> Option<String> {
    sibling(before, Part::After)
}

name_type! {
    /// The file name of a picture taken before or after an action: Atlas's
    /// own name, never a path or a screenshot's name.
    pub struct Before;
    refused(name) = format!("{name} is not a picture's name");
}

impl Before {
    /// `name` as a picture's, when [`time`] reads it.
    pub fn parse(name: &str) -> Option<Self> {
        time(name).map(|_| Self(name.to_owned()))
    }
}

/// The folder in `output`, opened without following a link: only a private
/// folder this account owns.
fn folder(output: &Path) -> Result<OwnedFd> {
    let flags = OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC;
    let folder = open(output.join(FOLDER).as_path(), flags, Mode::empty())
        .map_err(|error| Error::from(std::io::Error::from(error)))?;
    let stat = fstat(&folder).map_err(std::io::Error::from)?;
    if stat.st_uid != nix::unistd::getuid().as_raw() || stat.st_mode & 0o077 != 0 {
        return Err(Error::new(
            Code::Config,
            "The pictures' folder must be a private (0700) folder you own",
        ));
    }
    Ok(folder)
}

/// The folder in `output`, made private first when it is missing.
fn made(output: &Path) -> Result<OwnedFd> {
    private_folder(output, Code::Config)?;
    private_folder(&output.join(FOLDER), Code::Config)?;
    folder(output)
}

/// Whether `stat` is a plain file's.
fn plain(stat: &FileStat) -> bool {
    SFlag::from_bits_truncate(stat.st_mode) & SFlag::S_IFMT == SFlag::S_IFREG
}

/// The file `name` in `folder`, whole: only a plain file, never through a
/// link, and no larger than `limit`.
fn read_in(folder: &OwnedFd, name: &str, limit: u64) -> Option<Vec<u8>> {
    let flags = OFlag::O_RDONLY | OFlag::O_NOFOLLOW | OFlag::O_NONBLOCK | OFlag::O_CLOEXEC;
    let file = openat(folder, name, flags, Mode::empty()).ok()?;
    fstat(&file).ok().filter(plain)?;
    read_up_to(File::from(file), limit + 1)
        .ok()
        .filter(|bytes| bytes.len() as u64 <= limit)
}

/// The picture `name` in `output`, whole: only a plain file in its folder,
/// never through a link, and no larger than a screenshot.
///
/// # Errors
///
/// Returns an error for a picture that is missing, too large or not a plain
/// file, or a folder that is a link or not private.
pub fn read(output: &Path, name: &Before) -> Result<Vec<u8>> {
    folder(output)
        .ok()
        .and_then(|folder| read_in(&folder, name.as_str(), FRAME_LIMIT as u64))
        .ok_or_else(|| Error::new(Code::BadRequest, "No picture has that name"))
}

/// What Atlas.app needs to mark the action `before` names, as it was
/// recorded; none when its facts are not kept.
pub fn facts(output: &Path, before: &str) -> Option<Value> {
    let name = sibling(before, Part::Facts)?;
    let bytes = read_in(&folder(output).ok()?, &name, FACTS_LIMIT)?;
    serde_json::from_slice(&bytes).ok()
}

/// Packed RGB pixels of the helper's screen, three bytes each, and when
/// they were taken, in nanoseconds since 1970.
pub(crate) struct Raw {
    pub(crate) rgb: Vec<u8>,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) at: u128,
}

/// How many pictures this process is saving, and the signal that one ended.
static SAVING: Mutex<usize> = Mutex::new(0);
static SAVED: Condvar = Condvar::new();

/// Save `raw` as `name` in `output` on a thread of its own, which never holds
/// up the action or the runtime; [`settle`] bounds how long this process
/// waits for it before it ends. Pictures older than `keep` are pruned then.
pub(crate) fn save_aside(output: PathBuf, name: String, raw: Raw, keep: Duration) {
    *lock(&SAVING) += 1;
    let saving = std::thread::Builder::new()
        .name("atlas-picture".into())
        .spawn(move || {
            // A picture that cannot be saved is left out; its action stands.
            let _ = save(&output, &name, &raw, keep);
            *lock(&SAVING) -= 1;
            SAVED.notify_all();
        });
    if saving.is_err() {
        *lock(&SAVING) -= 1;
    }
}

/// Wait up to `within` for the pictures this process is saving, as it ends.
/// One still being saved then is abandoned: only its temporary file is
/// left, which a later pruning removes.
pub fn settle(within: Duration) {
    let saving = lock(&SAVING);
    // Whether every save ended or the time ran out, the process goes on.
    let _ = SAVED.wait_timeout_while(saving, within, |saving| *saving > 0);
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    // Pictures are never authority: a panicked writer leaves them as they were.
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Put `bytes` in `folder` as `name`: written whole to a temporary file
/// beside it, flushed, then linked into place, so no reader ever sees part
/// of it, never over another file of that name, or renamed over it when
/// `replace`. With `taken`, the file says it last changed then. The
/// temporary file goes either way.
fn publish(
    folder: &OwnedFd,
    name: &str,
    bytes: &[u8],
    replace: bool,
    taken: Option<SystemTime>,
) -> Result<()> {
    let partial = format!("{name}{PARTIAL}");
    let written = (|| -> Result<()> {
        let flags =
            OFlag::O_WRONLY | OFlag::O_CREAT | OFlag::O_EXCL | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC;
        let file = openat(
            folder,
            partial.as_str(),
            flags,
            Mode::S_IRUSR | Mode::S_IWUSR,
        )
        .map_err(std::io::Error::from)?;
        let mut file = File::from(file);
        file.write_all(bytes)?;
        if let Some(taken) = taken {
            file.set_modified(taken)?;
        }
        fsync(&file).map_err(std::io::Error::from)?;
        if replace {
            renameat(folder, partial.as_str(), folder, name).map_err(std::io::Error::from)?;
        } else {
            linkat(folder, partial.as_str(), folder, name, AtFlags::empty())
                .map_err(std::io::Error::from)?;
        }
        Ok(())
    })();
    // A failure to remove it leaves it to pruning.
    let _ = unlinkat(folder, partial.as_str(), UnlinkatFlags::NoRemoveDir);
    written
}

/// Keep `raw` in the private output folder `output` as a JPEG named `name`,
/// never over another picture, last changed when it was taken. One whose
/// session or task was cleaned up since it was taken is removed again. The folder is then pruned to `keep`
/// and its room.
///
/// # Errors
///
/// Returns the failure to make either folder private, encode the picture or
/// write it; its temporary file is removed then.
pub(crate) fn save(output: &Path, name: &str, raw: &Raw, keep: Duration) -> Result<()> {
    let named =
        Named::picture(name).ok_or_else(|| Error::new(Code::BadRequest, "Not a picture's name"))?;
    let folder = made(output)?;
    let (width, height) = (raw.width as usize, raw.height as usize);
    let (jpeg, _, _) = crate::helper_info::jpeg_rgb(
        &raw.rgb,
        (width, height, width.saturating_mul(3)),
        (SIDE, QUALITY),
    )?;
    // A picture says when it was taken, which for one after an action its
    // name does not.
    let taken = UNIX_EPOCH + nanos_duration(raw.at);
    publish(&folder, name, &jpeg, false, Some(taken))?;
    unless_forgotten(&folder, &named, name);
    prune_in(&folder, SystemTime::now(), ROOM, keep);
    Ok(())
}

/// Keep `facts` as what Atlas.app needs to mark the action `before` names,
/// in place of any kept so far.
///
/// # Errors
///
/// Returns the failure to make either folder private or write the facts.
pub(crate) fn keep_facts(output: &Path, before: &str, facts: &Value) -> Result<()> {
    let named = Named::before(before)
        .ok_or_else(|| Error::new(Code::BadRequest, "Not a picture's name"))?
        .with(Part::Facts);
    let name = named.name();
    let folder = made(output)?;
    publish(&folder, &name, &serde_json::to_vec(facts)?, true, None)?;
    unless_forgotten(&folder, &named, &name);
    Ok(())
}

/// Remove `name` again when a cleanup marked its session, or the person
/// deleted all, at or after `named` was taken: that may have looked before
/// it was in place.
fn unless_forgotten(folder: &OwnedFd, named: &Named, name: &str) {
    if forgotten_since(folder, named) {
        let _ = unlinkat(folder, name, UnlinkatFlags::NoRemoveDir);
    }
}

/// Prune the files in `output` to `keep` and 500 MB, as Atlas's viewer does
/// from when it starts, and say how long until the oldest left expires:
/// `keep` when none is left, as any picture taken from now on expires
/// later. A folder that cannot be opened is left as it is.
pub fn prune(output: &Path, keep: Duration) -> Duration {
    let now = SystemTime::now();
    let oldest = folder(output)
        .ok()
        .and_then(|folder| prune_in(&folder, now, ROOM, keep));
    oldest.map_or(keep, |at| {
        let expires = UNIX_EPOCH + nanos_duration(at) + keep;
        expires.duration_since(now).unwrap_or_default()
    })
}

fn nanos_duration(nanos: u128) -> Duration {
    Duration::from_nanos(u64::try_from(nanos).unwrap_or(u64::MAX))
}

/// What a file in the folder is, by its name.
enum Kind {
    Kept(Named),
    /// A file still being written, or one whose writer was abandoned.
    Partial(Named),
    /// When a cleanup forgot a session's or task's pictures: its time.
    Forgotten,
}

/// One file in the folder: its name, what it is, its size and when it was
/// last changed, in nanoseconds since 1970.
struct Entry {
    file: String,
    kind: Kind,
    size: u64,
    modified: u128,
}

/// A cleanup's mark: `<session>[-<task>].forgotten`.
fn mark(session: &Key, task: Option<&Key>) -> String {
    task.map_or_else(
        || format!("{session}{FORGOTTEN}"),
        |task| format!("{session}-{task}{FORGOTTEN}"),
    )
}

/// The mark of the person's Delete All.
fn everything_mark() -> String {
    format!("{EVERYTHING}{FORGOTTEN}")
}

/// When `stat` says its file last changed, in nanoseconds since 1970.
fn modified(stat: &FileStat) -> u128 {
    let seconds = u128::try_from(stat.st_mtime).unwrap_or(0);
    let nanos = u128::try_from(stat.st_mtime_nsec).unwrap_or(0);
    seconds * 1_000_000_000 + nanos
}

/// The plain files in `folder` that are Atlas's, read once.
fn entries(folder: &OwnedFd) -> Vec<Entry> {
    let Some(listing) = folder.try_clone().ok().and_then(|fd| Dir::from_fd(fd).ok()) else {
        return Vec::new();
    };
    listing
        .into_iter()
        .flatten()
        .filter(|entry| matches!(entry.file_type(), Some(Type::File) | None))
        .filter_map(|entry| {
            let file = entry.file_name().to_str().ok()?.to_owned();
            let kind = match file.strip_suffix(PARTIAL) {
                Some(kept) => Kind::Partial(Named::read(kept)?),
                None if file.ends_with(FORGOTTEN) => Kind::Forgotten,
                None => Kind::Kept(Named::read(&file)?),
            };
            let stat = fstatat(folder.as_fd(), file.as_str(), AtFlags::AT_SYMLINK_NOFOLLOW).ok()?;
            plain(&stat).then(|| Entry {
                file,
                kind,
                size: u64::try_from(stat.st_size).unwrap_or(0),
                modified: modified(&stat),
            })
        })
        .collect()
}

/// Remove the files in `folder` of actions older than `keep`, then those of
/// the oldest of the rest until they take at most `room` bytes; the
/// temporary files of writers abandoned a minute ago or more; and cleanup
/// marks older than any action kept. Something that cannot be removed is
/// tried again at the next pruning. The time of the oldest action left.
fn prune_in(folder: &OwnedFd, now: SystemTime, room: u64, keep: Duration) -> Option<u128> {
    let now = now.duration_since(UNIX_EPOCH).unwrap_or_default();
    let before = |age: Duration| now.saturating_sub(age).as_nanos();
    let mut kept = Vec::new();
    for entry in entries(folder) {
        let stale = match &entry.kind {
            Kind::Kept(named) => {
                kept.push((named.at, entry.size, entry.file));
                continue;
            }
            Kind::Partial(named) => named.at < before(ABANDONED),
            Kind::Forgotten => entry.modified < before(keep),
        };
        if stale {
            let _ = unlinkat(folder, entry.file.as_str(), UnlinkatFlags::NoRemoveDir);
        }
    }
    kept.sort_by_key(|(at, _, _)| Reverse(*at));
    let mut used = 0_u64;
    let mut oldest = None;
    for (at, size, file) in kept {
        used = used.saturating_add(size);
        if at < before(keep) || used > room {
            let _ = unlinkat(folder, file.as_str(), UnlinkatFlags::NoRemoveDir);
        } else {
            oldest = Some(at);
        }
    }
    oldest
}

/// Whether the person deleted all, or a cleanup marked `named`'s session
/// or its task, at or after it was taken.
fn forgotten_since(folder: &OwnedFd, named: &Named) -> bool {
    let session = named.session.as_ref();
    let marks = [
        Some(everything_mark()),
        session.map(|session| mark(session, None)),
        session
            .zip(named.task.as_ref())
            .map(|(session, task)| mark(session, Some(task))),
    ];
    marks.into_iter().flatten().any(|marked| {
        fstatat(folder, marked.as_str(), AtFlags::AT_SYMLINK_NOFOLLOW)
            .is_ok_and(|stat| plain(&stat) && modified(&stat) >= named.at)
    })
}

/// Remove the files of the actions of `session`, or only of its named
/// `task`, as a cleanup of their work does: it first marks when it did, so
/// a file still being written for them is removed once it is in place. A
/// folder that cannot be made or opened has none to remove.
pub(crate) fn forget(output: &Path, session: &Key, task: Option<&Key>) {
    // The folder is made first, so the mark is kept even before any
    // picture is: one saved later still finds it.
    let Ok(folder) = made(output) else {
        return;
    };
    // Without its mark a cleanup still removes what is in place now.
    let _ = touch(&folder, &mark(session, task));
    for entry in entries(&folder) {
        let (Kind::Kept(named) | Kind::Partial(named)) = &entry.kind else {
            continue;
        };
        if named.session.as_ref() == Some(session)
            && task.is_none_or(|task| named.task.as_ref() == Some(task))
        {
            let _ = unlinkat(&folder, entry.file.as_str(), UnlinkatFlags::NoRemoveDir);
        }
    }
}

/// Make the mark `name` in `folder`, or mark it again: its time is now.
fn touch(folder: &OwnedFd, name: &str) -> std::io::Result<()> {
    let flags =
        OFlag::O_WRONLY | OFlag::O_CREAT | OFlag::O_NOFOLLOW | OFlag::O_NONBLOCK | OFlag::O_CLOEXEC;
    let file = openat(folder, name, flags, Mode::S_IRUSR | Mode::S_IWUSR)
        .map(File::from)
        .map_err(std::io::Error::from)?;
    if file.metadata()?.is_file() {
        file.set_modified(SystemTime::now())?;
    }
    Ok(())
}

/// How many pictures `output` keeps and the bytes its files take, for the
/// person's Settings.
pub fn summary(output: &Path) -> (u64, u64) {
    let Ok(folder) = folder(output) else {
        return (0, 0);
    };
    entries(&folder)
        .iter()
        .fold((0, 0), |(count, bytes), entry| match &entry.kind {
            Kind::Kept(named) => (
                count + u64::from(named.part != Part::Facts),
                bytes + entry.size,
            ),
            Kind::Partial(_) | Kind::Forgotten => (count, bytes + entry.size),
        })
}

/// Remove every file of every action in `output`, as the person's Delete
/// All does. It first marks when it did, so a picture or facts still being
/// written of an action taken by then are removed once in place, and none
/// is kept again; cleanup marks stay, for the same reason.
///
/// # Errors
///
/// Returns the failure to make or open the folder, to mark it, or to remove
/// a file: without its mark, a picture still being written could be kept
/// again, so nothing is said to be deleted.
pub fn delete_all(output: &Path) -> Result<()> {
    let folder = made(output)?;
    touch(&folder, &everything_mark())?;
    let mut left = 0_usize;
    for entry in entries(&folder) {
        if matches!(entry.kind, Kind::Forgotten) {
            continue;
        }
        match unlinkat(&folder, entry.file.as_str(), UnlinkatFlags::NoRemoveDir) {
            // Pruned or cleaned up meanwhile.
            Ok(()) | Err(nix::errno::Errno::ENOENT) => {}
            Err(_) => left += 1,
        }
    }
    if left > 0 {
        return Err(Error::new(
            Code::Io,
            format!("{left} of the pictures could not be deleted; try again"),
        ));
    }
    Ok(())
}

/// The files kept in a folder, read once, to name each action's pictures:
/// each with when it last changed, in nanoseconds since 1970.
pub struct Listing(Vec<(Named, u128)>);

impl Listing {
    /// The files kept in `output`; none when its folder cannot be opened.
    pub fn of(output: &Path) -> Self {
        let kept = folder(output)
            .map(|folder| {
                entries(&folder)
                    .into_iter()
                    .filter_map(|entry| match entry.kind {
                        Kind::Kept(named) => Some((named, entry.modified)),
                        Kind::Partial(_) | Kind::Forgotten => None,
                    })
                    .collect()
            })
            .unwrap_or_default();
        Self(kept)
    }

    /// Whether the picture `name` is kept.
    pub fn has(&self, name: &str) -> bool {
        Named::picture(name).is_some_and(|named| self.0.iter().any(|(kept, _)| *kept == named))
    }

    /// The picture after the action `before` names, which `ended` at, in
    /// milliseconds since 1970, and when it was taken, in milliseconds
    /// since 1970: its own, taken a moment later, which says when it was
    /// taken by when it last changed, or else the picture before the same
    /// session's next action, when that began within [`NEXT_WITHIN`], which
    /// says by its name.
    pub fn after(&self, before: &str, ended: u64) -> Option<(String, u64)> {
        let millis = |nanos: u128| u64::try_from(nanos / 1_000_000).unwrap_or(u64::MAX);
        let named = Named::before(before)?;
        let own = named.with(Part::After);
        if let Some((_, modified)) = self.0.iter().find(|(kept, _)| *kept == own) {
            return Some((own.name(), millis(*modified)));
        }
        self.next(&named, ended)
            .map(|next| (next.name(), millis(next.at)))
    }

    /// The picture before the same session's next action after `named`'s,
    /// which `ended` at, when that began within [`NEXT_WITHIN`].
    fn next(&self, named: &Named, ended: u64) -> Option<&Named> {
        let ended = u128::from(ended) * 1_000_000;
        let within = ended + NEXT_WITHIN.as_nanos();
        self.0
            .iter()
            .map(|(next, _)| next)
            .filter(|next| {
                next.part == Part::Before
                    && next.session == named.session
                    && next.at > named.at
                    && next.at <= within
            })
            .min_by_key(|next| next.at)
    }
}

/// Whether the action `before` names, which `ended` at in milliseconds
/// since 1970, needs a picture of its own after it: no next step's picture
/// serves.
pub(crate) fn wants_after(output: &Path, before: &str, ended: u64) -> bool {
    Named::before(before).is_some_and(|named| Listing::of(output).next(&named, ended).is_none())
}

/// A command's picture: taken before its first input only, named on its
/// rows from when it is in hand, before it is saved, with the facts of the
/// action it came before and the text the command typed. Clones share it,
/// as the clients of one command do.
#[derive(Clone, Debug, Default)]
pub struct Slot(Arc<Mutex<Held>>);

#[derive(Debug, Default)]
struct Held {
    taken: Taken,
    /// What the command's first input did, for its facts.
    facts: Map<String, Value>,
    /// What the command found to act on before its first input, for its
    /// facts: the element and its window.
    found: Map<String, Value>,
    /// What the command typed.
    typed: Typed,
}

/// What a command typed: all its text, once it has typed it all, and the
/// role of the field each of its inputs that typed went into, as the
/// helper found it, none where it found none.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Typed {
    pub(crate) text: Option<String>,
    pub(crate) into: Vec<Option<String>>,
}

#[derive(Debug, Default)]
enum Taken {
    #[default]
    No,
    Taking,
    Named(String),
}

impl Slot {
    /// Whether the picture is this caller's to take: only the first time.
    pub(crate) fn take(&self) -> bool {
        let mut held = self.lock();
        let first = matches!(held.taken, Taken::No);
        if first {
            held.taken = Taken::Taking;
        }
        first
    }

    /// The picture is in hand, to be saved as `name`, before an input that
    /// did what `facts` say.
    pub(crate) fn named(&self, name: String, facts: Map<String, Value>) {
        let mut held = self.lock();
        held.taken = Taken::Named(name);
        held.facts = facts;
    }

    /// The picture's name, once it is in hand.
    pub(crate) fn name(&self) -> Option<String> {
        match &self.lock().taken {
            Taken::Named(name) => Some(name.clone()),
            Taken::No | Taken::Taking => None,
        }
    }

    /// The command found what `found` says to act on, before its first
    /// input: kept only while no picture is being taken, which is of that
    /// input.
    pub(crate) fn found(&self, found: Map<String, Value>) {
        let mut held = self.lock();
        if matches!(held.taken, Taken::No) {
            held.found = found;
        }
    }

    /// The picture's name, once it is in hand, and its input's facts with
    /// what the command found to act on.
    pub(crate) fn facts(&self) -> Option<(String, Map<String, Value>)> {
        let held = self.lock();
        match &held.taken {
            Taken::Named(name) => {
                let mut facts = held.facts.clone();
                facts.extend(held.found.clone());
                Some((name.clone(), facts))
            }
            Taken::No | Taken::Taking => None,
        }
    }

    /// The command typed `text`, all it types.
    pub(crate) fn typing(&self, text: &str) {
        self.lock().typed.text = Some(text.to_owned());
    }

    /// One of the command's inputs typed into a field the helper found of
    /// `role`, or into one it could not tell when none.
    pub(crate) fn typed_into(&self, role: Option<&str>) {
        self.lock().typed.into.push(role.map(str::to_owned));
    }

    /// What the command typed so far.
    pub(crate) fn typed(&self) -> Typed {
        self.lock().typed.clone()
    }

    fn lock(&self) -> MutexGuard<'_, Held> {
        lock(&self.0)
    }
}

/// One command's slot is only ever equal to itself.
impl PartialEq for Slot {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for Slot {}

#[cfg(test)]
mod tests;

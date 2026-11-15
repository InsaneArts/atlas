//! The copies a machine desktop keeps: listed with what runs from them,
//! and removed as a change with an ID. A removal writes its record before
//! anything moves and keeps it until what it came to is kept, so a daemon
//! that ends at any point leaves enough for the next one to finish it, or
//! to put the copy back, and never to delete what it was not asked to.
use super::{Root, Stages, placed, staging, write};
use crate::machine::{
    changes::{Changes, State},
    vault::Vault,
};
use atlas_core::{
    error::Code,
    files::Token,
    fs_move::{Replace, rename_within},
    protocol::{failure, interrupted},
};
use nix::{
    dir::Dir,
    fcntl::{AtFlags, OFlag, openat},
    sys::stat::{Mode, fstat, fstatat},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    ffi::CString,
    fs::File,
    io::{self, Write},
    os::fd::{AsFd, OwnedFd},
    path::{Path, PathBuf},
};

/// The operation a removal is kept under in the change records.
pub(crate) const OPERATION: &str = "remove-copy";

/// The most names one page of the inventory lists.
const PAGE: usize = 256;

/// The most processes named for one copy.
const PIDS: usize = 64;

/// Where a removal stands, as its record says.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum Phase {
    /// Moved, or about to be, and not yet checked again: recovery puts it
    /// back.
    Removing,
    /// Checked under both names, to be deleted.
    Deleting,
    /// To be put back, never deleted.
    PuttingBack,
    /// Deleted.
    Removed,
    /// Left where it was, or put back: why is the outcome.
    Kept,
}

/// A removal's record, beside the copy it is removing.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Record {
    pub(crate) change: Token,
    pub(crate) token: Token,
    /// The copy's folder as it was looked at: `dev:ino`.
    pub(crate) node: String,
    pub(crate) phase: Phase,
    /// What a kept copy came to: `changed`, `inUse` or `unknown`.
    pub(crate) why: Option<String>,
}

/// What a removal came to: kept before it is said, or, when it was cut
/// short, said as uncertain and left to be put right.
#[derive(Debug)]
pub(crate) enum Removal {
    Came(Value),
    Pending(Value),
}

/// The processes of this account and the paths each runs from or names,
/// as far as they can be told.
pub(crate) struct Running {
    by: Vec<(i32, Vec<PathBuf>)>,
    /// Some process's paths could not be read: no copy counts as unused.
    unknown: bool,
}

/// What a process was read to run from.
enum Read {
    /// Its program, its arguments and its folder, each as the kernel gave it.
    Paths {
        executable: Option<PathBuf>,
        arguments: Vec<String>,
        folder: Option<PathBuf>,
    },
    /// It ended, even if not yet reaped, or its pid is another's now.
    Ended,
    /// It could not be read, even a few times over.
    Unreadable,
}

impl Read {
    /// How many times a process that gives nothing is read before it
    /// counts as unreadable: one mid-exec or mid-exit gives nothing for a
    /// moment.
    const TRIES: usize = 5;

    /// Read `instance`'s paths, then read it again: a process that ended
    /// meanwhile, or a pid given to another, is not counted.
    fn of(instance: atlas_core::process::Instance) -> Self {
        for tried in 1..=Self::TRIES {
            let pid = instance.pid;
            let executable = u32::try_from(pid)
                .ok()
                .and_then(atlas_core::app_bundle::signature::executable_of);
            let arguments = atlas_core::process::arguments(pid);
            let folder = atlas_core::process::cwd(instance);
            match atlas_core::process::info(pid) {
                Ok(Some(again)) if again.instance == instance && !again.zombie => {}
                Ok(_) => return Self::Ended,
                Err(_) => return Self::Unreadable,
            }
            // A process whose program was replaced or deleted since it
            // started has no path the kernel gives: its arguments still name
            // it, so it is never counted without them. Nor is one with an
            // argument relative to a folder unread: the kernel gives even a
            // deleted folder's path, so an unread one is a failed read.
            if let Ok(arguments) = arguments {
                let unplaced = folder.is_none()
                    && arguments
                        .iter()
                        .enumerate()
                        .any(|(at, argument)| relative(at, argument));
                if !unplaced {
                    return Self::Paths {
                        executable,
                        arguments,
                        folder,
                    };
                }
            }
            if tried < Self::TRIES {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        }
        Self::Unreadable
    }
}

/// Whether argument `at`, `argument`, is a path relative to its process's
/// folder: not absolute, not an option, and not a program named bare, which
/// was found on the search path, not there.
fn relative(at: usize, argument: &str) -> bool {
    let bare = at == 0 && !argument.contains('/');
    !Path::new(argument).is_absolute() && !bare && !argument.starts_with('-')
}

impl Running {
    /// Read them now.
    pub(crate) fn now() -> Self {
        let uid = nix::unistd::getuid().as_raw();
        let Ok(processes) = atlas_core::process::of_user(uid) else {
            return Self {
                by: Vec::new(),
                unknown: true,
            };
        };
        let mut running = Self {
            by: Vec::new(),
            unknown: false,
        };
        for process in processes.into_iter().filter(|process| !process.zombie) {
            let pid = process.instance.pid;
            let (executable, arguments, folder) = match Read::of(process.instance) {
                Read::Paths {
                    executable,
                    arguments,
                    folder,
                } => (executable, arguments, folder),
                Read::Ended => continue,
                Read::Unreadable => {
                    running.unknown = true;
                    continue;
                }
            };
            let mut paths: Vec<PathBuf> = executable.into_iter().collect();
            for (at, argument) in arguments.iter().enumerate() {
                let named = Path::new(argument);
                if named.is_absolute() {
                    paths.push(named.to_owned());
                } else if relative(at, argument) {
                    // A path relative to where the process itself is: its
                    // folder was read whenever such a path is there.
                    if let Some(folder) = &folder {
                        paths.push(folder.join(named));
                    }
                }
            }
            // A path that climbs out with `..` through what is gone cannot be
            // placed: where it runs from is unknown.
            let climbs = |path: &PathBuf| {
                std::fs::canonicalize(path).is_err()
                    && path
                        .components()
                        .any(|part| part == std::path::Component::ParentDir)
            };
            if paths.iter().any(climbs) {
                running.unknown = true;
                continue;
            }
            let paths = paths.iter().map(|path| resolved(path)).collect();
            running.by.push((pid, paths));
        }
        running
    }

    /// The processes running from `folder` or naming what is in it; none
    /// when that cannot be told.
    pub(crate) fn from(&self, folder: &Path) -> Option<Vec<i32>> {
        self.within(&std::fs::canonicalize(folder).ok()?)
    }

    /// The processes running from the canonical path `folder`, which need
    /// not be there any more, or naming what is in it.
    fn within(&self, folder: &Path) -> Option<Vec<i32>> {
        if self.unknown {
            return None;
        }
        Some(
            self.by
                .iter()
                .filter(|(_, paths)| paths.iter().any(|path| path.starts_with(folder)))
                .map(|(pid, _)| *pid)
                .take(PIDS)
                .collect(),
        )
    }
}

/// `path` made canonical as far as it still exists: its nearest ancestor
/// that does, with the rest after it. A path into a copy since moved or
/// deleted still names that copy.
fn resolved(path: &Path) -> PathBuf {
    let mut ancestor = path;
    let mut rest = Vec::new();
    loop {
        if let Ok(found) = std::fs::canonicalize(ancestor) {
            return rest.iter().rev().fold(found, |path, part| path.join(part));
        }
        match (ancestor.parent(), ancestor.file_name()) {
            (Some(parent), Some(name)) => {
                rest.push(name.to_owned());
                ancestor = parent;
            }
            _ => return path.to_owned(),
        }
    }
}

/// The name a copy has while it is removed.
fn removing(token: &Token) -> CString {
    CString::new(format!(".removing-{}", token.as_str())).unwrap_or_default()
}

/// The name of a removal's record.
fn record_name(token: &Token) -> String {
    format!(".removing-{}.json", token.as_str())
}

/// The folder `name` of `root`'s identity, read without following it.
fn node_of(root: &Root, name: &CString) -> io::Result<String> {
    let stat = fstatat(&root.folder, name.as_c_str(), AtFlags::AT_SYMLINK_NOFOLLOW)?;
    Ok(format!("{}:{}", stat.st_dev, stat.st_ino))
}

/// The identity of the open `file`.
fn identity(file: &impl AsFd) -> io::Result<String> {
    let stat = fstat(file)?;
    Ok(format!("{}:{}", stat.st_dev, stat.st_ino))
}

/// Keep `record` in place of any before it, durably: written beside, synced,
/// renamed over, and the root synced.
fn keep(root: &Root, record: &Record) -> io::Result<()> {
    let name = record_name(&record.token);
    let beside = CString::new(format!("{name}.new")).map_err(io::Error::other)?;
    let _ = nix::unistd::unlinkat(
        &root.folder,
        beside.as_c_str(),
        nix::unistd::UnlinkatFlags::NoRemoveDir,
    );
    let flags =
        OFlag::O_WRONLY | OFlag::O_CREAT | OFlag::O_EXCL | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC;
    let mut file = File::from(openat(
        &root.folder,
        beside.as_c_str(),
        flags,
        Mode::S_IRUSR | Mode::S_IWUSR,
    )?);
    file.write_all(&serde_json::to_vec(record)?)?;
    file.sync_all()?;
    let named = CString::new(name).map_err(io::Error::other)?;
    rename_within(&root.folder, &beside, &named, Replace::Allowed)?;
    root.folder.sync_all()
}

/// The record of a removal of `token`, when there is one and it reads.
fn record_of(root: &Root, token: &Token) -> Option<Record> {
    let read = write::read_small(&root.folder, &record_name(token)).ok()?;
    serde_json::from_slice(&read).ok()
}

/// Forget the record of `token`'s removal, durably. Nothing there is
/// nothing to do.
///
/// # Errors
///
/// Fails when it cannot be removed, or that cannot be made durable.
pub(crate) fn forget(root: &Root, token: &Token) -> io::Result<()> {
    let named = CString::new(record_name(token)).map_err(io::Error::other)?;
    match nix::unistd::unlinkat(
        &root.folder,
        named.as_c_str(),
        nix::unistd::UnlinkatFlags::NoRemoveDir,
    ) {
        Ok(()) | Err(nix::errno::Errno::ENOENT) => {}
        Err(error) => return Err(error.into()),
    }
    root.folder.sync_all()
}

/// The reply a removal gives.
fn outcome(token: &Token, outcome: &str, pids: &[i32]) -> Value {
    json!({"ok": true, "op": "remove-copy", "copy": token, "outcome": outcome, "pids": pids})
}

/// Remove `token`'s copy for the change `change`, while it is still the
/// folder `node` names and nothing runs from it. `authorize` is asked
/// right before it moves, and refuses it when it may no longer.
pub(crate) fn remove(
    root: &Root,
    change: &Token,
    token: &Token,
    node: &str,
    authorize: impl FnOnce() -> Option<Value>,
) -> Removal {
    let unknown = || {
        Removal::Pending(interrupted(
            Code::Uncertain,
            "The copy's removal was cut short",
        ))
    };
    let name = placed(token);
    match write::exists(&root.folder, &name) {
        Ok(true) => {}
        Ok(false) => return Removal::Came(outcome(token, "alreadyGone", &[])),
        Err(_) => return Removal::Came(failure(Code::Io, "The copy could not be looked at")),
    }
    let Ok(folder) = write::open_folder(&root.folder, &name) else {
        return Removal::Came(failure(Code::Io, "The copy could not be opened"));
    };
    let Ok(seen) = identity(&folder) else {
        return Removal::Came(failure(Code::Io, "The copy could not be looked at"));
    };
    if seen != node {
        return Removal::Came(outcome(token, "changed", &[]));
    }
    let Ok(copy) = std::fs::canonicalize(root.path.join(token.as_str())) else {
        return Removal::Came(failure(Code::Io, "The copy could not be looked at"));
    };
    match Running::now().within(&copy) {
        None => return Removal::Came(outcome(token, "unknown", &[])),
        Some(pids) if !pids.is_empty() => return Removal::Came(outcome(token, "inUse", &pids)),
        Some(_) => {}
    }
    let mut record = Record {
        change: change.clone(),
        token: token.clone(),
        node: seen.clone(),
        phase: Phase::Removing,
        why: None,
    };
    // Kept before anything moves: without it, nothing ever is.
    if keep(root, &record).is_err() {
        return match forget(root, token) {
            Ok(()) => Removal::Came(failure(Code::Io, "The copy's removal could not begin")),
            Err(_) => unknown(),
        };
    }
    if let Some(refusal) = authorize() {
        return match forget(root, token) {
            Ok(()) => Removal::Came(refusal),
            Err(_) => unknown(),
        };
    }
    let moved_to = removing(token);
    if rename_within(&root.folder, &name, &moved_to, Replace::Never).is_err() {
        return match forget(root, token) {
            Ok(()) => Removal::Came(failure(
                Code::Io,
                "The copy could not be moved to be removed",
            )),
            Err(_) => unknown(),
        };
    }
    let _ = root.folder.sync_all();
    // What moved must be what was looked at; and nothing may have started
    // from it since, under either name.
    let same = write::open_folder(&root.folder, &moved_to)
        .ok()
        .and_then(|moved| identity(&moved).ok())
        .is_some_and(|moved| moved == seen);
    let running = Running::now();
    let kept_why = if !same {
        Some(("changed", Vec::new()))
    } else {
        let moved_path = root.path.join(moved_to.to_string_lossy().as_ref());
        match (running.from(&moved_path), running.within(&copy)) {
            (Some(now), Some(then)) if now.is_empty() && then.is_empty() => None,
            (Some(now), Some(then)) => Some(("inUse", [now, then].concat())),
            _ => Some(("unknown", Vec::new())),
        }
    };
    if let Some((why, pids)) = kept_why {
        return put_back(root, &mut record, why, &pids);
    }
    // Checked under both names: only from here may a recovery delete it.
    record.phase = Phase::Deleting;
    if keep(root, &record).is_err() {
        // Its record may say either: recovery, which checks both names
        // again before deleting, finishes it.
        return unknown();
    }
    let deleted = write::discard_as(&root.folder, &moved_to, &record.node);
    if deleted.is_err() || root.folder.sync_all().is_err() {
        return unknown();
    }
    record.phase = Phase::Removed;
    // Gone already: a record that still says deleting, with nothing left
    // under either name, says removed too.
    let _ = keep(root, &record);
    Removal::Came(outcome(token, "removed", &[]))
}

/// Put back the copy `record` is removing, first saying so in its record,
/// so no recovery ever deletes it.
fn put_back(root: &Root, record: &mut Record, why: &str, pids: &[i32]) -> Removal {
    let unknown = || {
        Removal::Pending(interrupted(
            Code::Uncertain,
            "The copy is still being put back",
        ))
    };
    // A removal not yet deleting is put back by recovery even unsaid; one
    // that was deleting is put back only once its record says so.
    let was_deleting = record.phase == Phase::Deleting;
    record.phase = Phase::PuttingBack;
    record.why = Some(why.to_owned());
    if keep(root, record).is_err() && was_deleting {
        return unknown();
    }
    let back = rename_within(
        &root.folder,
        &removing(&record.token),
        &placed(&record.token),
        Replace::Never,
    );
    if back.is_err() || root.folder.sync_all().is_err() {
        return unknown();
    }
    record.phase = Phase::Kept;
    let _ = keep(root, record);
    Removal::Came(outcome(&record.token, why, pids))
}

/// What the removal `record` describes came to, finishing it now that
/// none is running: none when that cannot be told. Only a removal its
/// record says was deleting is ever deleted; any other is put back.
fn finish(root: &Root, record: &mut Record) -> Option<Value> {
    let token = record.token.clone();
    let moved = write::exists(&root.folder, &removing(&token)).ok()?;
    let there = write::exists(&root.folder, &placed(&token)).ok()?;
    let why = record.why.clone().unwrap_or_else(|| "changed".to_owned());
    match (record.phase, moved) {
        (Phase::Removed, _) | (Phase::Deleting, false) => Some(outcome(&token, "removed", &[])),
        (Phase::Kept, _) => Some(outcome(&token, &why, &[])),
        (Phase::Removing, false) => {
            // Never moved, or gone by other hands: which, its name says.
            let came = if there { "notRemoved" } else { "alreadyGone" };
            Some(outcome(&token, came, &[]))
        }
        (Phase::PuttingBack, false) => there.then(|| outcome(&token, &why, &[])),
        (Phase::Deleting, true) => {
            let moved_path = root.path.join(removing(&token).to_string_lossy().as_ref());
            // An identity that cannot be read leaves it deleting, to be tried again.
            let same = node_of(root, &removing(&token)).ok()? == record.node;
            let running = Running::now().from(&moved_path);
            match (same, running) {
                (true, Some(pids)) if pids.is_empty() => {
                    // A deletion that fails, even partway, stays deleting.
                    write::discard_as(&root.folder, &removing(&token), &record.node).ok()?;
                    root.folder.sync_all().ok()?;
                    Some(outcome(&token, "removed", &[]))
                }
                (true, Some(_)) => back(root, record, there, "inUse"),
                (true, None) => back(root, record, there, "unknown"),
                (false, _) => back(root, record, there, "changed"),
            }
        }
        (Phase::Removing | Phase::PuttingBack, true) => back(root, record, there, &why),
    }
}

/// Put back the copy `record` moved, unless its name is taken again.
fn back(root: &Root, record: &mut Record, there: bool, why: &str) -> Option<Value> {
    if there {
        return None;
    }
    match put_back(root, record, why, &[]) {
        Removal::Came(reply) => Some(reply),
        Removal::Pending(_) => None,
    }
}

/// Put right whatever an earlier removal or stage of `token` left, and
/// keep what its change came to: whether nothing of it is left.
pub(crate) fn put_right<V: Vault>(root: &Root, changes: &Changes<V>, token: &Token) -> bool {
    let named = CString::new(record_name(token)).unwrap_or_default();
    match write::exists(&root.folder, &named) {
        Ok(false) => {}
        Ok(true) => {
            let Some(mut record) = record_of(root, token) else {
                return false;
            };
            let Some(reply) = finish(root, &mut record) else {
                return false;
            };
            // Kept for its change while that is intended; a record that
            // cannot be read leaves the removal's own record in place.
            match changes.of(&record.change) {
                Ok(Some(State::Intended)) => {
                    if changes.done(&record.change, &reply).is_err() {
                        return false;
                    }
                }
                Ok(_) => {}
                Err(_) => return false,
            }
            if forget(root, token).is_err() {
                return false;
            }
        }
        Err(_) => return false,
    }
    // A stage of it left intended is put right first, folder or not, as at
    // start: its evidence says what it came to, and goes with the copy.
    let Ok(intended) = changes.intended() else {
        return false;
    };
    let stage = intended
        .into_iter()
        .find(|intended| intended.change == *token && intended.operation == super::OPERATION);
    if let Some(stage) = stage {
        let Some(reply) = super::came_to(root, &stage) else {
            return false;
        };
        if changes.done(token, &reply).is_err() {
            return false;
        }
    }
    // Nothing else of it may be left over.
    let leftover = write::exists(&root.folder, &staging(token)).unwrap_or(true)
        || write::exists(&root.folder, &removing(token)).unwrap_or(true);
    !leftover
}

/// Finish every removal a daemon before this one left. Then each intended
/// removal no record names removed nothing, as its record is written
/// before anything moves; but only while every record could be read, as
/// one that cannot could be its.
///
/// # Errors
///
/// Fails when the change records cannot be read.
pub(crate) fn recover<V: Vault>(
    root: &Root,
    changes: &Changes<V>,
) -> atlas_core::error::Result<()> {
    let mut named = Vec::new();
    let mut unreadable = false;
    for name in names(root)? {
        if let Some(rest) = name.strip_prefix(".removing-") {
            // A record never renamed into place says nothing: the one in
            // place, if any, is the record.
            if rest.ends_with(".json.new") {
                let named = CString::new(name.as_str()).unwrap_or_default();
                let _ = nix::unistd::unlinkat(
                    &root.folder,
                    named.as_c_str(),
                    nix::unistd::UnlinkatFlags::NoRemoveDir,
                );
                continue;
            }
            let Some(token) = rest.strip_suffix(".json").and_then(Token::parse) else {
                // A folder being removed has its record beside it.
                unreadable |=
                    Token::parse(rest).is_none_or(|token| record_of(root, &token).is_none());
                continue;
            };
            match record_of(root, &token) {
                Some(record) => named.push(record.change.clone()),
                None => unreadable = true,
            }
            put_right(root, changes, &token);
        }
    }
    if unreadable {
        return Ok(());
    }
    for intended in changes.intended()? {
        if intended.operation == OPERATION && !named.contains(&intended.change) {
            let reply = failure(
                Code::Cancelled,
                "Nothing was removed: the daemon ended before it began",
            );
            changes.done(&intended.change, &reply)?;
        }
    }
    Ok(())
}

/// Every name in `root`, sorted.
fn names(root: &Root) -> io::Result<Vec<String>> {
    let mut names = Vec::new();
    for listed in Dir::from_fd(OwnedFd::from(root.folder.try_clone()?))?.iter() {
        let listed = listed?;
        let name = listed.file_name().to_string_lossy().into_owned();
        if name != "." && name != ".." {
            names.push(name);
        }
    }
    names.sort();
    Ok(names)
}

/// The most names in the root that are none of Atlas's listed at once.
const UNKNOWN: usize = 64;

/// Everything kept of one copy token: what is on the disk under its names,
/// whether a removal record names it, and whether work on it is running.
#[derive(Default)]
struct Group {
    placed: bool,
    staging: bool,
    removing: bool,
    record: bool,
}

/// The copies `root` keeps, by token: the one `copy` names, or a page of
/// them in token order after `after`. Each has its state, identity,
/// evidence, removal record and what runs from it; work registered on a
/// token, or a removal record left, shows its token even with no folder.
/// Names that are none of Atlas's are listed apart, on the first page.
pub(crate) fn inventory(
    root: &Root,
    stages: &Stages,
    copy: Option<&Token>,
    after: Option<&Token>,
) -> Value {
    let Ok(all) = names(root) else {
        return failure(Code::Io, "The desktop's copies could not be listed");
    };
    let mut groups = std::collections::BTreeMap::<String, (Token, Group)>::new();
    let mut unknown = Vec::new();
    for name in all {
        if let Some(token) = name
            .strip_prefix(".removing-")
            .and_then(|rest| rest.strip_suffix(".json"))
            .and_then(Token::parse)
        {
            group(&mut groups, token).record = true;
        } else if let Some(token) = name.strip_prefix(".removing-").and_then(Token::parse) {
            group(&mut groups, token).removing = true;
        } else if let Some(token) = name.strip_prefix(".staging-").and_then(Token::parse) {
            group(&mut groups, token).staging = true;
        } else if let Some(token) = Token::parse(&name) {
            group(&mut groups, token).placed = true;
        } else if !name.ends_with(".json.new") && unknown.len() < UNKNOWN {
            unknown.push(name);
        }
    }
    for token in stages.busy() {
        group(&mut groups, token);
    }
    let running = Running::now();
    let chosen: Vec<_> = groups
        .into_values()
        .filter(|(token, _)| copy.is_none_or(|copy| token == copy))
        .filter(|(token, _)| after.is_none_or(|after| token.as_str() > after.as_str()))
        .collect();
    let more = chosen.len() > PAGE;
    let listed: Vec<Value> = chosen
        .iter()
        .take(PAGE)
        .map(|(token, group)| listing(root, stages, &running, token, group))
        .collect();
    let next = more
        .then(|| chosen.get(PAGE - 1).map(|(token, _)| token.clone()))
        .flatten();
    let first = copy.is_none() && after.is_none();
    json!({
        "ok": true,
        "op": "copies",
        "copies": listed,
        "next": next,
        "unknown": if first { unknown } else { Vec::new() },
    })
}

/// The group of `token` in `groups`, made when it is the first of it.
fn group(
    groups: &mut std::collections::BTreeMap<String, (Token, Group)>,
    token: Token,
) -> &mut Group {
    &mut groups
        .entry(token.as_str().to_owned())
        .or_insert_with(|| (token, Group::default()))
        .1
}

/// One token's listing.
fn listing(root: &Root, stages: &Stages, running: &Running, token: &Token, group: &Group) -> Value {
    let busy = stages.running(token).is_some() || stages.removing(token);
    let state = if busy || (group.record && !group.removing) {
        "pending"
    } else if group.removing {
        "removing"
    } else if group.staging {
        "staging"
    } else if group.placed {
        "placed"
    } else {
        "pending"
    };
    let name = match state {
        "removing" => removing(token),
        "staging" => staging(token),
        _ => placed(token),
    };
    let folder = root.path.join(name.to_string_lossy().as_ref());
    let there = group.placed || group.staging || group.removing;
    let mut entry = json!({
        "copy": token,
        "state": state,
        "node": there.then(|| node_of(root, &name).ok()).flatten(),
        "processes": if there {
            running.from(&folder).map_or(json!("unknown"), |pids| json!(pids))
        } else {
            json!([])
        },
    });
    if group.placed
        && let Some(evidence) = super::evidence(root, token)
    {
        let path = root
            .path
            .join(token.as_str())
            .join(evidence.request["name"].as_str().unwrap_or_default());
        entry["path"] = json!(path);
        entry["evidence"] = json!({
            "change": evidence.change,
            "digest": evidence.request["digest"],
        });
    }
    if group.record {
        entry["removal"] =
            record_of(root, token).map_or(json!("unreadable"), |record| json!(record));
    }
    entry
}

#[cfg(test)]
#[path = "copies_tests.rs"]
mod tests;

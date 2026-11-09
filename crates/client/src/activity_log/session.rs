//! One session's own rows, from every file the log keeps, newest file
//! first, for the viewer's Activity once a session is chosen. Kept files
//! never change, so what each held for a session is kept by its revision,
//! and an unchanged log is read again with a look at each file and nothing
//! more.
use super::{
    FILE, PREVIOUS, ROTATE_AT, Row, ordered,
    retained::{self, KEEP_BYTES, Revision},
    selected,
};
use crate::host_session::{record::Record, store::ENVIRONMENT_VERB};
use atlas_core::host::Key;
use memchr::memmem;
use serde_json::Value;
use std::{
    collections::HashSet,
    fs::File,
    io,
    path::{Path, PathBuf},
    sync::Mutex,
};

/// The most sessions whose reads are kept, so going back to one read
/// lately reads nothing again.
const SESSIONS: usize = 4;
/// The verbs a record an older build wrote counts for work that may need no
/// helper command, and so leave no row to clear: environment work, and a
/// cleanup that found nothing to do.
const ROWLESS: [&str; 2] = [ENVIRONMENT_VERB, "cleanup"];
/// How many times a read starts, when the log keeps starting over.
const TRIES: usize = 3;
/// The most of the log's files one read goes through, from the newest.
const READ_LIMIT: u64 = KEEP_BYTES + 4 * ROTATE_AT;

// Atlas reads one configured output folder, and the person looks at one
// session at a time. Holding the lock through a read also coalesces
// concurrent cold reads.
static CACHE: Mutex<Cache> = Mutex::new(Cache { kept: Vec::new() });

/// Whose rows a read finds: a recorded host session's, or the agents'
/// commands no host named. Never the person's own input.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Whose {
    Host(Key),
    Terminal,
}

impl Whose {
    fn wrote(&self, row: &Row) -> bool {
        row.actor() != "viewer"
            && match (self, row.session.as_deref()) {
                (Self::Host(key), session) => session == Some(key.as_str()),
                (Self::Terminal, session) => session.is_none(),
            }
    }
}

/// How far back the log reaches, as a read found it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reach {
    /// The read went through every file, and the oldest row they keep was
    /// recorded then, in milliseconds since 1970.
    Since(u64),
    /// The read went through every file, and they keep no row.
    Nothing,
    /// The read stopped sooner, or the oldest row's time does not read.
    Unknown,
}

/// What a read of one session's rows found.
#[derive(Clone, Debug, PartialEq)]
pub struct SessionRows {
    /// One row per command, newest first, as [`super::recent`] orders them.
    pub rows: Vec<Row>,
    pub reach: Reach,
}

impl SessionRows {
    /// Whether `record`'s rows were all cleared to keep the log within its
    /// bounds: nothing of it is kept, and the log, which holds rows, now
    /// starts after the newest row its record says it wrote. A session that
    /// wrote none had nothing to clear, and a log with no rows at all was
    /// never filled, so it cleared nothing.
    ///
    /// A record an older build made does not say when its newest row was
    /// written. Its counts then say whether it did something the log keeps
    /// a row for, and its last command stands for the newest row: later
    /// than any of its rows, so it may say nothing was cleared when its
    /// rows are gone, never the other way round, but for a command counted
    /// that wrote no row.
    pub fn cleared(&self, record: &Record) -> bool {
        let newest = if record.logs_rows {
            record.logged
        } else {
            record
                .counts
                .iter()
                .any(|(verb, count)| !ROWLESS.contains(&verb.as_str()) && *count > 0)
                .then_some(record.last_seen)
        };
        self.rows.is_empty()
            && newest.is_some_and(|newest| match self.reach {
                Reach::Since(oldest) => oldest > newest,
                Reach::Nothing | Reach::Unknown => false,
            })
    }
}

/// What one file held for one session.
struct Scan {
    /// The name it was read by.
    path: PathBuf,
    revision: Revision,
    /// Its rows of that session, in reverse append order, each command's
    /// last appended step only.
    rows: Vec<Row>,
    /// When its first row was recorded, in nanoseconds since 1970.
    first_at: Option<u128>,
    /// Whether it holds any finished line.
    empty: bool,
}

/// The history folder as a read listed it. Its entries change only as
/// files are kept or pruned, which changes the folder itself, and a kept
/// file is never written again: so while the folder is unchanged, so is
/// every file it lists.
struct Listing {
    folder: Revision,
    history: retained::History,
}

/// One session's last read.
struct Kept {
    directory: PathBuf,
    whose: Whose,
    scans: Vec<Scan>,
    listing: Option<Listing>,
    /// The last answer, with the limit it was for, the files it came from,
    /// in order, and whether it went through every file.
    answer: Option<(usize, Vec<Revision>, bool, SessionRows)>,
}

/// The reads of the last few sessions, newest first.
struct Cache {
    kept: Vec<Kept>,
}

impl Cache {
    fn rows(&mut self, directory: &Path, whose: &Whose, limit: usize) -> io::Result<SessionRows> {
        let index = self
            .kept
            .iter()
            .position(|kept| kept.directory == directory && kept.whose == *whose);
        let mut kept = match index {
            Some(index) => self.kept.remove(index),
            None => Kept {
                directory: directory.to_owned(),
                whose: whose.clone(),
                scans: Vec::new(),
                listing: None,
                answer: None,
            },
        };
        let read = kept.read(limit);
        self.kept.insert(0, kept);
        self.kept.truncate(SESSIONS);
        read
    }
}

/// `whose` rows from the log in `directory`, newest first, at most `limit`:
/// each command's last appended step, ordered by when it was recorded, as
/// [`super::recent`] orders every session's.
///
/// Files are read newest first, each once, within [`READ_LIMIT`] bytes, and
/// the read stops after the file that brings the rows to `limit`. So a row
/// in an older file is not listed before a newer file's, even if a clock
/// that moved backwards recorded it later.
///
/// # Errors
///
/// Returns the I/O error of a file or the history that could not be read,
/// or a file that is not a plain one; a missing file holds nothing.
pub fn session_rows(directory: &Path, whose: &Whose, limit: usize) -> io::Result<SessionRows> {
    CACHE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .rows(directory, whose, limit)
}

impl Kept {
    /// Read the session's rows again, and once more, up to [`TRIES`] in
    /// all, when the log started over meanwhile: the current or previous
    /// file read then no longer goes by its name, and a file opened after
    /// it may be newer than it, which would put a command's steps out of
    /// order. Kept files never move, so only those two names are checked.
    ///
    /// # Errors
    ///
    /// Returns an interrupted read when the log started over during every
    /// try, besides the errors of reading it.
    fn read(&mut self, limit: usize) -> io::Result<SessionRows> {
        let names = [self.directory.join(FILE), self.directory.join(PREVIOUS)];
        for _ in 0..TRIES {
            let answer = self.read_once(limit)?;
            let mut moved = false;
            for scan in self.scans.iter().filter(|scan| names.contains(&scan.path)) {
                moved |= match std::fs::symlink_metadata(&scan.path) {
                    Ok(metadata) => !Revision::from(&metadata).same_file(&scan.revision),
                    Err(error) if error.kind() == io::ErrorKind::NotFound => true,
                    Err(error) => return Err(error),
                };
            }
            if !moved {
                return Ok(answer);
            }
        }
        Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "The activity log kept starting over while it was read",
        ))
    }

    /// Read the session's rows once, reusing the scans of files that have
    /// not changed, and keeping those of the files this read went through.
    fn read_once(&mut self, limit: usize) -> io::Result<SessionRows> {
        let mut earlier = std::mem::take(&mut self.scans);
        let mut room = READ_LIMIT;
        let mut commands = HashSet::new();
        let mut found = 0_usize;
        let mut whole = true;
        // Whether the history's files are known unchanged since they were
        // last read, so each is not opened again.
        let mut unchanged = false;
        let mut next = 0;
        loop {
            let path = match next {
                0 => self.directory.join(FILE),
                1 => self.directory.join(PREVIOUS),
                _ => {
                    if next == 2 {
                        unchanged = self.list_history()?;
                    }
                    let Some(history) = self.listing.as_ref().map(|listing| &listing.history)
                    else {
                        break;
                    };
                    match history.files.get(next - 2) {
                        Some(path) => path.clone(),
                        None => {
                            whole &= history.whole;
                            break;
                        }
                    }
                }
            };
            next += 1;
            let kept = unchanged
                .then(|| earlier.iter().position(|scan| scan.path == path))
                .flatten();
            let scan = match kept {
                Some(index) => earlier.swap_remove(index),
                None => {
                    let Some(file) = atlas_core::files::try_open_plain(&path)? else {
                        continue;
                    };
                    let revision = Revision::of(&file)?;
                    if self
                        .scans
                        .iter()
                        .any(|kept| kept.revision.same_file(&revision))
                    {
                        continue;
                    }
                    match earlier.iter().position(|scan| scan.revision == revision) {
                        Some(index) => {
                            let mut scan = earlier.swap_remove(index);
                            scan.path = path;
                            scan
                        }
                        None if revision.length > room => {
                            whole = false;
                            break;
                        }
                        None => scan(file, path, revision, &self.whose)?,
                    }
                }
            };
            // A file kept under two names, as while the log starts over, is
            // read once.
            if self
                .scans
                .iter()
                .any(|kept| kept.revision.same_file(&scan.revision))
            {
                continue;
            }
            if scan.revision.length > room {
                whole = false;
                break;
            }
            room -= scan.revision.length;
            for row in &scan.rows {
                let new = row
                    .command
                    .as_ref()
                    .is_none_or(|command| commands.insert(command.clone()));
                found += usize::from(new);
            }
            self.scans.push(scan);
            if found >= limit {
                whole = false;
                break;
            }
        }

        let revisions: Vec<Revision> = self
            .scans
            .iter()
            .map(|scan| scan.revision.clone())
            .collect();
        if let Some((_, _, _, answer)) = self
            .answer
            .as_ref()
            .filter(|(kept, read, went, _)| *kept == limit && *read == revisions && *went == whole)
        {
            return Ok(answer.clone());
        }
        let reach = if whole {
            match self.scans.iter().rev().find(|scan| !scan.empty) {
                None => Reach::Nothing,
                Some(oldest) => oldest
                    .first_at
                    .and_then(|at| u64::try_from(at / 1_000_000).ok())
                    .map_or(Reach::Unknown, Reach::Since),
            }
        } else {
            Reach::Unknown
        };
        let mut rows: Vec<Row> = self
            .scans
            .iter()
            .flat_map(|scan| scan.rows.iter().cloned())
            .collect();
        ordered(&mut rows);
        let answer = SessionRows {
            rows: selected(&rows, limit),
            reach,
        };
        self.answer = Some((limit, revisions, whole, answer.clone()));
        Ok(answer)
    }

    /// List the history folder again unless it is unchanged since it was
    /// last listed; whether it was. A missing folder lists nothing.
    fn list_history(&mut self) -> io::Result<bool> {
        let folder = match std::fs::symlink_metadata(self.directory.join(retained::HISTORY)) {
            Ok(metadata) => Revision::from(&metadata),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                self.listing = None;
                return Ok(false);
            }
            Err(error) => return Err(error),
        };
        if self
            .listing
            .as_ref()
            .is_some_and(|listing| listing.folder == folder)
        {
            return Ok(true);
        }
        // Listed after its revision was taken, so a change meanwhile reads
        // as a change next time.
        self.listing = None;
        let history = retained::history(&self.directory)?;
        self.listing = Some(Listing { folder, history });
        Ok(false)
    }
}

/// What `file`, of `revision`, holds for `whose`.
///
/// For a host session, only a line holding its key in quotes is parsed:
/// keys are hex digits, so a row naming the session always holds them so.
/// For Terminal, a line naming a session as text is passed over: Atlas
/// writes a row's session only as its key, and rows are flat. Every parsed
/// row is then checked whole, and an unfinished last line is passed over,
/// as no writer acknowledged it.
fn scan(file: File, path: PathBuf, revision: Revision, whose: &Whose) -> io::Result<Scan> {
    #[cfg(test)]
    tests::scanned(&path);
    let bytes = atlas_core::files::read_up_to(file, revision.length)?;
    let finished = memchr::memrchr(b'\n', &bytes).map_or(0, |newline| newline + 1);
    let bytes = &bytes[..finished];
    let first_at = bytes
        .split_inclusive(|byte| *byte == b'\n')
        .find_map(parse)
        .and_then(|row| row.at_nanos());

    let mut rows = Vec::new();
    let mut keep = |line: &[u8]| {
        if let Some(row) = parse(line).filter(|row| whose.wrote(row)) {
            rows.push(row);
        }
    };
    match whose {
        Whose::Host(key) => {
            let needle = format!("\"{key}\"");
            let mut after = 0;
            for found in memmem::find_iter(bytes, needle.as_bytes()) {
                if found < after {
                    continue;
                }
                let start =
                    memchr::memrchr(b'\n', &bytes[..found]).map_or(0, |newline| newline + 1);
                let end = memchr::memchr(b'\n', &bytes[found..])
                    .map_or(bytes.len(), |newline| found + newline + 1);
                keep(&bytes[start..end]);
                after = end;
            }
        }
        Whose::Terminal => {
            let named = memmem::Finder::new(b"\"session\":\"");
            for line in bytes.split_inclusive(|byte| *byte == b'\n') {
                if named.find(line).is_none() {
                    keep(line);
                }
            }
        }
    }
    // Each command's last appended step stands for it, in this file as in
    // the whole log, so a long command's many steps take no more room.
    rows.reverse();
    let mut commands = HashSet::new();
    rows.retain(|row| {
        row.command
            .as_ref()
            .is_none_or(|command| commands.insert(command.clone()))
    });
    Ok(Scan {
        path,
        revision,
        rows,
        first_at,
        empty: bytes.is_empty(),
    })
}

/// A whole line of the log as a row, when it is one.
fn parse(line: &[u8]) -> Option<Row> {
    let line = line.strip_suffix(b"\n")?;
    serde_json::from_slice::<Value>(line)
        .ok()
        .and_then(|value| Row::parse(&value))
}

#[cfg(test)]
mod tests;

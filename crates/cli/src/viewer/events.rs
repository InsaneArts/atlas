//! The service's feed of invalidations: what changed, never what it is.
//! Each event names a desktop and a kind of thing to read again (activity,
//! sessions, questions, status, control, the desktops), numbered in one
//! feed. Readers ask for what came after the last number they saw and read
//! the details through the routes they may already use, so the feed tells
//! nobody anything they could not read.
//!
//! The feed is kept on disk in two segments at most, each written and
//! synced before its event is told to anyone. A torn last record is cut off
//! when the feed opens; anything else it cannot prove continuous starts a
//! new feed, which every reader takes as "read everything again".
use atlas_core::{
    error::{Code, Error, Result},
    files::token,
    time::now_ms,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::VecDeque,
    fs::{self, File, OpenOptions},
    io::{BufRead, BufReader, Write},
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
};

/// The file naming the feed, in its folder.
const IDENTITY: &str = "feed";
/// How many events one segment holds before the next begins; fewer in
/// tests, which sync each one, though two still hold more than a batch.
#[cfg(not(test))]
const SEGMENT: u64 = 2048;
#[cfg(test)]
const SEGMENT: u64 = 160;
/// The most events one answer gives.
pub(super) const BATCH: usize = 256;
/// The longest line a segment may hold; one longer is not a record.
const LINE: usize = 4096;

/// What an event says to read again.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum Kind {
    Activity,
    Sessions,
    Asks,
    Status,
    Control,
    Desktops,
}

/// One invalidation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Event {
    pub(super) feed: String,
    pub(super) seq: u64,
    /// When it was written, in milliseconds since 1970.
    pub(super) at: u64,
    /// The desktop it is for, by its ID.
    pub(super) desktop: String,
    pub(super) kind: Kind,
}

/// What a reader asking for the events after a number is told.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Since {
    /// The events after it, oldest first, at most [`BATCH`], in the feed
    /// named, whose newest is `head`.
    Events {
        feed: String,
        events: Vec<Event>,
        head: u64,
    },
    /// Its number does not follow on: another feed, one older than the
    /// oldest kept, or one past the head. The reader takes this position,
    /// reads everything again, then asks from it.
    Reset { feed: String, head: u64 },
}

/// The feed, open for writing by the one service.
pub(super) struct EventLog {
    folder: PathBuf,
    feed: String,
    head: u64,
    /// The events kept in memory, as the segments on disk hold them.
    kept: VecDeque<Event>,
    /// The segment written to now, and how many events it holds.
    active: Option<(File, u64)>,
}

impl EventLog {
    /// Open the feed kept in `folder`, made private (0700) when missing:
    /// its last whole records continue it, a torn last record is cut off,
    /// and a feed it cannot prove continuous is replaced by a new one.
    ///
    /// # Errors
    ///
    /// Returns the failure to make the folder or write a new feed.
    pub(super) fn open(folder: &Path) -> Result<Self> {
        private_folder(folder)?;
        if let Some(log) = Self::continued(folder) {
            return Ok(log);
        }
        Self::begin(folder)
    }

    /// The feed in `folder` as its segments hold it, when they hold one
    /// continuous run of whole records.
    fn continued(folder: &Path) -> Option<Self> {
        let feed = fs::read_to_string(folder.join(IDENTITY)).ok()?;
        let feed = feed.trim().to_owned();
        if feed.is_empty() {
            return None;
        }
        let segments = segments(folder).ok()?;
        let mut kept = VecDeque::new();
        let last = segments.len().checked_sub(1);
        for (index, (first, path)) in segments.iter().enumerate() {
            let records = read_segment(path, Some(index) == last)?;
            if records.first().is_some_and(|event| event.seq != *first) {
                return None;
            }
            kept.extend(records);
        }
        let continuous = kept
            .iter()
            .zip(kept.iter().skip(1))
            .all(|(before, after)| after.seq == before.seq + 1)
            && kept.iter().all(|event| event.feed == feed);
        if !continuous {
            return None;
        }
        let head = kept.back().map_or(0, |event| event.seq);
        let active = match segments.last() {
            Some((first, path)) => {
                let file = OpenOptions::new().append(true).open(path).ok()?;
                let count = kept.iter().filter(|event| event.seq >= *first).count();
                Some((file, u64::try_from(count).ok()?))
            }
            None => None,
        };
        Some(Self {
            folder: folder.to_owned(),
            feed,
            head,
            kept,
            active,
        })
    }

    /// A new, empty feed in `folder`, replacing whatever was there.
    ///
    /// # Errors
    ///
    /// Returns the failure to remove the old segments or write the identity.
    pub(super) fn begin(folder: &Path) -> Result<Self> {
        private_folder(folder)?;
        for (_, path) in segments(folder).unwrap_or_default() {
            fs::remove_file(&path).map_err(|error| Error::writing(&path, error))?;
        }
        let feed = token();
        atlas_core::files::write_private(&folder.join(IDENTITY), feed.as_bytes())?;
        Ok(Self {
            folder: folder.to_owned(),
            feed,
            head: 0,
            kept: VecDeque::new(),
            active: None,
        })
    }

    /// The feed's identity and the number of its newest event.
    pub(super) fn position(&self) -> (&str, u64) {
        (&self.feed, self.head)
    }

    /// Write that `kind` of `desktop` changed, synced to disk before it is
    /// counted: the event.
    ///
    /// # Errors
    ///
    /// Returns the failure to write or sync it. What was written of it is
    /// taken back; when even that fails, the feed is begun again before the
    /// next event, so no reader ever reads past a hole.
    pub(super) fn append(&mut self, desktop: &str, kind: Kind) -> Result<Event> {
        let event = Event {
            feed: self.feed.clone(),
            seq: self.head + 1,
            at: now_ms(),
            desktop: desktop.to_owned(),
            kind,
        };
        let mut line = serde_json::to_vec(&event)?;
        line.push(b'\n');
        let (file, count) = self.segment_for(event.seq)?;
        let before = file.metadata()?.len();
        let written = file.write_all(&line).and_then(|()| file.sync_data());
        if let Err(error) = written {
            let taken_back = file.set_len(before).and_then(|()| file.sync_data());
            if taken_back.is_err() {
                // Begun again: every reader reads everything again.
                *self = Self::begin(&self.folder)?;
            }
            return Err(
                Error::new(Code::Io, "Atlas could not keep what changed").with_source(error)
            );
        }
        *count += 1;
        self.head = event.seq;
        self.kept.push_back(event.clone());
        Ok(event)
    }

    /// The segment the event numbered `seq` goes in: the active one, or a
    /// new one once it is full.
    fn segment_for(&mut self, seq: u64) -> Result<&mut (File, u64)> {
        let full = self
            .active
            .as_ref()
            .is_none_or(|(_, count)| *count >= SEGMENT);
        if full {
            let path = self.folder.join(format!("events-{seq:020}.jsonl"));
            let file = OpenOptions::new()
                .append(true)
                .create(true)
                .mode(0o600)
                .open(&path)
                .map_err(|error| Error::writing(&path, error))?;
            self.active = Some((file, 0));
            self.keep_two_segments()?;
        }
        Ok(self.active.as_mut().expect("a segment was just opened"))
    }

    /// Keep the two newest segments on disk, and in memory what they hold:
    /// a reader is told the same before the service starts again as after.
    fn keep_two_segments(&mut self) -> Result<()> {
        let segments = segments(&self.folder)?;
        let surplus = segments.len().saturating_sub(2);
        for (_, path) in &segments[..surplus] {
            fs::remove_file(path).map_err(|error| Error::writing(path, error))?;
        }
        if let Some((oldest, _)) = segments.get(surplus) {
            while self.kept.front().is_some_and(|event| event.seq < *oldest) {
                self.kept.pop_front();
            }
        }
        Ok(())
    }

    /// What a reader who last saw `after` in feed `feed` is told.
    pub(super) fn since(&self, feed: &str, after: u64) -> Since {
        let oldest = self.kept.front().map_or(self.head + 1, |event| event.seq);
        let follows = feed == self.feed && after <= self.head && after + 1 >= oldest;
        if !follows {
            return Since::Reset {
                feed: self.feed.clone(),
                head: self.head,
            };
        }
        let events = self
            .kept
            .iter()
            .filter(|event| event.seq > after)
            .take(BATCH)
            .cloned()
            .collect();
        Since::Events {
            feed: self.feed.clone(),
            events,
            head: self.head,
        }
    }
}

/// Make `folder` private when it is missing, and refuse one that is not:
/// another account's, open to others, or a link.
fn private_folder(folder: &Path) -> Result<()> {
    atlas_core::files::private_folder(folder, Code::Io)
}

/// The segments in `folder`, by the number of their first event, oldest
/// first.
fn segments(folder: &Path) -> Result<Vec<(u64, PathBuf)>> {
    let mut found = Vec::new();
    for entry in fs::read_dir(folder).map_err(|error| Error::writing(folder, error))? {
        let entry = entry?;
        let name = entry.file_name();
        let Some(first) = name
            .to_str()
            .and_then(|name| name.strip_prefix("events-"))
            .and_then(|name| name.strip_suffix(".jsonl"))
            .and_then(|number| number.parse::<u64>().ok())
        else {
            continue;
        };
        found.push((first, entry.path()));
    }
    found.sort_by_key(|(first, _)| *first);
    Ok(found)
}

/// The whole records of the segment at `path`. In the `last` segment a torn
/// final record, one its write never ended with a newline, is cut off; any
/// other record that does not read makes the feed unprovable, as cutting
/// it off could number a later event as one already told.
fn read_segment(path: &Path, last: bool) -> Option<Vec<Event>> {
    let file = File::open(path).ok()?;
    let mut reader = BufReader::new(file);
    let mut records = Vec::new();
    let mut whole = 0_u64;
    let mut line = Vec::new();
    loop {
        line.clear();
        // Never more than one record's worth: a longer line is damage.
        let read = std::io::Read::take(&mut reader, LINE as u64 + 1)
            .read_until(b'\n', &mut line)
            .ok()?;
        if read == 0 {
            return Some(records);
        }
        let ended = line.ends_with(b"\n");
        match (ended && line.len() <= LINE)
            .then(|| serde_json::from_slice::<Event>(&line).ok())
            .flatten()
        {
            Some(event) => {
                records.push(event);
                whole += read as u64;
            }
            // Torn while it was written: cut back to the whole records. Only
            // the very last record can be; one followed by more is damage.
            None if !ended && last && reader.fill_buf().ok()?.is_empty() => {
                let file = OpenOptions::new().write(true).open(path).ok()?;
                file.set_len(whole).ok()?;
                file.sync_data().ok()?;
                return Some(records);
            }
            None => return None,
        }
    }
}

#[cfg(test)]
#[path = "events_tests.rs"]
mod tests;

//! What every surface calls a host session: its host and project folder,
//! "Claude Code · left", with its start time only when another session
//! shares both. Names are worked out when read, so one may gain its time
//! later. No host title, transcript or host store is ever read.
use super::{Host, HostId, HostSession, Key, record::Record};
use atlas_control::Note;
use std::collections::BTreeMap;

/// What a session is named by: a record, or the note of a session on its
/// first command, which has no record yet.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Naming<'a> {
    pub(crate) key: &'a Key,
    pub(crate) host: Host,
    /// A named session's own label, which names it.
    pub(crate) named: Option<&'a str>,
    /// The project folder's name.
    pub(crate) folder: Option<&'a str>,
    /// When it started, in milliseconds since 1970.
    pub(crate) started: u64,
}

impl<'a> From<&'a Record> for Naming<'a> {
    fn from(record: &'a Record) -> Self {
        Self {
            key: &record.key,
            host: record.host,
            named: (record.host == Host::Named).then(|| record.host_session.as_str()),
            folder: record.folder.as_deref(),
            started: record.started,
        }
    }
}

/// How many of a key's characters tell apart sessions started in the same
/// second.
const KEY_SHOWN: usize = 4;
/// What separates a name's parts.
const DOT: &str = " · ";

impl<'a> Naming<'a> {
    pub(crate) fn of_session(session: &'a HostSession) -> Self {
        Self {
            key: session.key(),
            host: session.host(),
            named: (session.host() == Host::Named).then(|| session.id().as_str()),
            folder: session.folder(),
            started: session.began().unwrap_or_else(atlas_core::time::now_ms),
        }
    }

    /// What a session on its first command is named by, from its `note`:
    /// its host and folder as that command resolved them, and when that
    /// command started, as its record will say once written.
    pub(crate) fn of_note(note: &'a Note) -> Option<Self> {
        Some(Self {
            key: note.session.as_ref()?,
            host: note.host?,
            named: note.named.as_ref().map(HostId::as_str),
            folder: note.folder.as_deref(),
            started: note.process.started_ms(),
        })
    }
}

/// The name of each session `records` name and of each on its first
/// command, which only a live note of `notes` names yet: one rule for both,
/// so a name does not change once its record is written.
pub(crate) fn names_with<'a>(
    records: impl IntoIterator<Item = &'a Record>,
    notes: impl IntoIterator<Item = &'a Note>,
) -> BTreeMap<Key, String> {
    names_with_left(records, notes, [])
}

/// [`names_with`], also naming each session only a note of `left` names,
/// live or not: the note a command killed on its session's first command
/// left with its unknown outcome, so that session is named as its record
/// would have named it.
pub(crate) fn names_with_left<'a>(
    records: impl IntoIterator<Item = &'a Record>,
    notes: impl IntoIterator<Item = &'a Note>,
    left: impl IntoIterator<Item = &'a Note>,
) -> BTreeMap<Key, String> {
    let mut sessions: Vec<Naming<'a>> = records.into_iter().map(Naming::from).collect();
    let live = notes.into_iter().filter(|note| note.live());
    for first in live.chain(left).filter_map(Naming::of_note) {
        if sessions.iter().all(|known| known.key != first.key) {
            sessions.push(first);
        }
    }
    named(sessions)
}

/// Each record's name as every surface shows it: "Claude Code · left",
/// with its start time when another record shares host and folder.
pub fn names<'a>(records: impl IntoIterator<Item = &'a Record>) -> BTreeMap<Key, String> {
    named(records.into_iter().map(Naming::from))
}

/// [`names`] of each session `sessions` name, records or not.
pub(crate) fn named<'a>(sessions: impl IntoIterator<Item = Naming<'a>>) -> BTreeMap<Key, String> {
    let mut groups: BTreeMap<String, Vec<Naming<'a>>> = BTreeMap::new();
    for session in sessions {
        groups.entry(base(&session)).or_default().push(session);
    }

    let mut named = BTreeMap::new();
    for (base, group) in groups {
        if group.len() == 1 {
            named.insert(group[0].key.clone(), base);
            continue;
        }

        let minutes: Vec<Option<String>> = group
            .iter()
            .map(|session| minute(session.started))
            .collect();
        for (session, at) in group.iter().zip(&minutes) {
            let shared = minutes.iter().filter(|other| *other == at).count() > 1;
            let name = match at {
                Some(at) if !shared => format!("{base}{DOT}{at}"),
                Some(at) => {
                    let seconds = second(session.started);
                    let alike = group
                        .iter()
                        .filter(|other| second(other.started) == seconds)
                        .count();
                    match seconds {
                        Some(seconds) if alike == 1 => format!("{base}{DOT}{seconds}"),
                        _ => format!("{base}{DOT}{at}{DOT}{}", short(session.key)),
                    }
                }
                None => format!("{base}{DOT}{}", short(session.key)),
            };
            named.insert(session.key.clone(), name);
        }
    }

    named
}

/// The name of `key` among `names`, else its host as people know it, else
/// the key itself.
pub fn name_of(names: &BTreeMap<Key, String>, key: &Key, host: Option<Host>) -> String {
    names
        .get(key)
        .cloned()
        .or_else(|| {
            host.map(Host::label)
                .filter(|label| !label.is_empty())
                .map(str::to_owned)
        })
        .unwrap_or_else(|| key.to_string())
}

/// The local time of day `ms` since 1970 falls at, as "14:02".
pub fn minute(ms: u64) -> Option<String> {
    clock(ms).map(|(hour, minute, _)| format!("{hour:02}:{minute:02}"))
}

/// A session's name before any time is added: its host, or its own label
/// for a named session (its key when that is not known), and its folder
/// when it has one.
fn base(session: &Naming<'_>) -> String {
    let host = match (session.host, session.named) {
        // An ATLAS_SESSION value is cut as an app's name is.
        (Host::Named, Some(named)) => atlas_core::name::bounded(named),
        (Host::Named, None) => session.key.to_string(),
        (host, _) => host.label().to_owned(),
    };

    match session.folder {
        Some(folder) => format!("{host}{DOT}{folder}"),
        None => host,
    }
}

/// [`minute`] with its seconds, as "14:02:31".
fn second(ms: u64) -> Option<String> {
    clock(ms).map(|(hour, minute, second)| format!("{hour:02}:{minute:02}:{second:02}"))
}

/// The local hour, minute and second of `ms` since 1970.
fn clock(ms: u64) -> Option<(u8, u8, u8)> {
    atlas_core::time::local_clock(i64::try_from(ms / 1000).ok()?)
}

/// The first characters of `key`.
fn short(key: &Key) -> &str {
    &key.as_str()[..KEY_SHOWN]
}

#[cfg(test)]
#[path = "names_tests.rs"]
mod tests;

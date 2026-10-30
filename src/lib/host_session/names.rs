//! What every surface calls a host session: its host and project folder,
//! "Claude Code · left", with its start time only when another session
//! shares both. Names are worked out when read, so one may gain its time
//! later. No host title, transcript or host store is ever read.
use super::{Host, HostId, Key, record::Record};
use crate::control::Note;
use std::collections::BTreeMap;

/// What a session is named by: a record, or the note of a session on its
/// first command, which has no record yet.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Naming<'a> {
    pub key: &'a Key,
    pub host: Host,
    /// A named session's own label, which names it.
    pub named: Option<&'a str>,
    /// The project folder's name.
    pub folder: Option<&'a str>,
    /// When it started, in milliseconds since 1970.
    pub started: u64,
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
pub(crate) fn names<'a>(records: impl IntoIterator<Item = &'a Record>) -> BTreeMap<Key, String> {
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
pub(crate) fn name_of(names: &BTreeMap<Key, String>, key: &Key, host: Option<Host>) -> String {
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
pub(crate) fn minute(ms: u64) -> Option<String> {
    clock(ms).map(|(hour, minute, _)| format!("{hour:02}:{minute:02}"))
}

/// A session's name before any time is added: its host, or its own label
/// for a named session (its key when that is not known), and its folder
/// when it has one.
fn base(session: &Naming<'_>) -> String {
    let host = match (session.host, session.named) {
        // An ATLAS_SESSION value is cut as an app's name is.
        (Host::Named, Some(named)) => crate::doing::bounded(named),
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
    crate::time::local_clock(i64::try_from(ms / 1000).ok()?)
}

/// The first characters of `key`.
fn short(key: &Key) -> &str {
    &key.as_str()[..KEY_SHOWN]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host_session::HostSession;

    /// A record of `id` of `host` in `folder`, started at `started`.
    fn record(host: Host, id: &str, folder: Option<&str>, started: u64) -> Record {
        let session = HostSession::new(host, id).unwrap();
        let mut record = Record::new(&session, started, folder.map(str::to_owned));
        record.last_seen = started;
        record
    }

    /// Milliseconds since 1970 of the local time `hour:minute:second` on
    /// the day `base` falls in.
    fn at(base: u64, hour: u64, minute: u64, second: u64) -> u64 {
        let (h, m, s) = clock(base).unwrap();
        let midnight = base / 1000 - u64::from(h) * 3600 - u64::from(m) * 60 - u64::from(s);
        (midnight + hour * 3600 + minute * 60 + second) * 1000
    }

    const DAY: u64 = 1_761_660_000_000;

    #[test]
    fn a_session_alone_in_its_folder_is_named_by_host_and_folder() {
        let records = [
            record(Host::Claude, "one", Some("left"), DAY),
            record(Host::Codex, "two", Some("left"), DAY),
        ];
        let names = names(&records);

        assert_eq!(names[&records[0].key], "Claude Code · left");
        assert_eq!(names[&records[1].key], "Codex · left");
    }

    #[test]
    fn two_sessions_of_one_host_in_one_folder_add_their_start_times() {
        let records = [
            record(Host::Claude, "one", Some("left"), at(DAY, 14, 2, 10)),
            record(Host::Claude, "two", Some("left"), at(DAY, 14, 30, 0)),
        ];
        let names = names(&records);
        assert_eq!(names[&records[0].key], "Claude Code · left · 14:02");
        assert_eq!(names[&records[1].key], "Claude Code · left · 14:30");

        let minute = [
            record(Host::Claude, "one", Some("left"), at(DAY, 14, 2, 10)),
            record(Host::Claude, "two", Some("left"), at(DAY, 14, 2, 31)),
        ];
        let names = super::names(&minute);
        assert_eq!(names[&minute[0].key], "Claude Code · left · 14:02:10");
        assert_eq!(names[&minute[1].key], "Claude Code · left · 14:02:31");

        let second = [
            record(Host::Claude, "one", Some("left"), at(DAY, 14, 2, 10)),
            record(Host::Claude, "two", Some("left"), at(DAY, 14, 2, 10) + 400),
        ];
        let names = super::names(&second);
        for record in &second {
            assert_eq!(
                names[&record.key],
                format!("Claude Code · left · 14:02 · {}", &record.key.as_str()[..4])
            );
        }
    }

    #[test]
    fn no_folder_names_the_host_alone_and_a_named_session_its_label() {
        let records = [
            record(Host::Pi, "pi-1", None, DAY),
            record(Host::Named, "release-check", Some("left"), DAY),
            record(Host::Named, &"n".repeat(60), None, DAY),
        ];
        let names = names(&records);

        assert_eq!(names[&records[0].key], "Pi");
        assert_eq!(names[&records[1].key], "release-check · left");
        assert_eq!(names[&records[2].key].chars().count(), 40);
        assert_eq!(
            name_of(&names, &Key::of(Host::Codex, "unknown"), Some(Host::Codex)),
            "Codex"
        );
        let unknown = Key::of(Host::Named, "unknown");
        assert_eq!(
            name_of(&names, &unknown, Some(Host::Named)),
            unknown.to_string()
        );
    }

    #[test]
    fn a_session_with_no_record_yet_is_named_by_the_same_rule() {
        let records = [record(
            Host::Claude,
            "one",
            Some("left"),
            at(DAY, 14, 2, 10),
        )];
        let (two, three) = (Key::of(Host::Claude, "two"), Key::of(Host::Named, "three"));
        let first_commands = [
            Naming {
                key: &two,
                host: Host::Claude,
                named: None,
                folder: Some("left"),
                started: at(DAY, 14, 30, 0),
            },
            Naming {
                key: &three,
                host: Host::Named,
                named: None,
                folder: None,
                started: DAY,
            },
        ];
        let names = named(records.iter().map(Naming::from).chain(first_commands));

        assert_eq!(names[&records[0].key], "Claude Code · left · 14:02");
        assert_eq!(names[&two], "Claude Code · left · 14:30");
        assert_eq!(names[&three], three.to_string(), "no label, so its key");
    }

    #[test]
    fn a_note_a_killed_command_left_names_its_session_only_where_asked() {
        let session = HostSession::new(Host::Named, "agent-a")
            .unwrap()
            .in_folder(Some("left"));
        let current = crate::host_session::Instance::current().unwrap();
        let note = Note {
            session: Some(session.key().clone()),
            host: Some(Host::Named),
            folder: Some("left".into()),
            named: HostId::parse("agent-a"),
            process: crate::host_session::Instance::new(i32::MAX, current.start),
            since: DAY,
        };

        assert!(
            !names_with([], [&note]).contains_key(session.key()),
            "a note whose command ended names nobody"
        );
        assert_eq!(
            names_with_left([], [], [&note])[session.key()],
            "agent-a · left",
            "as its record would have named it"
        );
        let recorded = [record(Host::Named, "agent-a", Some("right"), DAY)];
        assert_eq!(
            names_with_left(&recorded, [], [&note])[session.key()],
            "agent-a · right",
            "a record names it first"
        );
    }
}

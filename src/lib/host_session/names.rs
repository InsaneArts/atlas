//! What every surface calls a host session: its host and project folder,
//! "Claude Code · left", with its start time only when another recorded
//! session shares both. Names are worked out when read, so one may gain
//! its time later. No host title, transcript or host store is ever read.
use super::{Host, Key, record::Record};
use std::collections::BTreeMap;

/// How many of a key's characters tell apart sessions started in the same
/// second.
const KEY_SHOWN: usize = 4;
/// What separates a name's parts.
const DOT: &str = " · ";

/// Each record's name as every surface shows it: "Claude Code · left",
/// with its start time when another record shares host and folder.
pub(crate) fn names<'a>(records: impl IntoIterator<Item = &'a Record>) -> BTreeMap<Key, String> {
    let mut groups: BTreeMap<String, Vec<&Record>> = BTreeMap::new();
    for record in records {
        groups.entry(base(record)).or_default().push(record);
    }

    let mut named = BTreeMap::new();
    for (base, group) in groups {
        if group.len() == 1 {
            named.insert(group[0].key.clone(), base);
            continue;
        }

        let minutes: Vec<Option<String>> =
            group.iter().map(|record| minute(record.started)).collect();
        for (record, at) in group.iter().zip(&minutes) {
            let shared = minutes.iter().filter(|other| *other == at).count() > 1;
            let name = match at {
                Some(at) if !shared => format!("{base}{DOT}{at}"),
                Some(at) => {
                    let seconds = second(record.started);
                    let alike = group
                        .iter()
                        .filter(|other| second(other.started) == seconds)
                        .count();
                    match seconds {
                        Some(seconds) if alike == 1 => format!("{base}{DOT}{seconds}"),
                        _ => format!("{base}{DOT}{at}{DOT}{}", short(&record.key)),
                    }
                }
                None => format!("{base}{DOT}{}", short(&record.key)),
            };
            named.insert(record.key.clone(), name);
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

/// A record's name before any time is added: its host, or its own label
/// for a named session, and its folder when it has one.
fn base(record: &Record) -> String {
    let host = match record.host {
        // An ATLAS_SESSION value is cut as an app's name is.
        Host::Named => crate::doing::bounded(record.host_session.as_str()),
        host => host.label().to_owned(),
    };

    match &record.folder {
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
    super::sys::local_clock(i64::try_from(ms / 1000).ok()?)
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
}

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

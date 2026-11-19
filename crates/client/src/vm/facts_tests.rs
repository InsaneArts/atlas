use super::*;
use crate::vm::lifecycle::{
    Provenance, Reservation,
    record::{Maintenance, Phase},
};
use atlas_core::files::Token;
use std::path::PathBuf;

const WORK: &str = "0123456789abcdef01234567";

fn id() -> DesktopId {
    DesktopId::parse(WORK).unwrap()
}

/// The record of the VM `work`, its lifecycle at `state`.
fn record(state: State) -> Record {
    let reservation = Reservation {
        name: "work".to_owned(),
        id: id(),
        named: true,
        resumed: false,
    };
    let mut record = Record::adding(
        &reservation,
        PathBuf::from("/Users/someone/.config/atlas.toml"),
        Provenance {
            program: PathBuf::from("/Applications/Atlas.app/Contents/MacOS/atlas"),
            requirement: "identifier \"com.insanearts.atlas\"".to_owned(),
        },
    );
    record.state = state;
    record
}

/// What Atlas VM said: its capabilities, its state and the newest macOS.
fn word(declares: &[&str], state: Value, latest: Value) -> Word {
    let reply = |reply: Value| serde_json::from_value::<Reply>(reply).unwrap();
    let mut state = state;
    state["ok"] = json!(true);
    Word {
        hello: Some(Hello {
            version: 1,
            capabilities: declares.iter().map(|&name| name.to_owned()).collect(),
        }),
        state: Ok(reply(state)),
        latest: (!latest.is_null()).then(|| serde_json::from_value(latest).unwrap()),
    }
}

/// The facts of `work`, as `record` and `word` say, its guest reporting
/// `system`.
fn of(lifecycle: &Lifecycle, record: &Record, word: &Word, system: &Value) -> Value {
    facts(&Seen {
        lifecycle,
        vm: &id(),
        name: "work",
        record: Some(record),
        word,
        system,
        atlas: json!({"state": "current"}),
    })
}

#[test]
fn a_newer_macos_is_said_only_when_strictly_greater() {
    let home = tempfile::tempdir().unwrap();
    let lifecycle = Lifecycle::under(home.path());
    let created = record(State::Created);
    let newer = |guest: Value, latest: &str, declares: &[&str]| {
        let latest = json!({"version": latest, "build": "27D50"});
        let system = json!({"version": guest, "build": "27C101"});
        of(
            &lifecycle,
            &created,
            &word(declares, json!({}), latest),
            &system,
        )["macos"]["newer"]
            .clone()
    };
    let declared = &["latest-macos"][..];

    assert_eq!(newer(json!("27.2.1"), "27.3", declared), json!(true));
    assert_eq!(newer(json!("27.2.1"), "27.10", declared), json!(true));
    assert_eq!(newer(json!("27.2"), "27.2.0", declared), json!(false));
    assert_eq!(newer(json!("27.3"), "27.2.1", declared), json!(false));
    assert_eq!(newer(json!("27.3"), "27.3", declared), json!(false));
    assert_eq!(
        newer(Value::Null, "27.3", declared),
        Value::Null,
        "guest unknown"
    );
    assert_eq!(newer(json!("27.2"), "27.3 beta", declared), Value::Null);
    assert_eq!(
        newer(json!("27.2"), "27.3", &[]),
        Value::Null,
        "an Atlas VM that does not declare it"
    );
    let unknown = of(
        &lifecycle,
        &created,
        &word(declared, json!({}), Value::Null),
        &json!({"version": "27.2", "build": "27C101"}),
    );
    assert_eq!(
        unknown["macos"],
        json!({"version": "27.2", "build": "27C101", "latest": null, "newer": null}),
        "unknown is never up to date"
    );
}

#[test]
fn disk_use_is_atlas_vms_word_apart_from_its_size() {
    let home = tempfile::tempdir().unwrap();
    let lifecycle = Lifecycle::under(home.path());
    let created = record(State::Created);
    let disk = |declares: &[&str], disk: Value| {
        of(
            &lifecycle,
            &created,
            &word(declares, json!({"disk": disk}), Value::Null),
            &Value::Null,
        )["disk"]
            .clone()
    };

    assert_eq!(
        disk(
            &["disk"],
            json!({"used": 28_000_000_000_u64, "size": 128_000_000_000_u64})
        ),
        json!({"used": 28_000_000_000_u64, "size": 128_000_000_000_u64})
    );
    assert_eq!(
        disk(&["disk"], json!({"size": 128_000_000_000_u64})),
        json!({"used": null, "size": 128_000_000_000_u64}),
        "its size alone, never a use made of it"
    );
    assert_eq!(
        disk(
            &[],
            json!({"used": 28_000_000_000_u64, "size": 128_000_000_000_u64})
        ),
        Value::Null,
        "an Atlas VM that does not declare it"
    );
}

#[test]
fn what_the_person_can_do_follows_its_power_its_maintenance_and_its_removal() {
    let home = tempfile::tempdir().unwrap();
    let lifecycle = Lifecycle::under(home.path());
    let can = |record: &Record, power: &str, confirmed: bool| {
        let state = json!({"power": power, "confirmed": confirmed});
        of(
            &lifecycle,
            record,
            &word(&[], state, Value::Null),
            &Value::Null,
        )["can"]
            .clone()
    };
    let created = record(State::Created);
    let mut maintained = created.clone();
    maintained.maintenance = Some(Maintenance {
        change: Token::new(),
        phase: Phase::Open,
        restart: false,
        since_ms: 1,
        stop: None,
        start: None,
        open: None,
        close: None,
        opened_by: None,
        settled: false,
        ended: None,
    });
    let mut removing = created.clone();
    removing.removal = Some(Token::new());

    assert_eq!(
        can(&created, "stopped", true),
        json!(["start", "maintain", "remove"])
    );
    assert_eq!(
        can(&created, "running", true),
        json!(["stop", "maintain", "remove"])
    );
    assert_eq!(
        can(&created, "running", false),
        json!(["maintain", "remove"]),
        "a power last known is not seen"
    );
    assert_eq!(can(&maintained, "running", true), json!(["end", "remove"]));
    assert_eq!(can(&removing, "stopped", true), json!(["remove"]));
    assert_eq!(
        can(&record(State::Adding), "stopped", true),
        json!(["remove"])
    );
}

#[test]
fn what_the_guest_allows_in_the_setup_window_is_listed_from_the_adds_record() {
    let home = tempfile::tempdir().unwrap();
    let lifecycle = Lifecycle::under(home.path());
    let allowed = |record: &Record| {
        of(
            &lifecycle,
            record,
            &word(&[], json!({}), Value::Null),
            &Value::Null,
        )["allowed"]
            .clone()
    };
    let mut adding = record(State::Adding);
    adding.done(Step::Closed);
    assert_eq!(allowed(&adding), Value::Null, "no window open yet");

    adding.done(Step::Shown);
    adding.setup = Some(crate::vm::lifecycle::record::Setup {
        recording: true,
        ..Default::default()
    });
    assert_eq!(
        allowed(&adding),
        json!({"screenCapture": true, "accessibility": false})
    );

    adding.done(Step::Allowed);
    assert_eq!(
        allowed(&adding),
        json!({"screenCapture": true, "accessibility": true})
    );
    assert_eq!(allowed(&record(State::Created)), Value::Null, "added");
}

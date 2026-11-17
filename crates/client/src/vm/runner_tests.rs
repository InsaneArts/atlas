use super::*;
use crate::vm::{
    lifecycle::{Lifecycle, Provenance, record::Record},
    tests::{Fake, desktop, home},
};

fn record(home: &Path) -> (Lifecycle, Record) {
    let lifecycle = Lifecycle::under(home);
    let reservation = lifecycle.reserve(Some("work"), &[]).unwrap();
    let record = Record::adding(
        &reservation,
        PathBuf::from("/Users/someone/.config/atlas.toml"),
        Provenance {
            program: PathBuf::from("/Applications/Atlas.app/Contents/MacOS/atlas"),
            requirement: "identifier \"com.insanearts.atlas\"".to_owned(),
        },
    );
    (lifecycle, record)
}

#[test]
fn each_operation_is_asked_as_atlas_vm_reads_it() {
    let change = Token::parse(&"a".repeat(48)).unwrap();
    let install = Token::parse(&"b".repeat(48)).unwrap();
    let enrolment = Token::parse(&"c".repeat(48)).unwrap();
    let cases = [
        (Operation::Hello, json!({"op": "hello"})),
        (Operation::State, json!({"op": "state"})),
        (
            Operation::Outcome(change.clone()),
            json!({"op": "outcome", "change": change}),
        ),
        (
            Operation::Create(change.clone()),
            json!({"op": "create", "change": change}),
        ),
        (
            Operation::Start(change.clone()),
            json!({"op": "start", "change": change}),
        ),
        (
            Operation::Stop(change.clone()),
            json!({"op": "stop", "change": change}),
        ),
        (
            Operation::Forget(change.clone()),
            json!({"op": "forget", "change": change}),
        ),
        (
            Operation::Window {
                change: change.clone(),
                input: true,
            },
            json!({"op": "window", "change": change, "input": true}),
        ),
        (
            Operation::Install {
                change: change.clone(),
                install: Install::new(
                    "/Applications/Atlas.app".into(),
                    enrolment.clone(),
                    "s".into(),
                ),
            },
            json!({"op": "install", "change": change, "bundle": "/Applications/Atlas.app",
                   "enrolment": enrolment, "secret": "s"}),
        ),
        (
            Operation::Paired {
                change: change.clone(),
                install: install.clone(),
                enrolment: enrolment.clone(),
            },
            json!({"op": "paired", "change": change, "install": install, "enrolment": enrolment}),
        ),
        (
            Operation::Closed {
                change: change.clone(),
                install: install.clone(),
            },
            json!({"op": "closed", "change": change, "install": install}),
        ),
        (
            Operation::InstallResume(install.clone()),
            json!({"op": "install-resume", "change": install}),
        ),
    ];
    for (operation, request) in cases {
        assert_eq!(operation.request(), request, "{}", operation.name());
        assert_eq!(operation.name(), request["op"]);
    }
}

#[test]
fn an_installs_digest_holds_no_secret_and_changes_with_it() {
    let install = |secret: &str| Operation::Install {
        change: Token::new(),
        install: Install::new(
            "/Applications/Atlas.app".into(),
            Token::new(),
            secret.into(),
        ),
    };
    let one = install("one");

    assert_eq!(one.digest().len(), 64);
    assert!(!one.digest().contains("one"));
    assert!(!format!("{one:?}").contains("\"one\""));
    let Operation::Install {
        change,
        install: kept,
    } = &one
    else {
        unreachable!()
    };
    let other = Operation::Install {
        change: change.clone(),
        install: Install::new(kept.bundle.clone(), kept.enrolment.clone(), "two".into()),
    };
    assert_ne!(one.digest(), other.digest());
}

#[test]
fn a_reply_is_read_as_atlas_vm_writes_it() {
    let (_home, vms) = home();
    let fake = Fake::new();
    *fake.reply.lock().unwrap() = Some(json!({
        "ok": true, "op": "state",
        "incarnation": {"pid": 7, "startSeconds": 1, "startMicroseconds": 2, "id": "x"},
        "power": "running", "confirmed": true, "later": "a field to come",
    }));

    let reply = read(&fake, &socket(&vms, &desktop()), &Operation::State).unwrap();

    assert!(reply.ok);
    assert_eq!(reply.power, Some(Power::Running));
    assert_eq!(reply.confirmed, Some(true));
    assert_eq!(reply.incarnation.unwrap().pid, 7);
    let outcome: Reply = serde_json::from_value(json!({
        "ok": true, "change": "a".repeat(48), "outcome": {"state": "refused", "reason": "no"},
    }))
    .unwrap();
    assert_eq!(
        outcome.outcome,
        Some(Outcome::Refused {
            reason: "no".to_owned()
        })
    );
    let hello: Reply = serde_json::from_value(json!({
        "ok": true, "hello": {"version": 1, "capabilities": ["install"]},
    }))
    .unwrap();
    assert_eq!(hello.hello.unwrap().capabilities, ["install"]);
}

#[test]
fn a_change_is_kept_in_the_record_before_it_is_sent() {
    let (home_folder, vms) = home();
    let fake = Fake::new();
    let (lifecycle, mut record) = record(home_folder.path());
    let change = Token::new();
    *fake.reply.lock().unwrap() = Some(json!({"ok": true, "change": change, "pending": true}));
    let asked_before_kept = std::cell::Cell::new(None);

    let reply = super::change(
        &fake,
        &socket(&vms, &desktop()),
        &mut record,
        |record| {
            asked_before_kept.set(Some(fake.asked.lock().unwrap().len()));
            lifecycle.keep_record(record)
        },
        &Operation::Create(change.clone()),
    )
    .unwrap();

    assert_eq!(asked_before_kept.get(), Some(0));
    assert_eq!(reply.pending, Some(true));
    let kept = lifecycle.record(&record.id).unwrap().unwrap();
    assert_eq!(kept.sent(&change).unwrap().op, "create");
    assert_eq!(fake.asked.lock().unwrap().len(), 1);
}

#[test]
fn a_change_whose_record_cannot_be_kept_is_never_sent() {
    let (home_folder, vms) = home();
    let fake = Fake::new();
    let (_lifecycle, mut record) = record(home_folder.path());

    let refused = super::change(
        &fake,
        &socket(&vms, &desktop()),
        &mut record,
        |_| Err(Error::new(Code::State, "The disk is full.")),
        &Operation::Start(Token::new()),
    );

    assert!(refused.is_err());
    assert!(fake.asked.lock().unwrap().is_empty());
}

#[test]
fn a_change_answered_for_another_is_refused_and_a_read_never_sends_a_change() {
    let (home_folder, vms) = home();
    let fake = Fake::new();
    let (lifecycle, mut record) = record(home_folder.path());
    *fake.reply.lock().unwrap() = Some(json!({"ok": true, "change": Token::new()}));

    let other = super::change(
        &fake,
        &socket(&vms, &desktop()),
        &mut record,
        |record| lifecycle.keep_record(record),
        &Operation::Stop(Token::new()),
    )
    .unwrap_err();
    let read = read(
        &fake,
        &socket(&vms, &desktop()),
        &Operation::Stop(Token::new()),
    );

    assert!(other.error.contains("another change"), "{}", other.error);
    assert!(read.is_err());
    assert_eq!(fake.asked.lock().unwrap().len(), 1);
}

#[test]
fn a_reply_that_is_not_one_is_refused() {
    let (_home, vms) = home();
    let fake = Fake::new();
    *fake.reply.lock().unwrap() = Some(json!({"ok": "yes"}));

    let refused = read(&fake, &socket(&vms, &desktop()), &Operation::Hello).unwrap_err();

    assert!(refused.error.contains("not a reply"), "{}", refused.error);
}

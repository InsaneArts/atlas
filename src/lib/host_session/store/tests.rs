use super::*;
use crate::error::{Code, Error};
use crate::{
    doing::Doing,
    host_session::{Host, HostSession, Instance},
    test_support::client,
};
use fs2::FileExt;
use serde_json::json;
use std::{
    collections::BTreeMap,
    fs::OpenOptions,
    os::unix::fs::{DirBuilderExt, PermissionsExt},
};

fn session(id: &str) -> HostSession {
    HostSession::new(Host::Claude, id).unwrap()
}

fn store(client: &Client) -> Store {
    Store::at(client.output_dir().unwrap())
}

/// A capture name `n` nanoseconds after a fixed moment.
fn capture(n: u64) -> Capture {
    Capture::parse(&format!("{}-capture.png", 1_761_660_429_000_000_000 + n)).unwrap()
}

/// Note `kind` for `session` doing `verb`, replying `reply`.
fn noted(client: &Client, session: &HostSession, kind: Kind, verb: &'static str, reply: Value) {
    note(client, session, kind, Some(&Doing::new(verb)), &Ok(reply)).unwrap();
}

/// Note an environment request of `session` that `changes` the helper, or
/// only previews, replying `reply`.
fn environment(client: &Client, session: &HostSession, changes: bool, reply: Value) {
    let kind = Kind::Environment { changes };
    let result = Ok(reply);
    note(client, session, kind, None, &result).unwrap();
    forget_rolled_back(client, kind, &result).unwrap();
}

#[test]
fn a_first_command_makes_the_record_and_later_ones_update_it() {
    let directory = tempfile::tempdir().unwrap();
    let client = client(directory.path());
    let session = session("first");

    noted(&client, &session, Kind::Other, "click", json!({"ok": true}));
    let first = store(&client).read(session.key()).unwrap();
    std::thread::sleep(Duration::from_millis(5));
    noted(
        &client,
        &session,
        Kind::Other,
        "click",
        json!({"ok": false}),
    );
    noted(&client, &session, Kind::Other, "type", json!({"ok": true}));
    let later = store(&client).read(session.key()).unwrap();

    let files: Vec<_> = fs::read_dir(client.output_dir().unwrap().join("sessions"))
        .unwrap()
        .flatten()
        .filter(|entry| entry.file_name().to_string_lossy().ends_with(".json"))
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(later.started, first.started);
    assert!(later.last_seen > first.last_seen);
    assert_eq!(
        later.counts,
        BTreeMap::from([("click".into(), 2), ("type".into(), 1)])
    );
    assert_eq!(later.host_session.as_str(), "first");
    assert_eq!(later.version, 1);
}

#[test]
fn reads_that_do_nothing_in_the_helper_record_nothing() {
    let directory = tempfile::tempdir().unwrap();
    let client = client(directory.path());
    let session = session("reader");

    note(
        &client,
        &session,
        Kind::Other,
        None,
        &Ok(json!({"ok": true})),
    )
    .unwrap();
    assert_eq!(store(&client).read(session.key()), None);

    environment(&client, &session, false, json!({"ok": true}));
    let record = store(&client).read(session.key()).unwrap();
    assert_eq!(record.counts, BTreeMap::from([("environment".into(), 1)]));
}

#[test]
fn a_refused_command_is_seen_but_not_counted() {
    let directory = tempfile::tempdir().unwrap();
    let client = client(directory.path());
    let session = session("refused");
    let click = Doing::new("click");

    for refusal in [
        Error::new(Code::Busy, "Another command has control"),
        Error::stopped(),
        Error::new(Code::Uncertain, "An earlier outcome is unknown"),
    ] {
        note(&client, &session, Kind::Other, Some(&click), &Err(refusal)).unwrap();
    }
    let refused = json!({"ok": false, "code": "busy", "uncertain": false});
    note(&client, &session, Kind::Other, Some(&click), &Ok(refused)).unwrap();
    let record = store(&client).read(session.key()).unwrap();
    assert!(record.counts.is_empty(), "{:?}", record.counts);

    let maybe = Error::new(Code::Transport, "No reply").uncertain();
    note(&client, &session, Kind::Other, Some(&click), &Err(maybe)).unwrap();
    let failed = json!({"ok": false, "code": "focus-changed", "uncertain": false});
    note(&client, &session, Kind::Other, Some(&click), &Ok(failed)).unwrap();
    let record = store(&client).read(session.key()).unwrap();
    assert_eq!(record.counts["click"], 2, "what may have acted counts");
}

#[test]
fn the_record_keeps_atlas_names_and_tokens_but_never_file_names_or_text() {
    let directory = tempfile::tempdir().unwrap();
    let client = client(directory.path());
    let session = session("private");
    let token = Token::new();
    let copy = format!("{}/{token}/Quarterly Plan.txt", handoff::ROOT);

    noted(
        &client,
        &session,
        Kind::Other,
        "type",
        json!({"ok": true, "typed": 12, "text": "secret words"}),
    );
    noted(
        &client,
        &session,
        Kind::Open,
        "open",
        json!({"ok": true, "opened": true, "copied": [{"from": "/tmp/Quarterly Plan.txt", "to": copy}]}),
    );
    let shot = capture(1);
    noted(
        &client,
        &session,
        Kind::Screenshot,
        "screenshot",
        json!({"ok": true, "path": format!("/somewhere/{}", shot.as_str())}),
    );

    let path = store(&client).folder.path(session.key());
    let written = fs::read_to_string(&path).unwrap();
    assert!(!written.contains("secret"), "{written}");
    assert!(!written.contains("Quarterly"), "{written}");
    assert!(!written.contains("/somewhere"), "{written}");
    let record = store(&client).read(session.key()).unwrap();
    assert_eq!(record.copies, [token]);
    assert_eq!(record.screenshots, [shot]);
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    let folder = path.parent().unwrap();
    assert_eq!(
        fs::metadata(folder).unwrap().permissions().mode() & 0o777,
        0o700
    );
}

#[test]
fn screenshots_keep_only_capture_names_newest_first_and_at_most_fifty() {
    let directory = tempfile::tempdir().unwrap();
    let client = client(directory.path());
    let session = session("shots");
    let take = |path: String| {
        noted(
            &client,
            &session,
            Kind::Screenshot,
            "screenshot",
            json!({"ok": true, "path": path}),
        );
    };

    for n in 0..55 {
        take(format!("/output/{}", capture(n).as_str()));
    }
    take("/output/notes.png".into());
    take(format!("/output/{}", capture(10).as_str()));

    let record = store(&client).read(session.key()).unwrap();
    assert_eq!(record.screenshots.len(), 50);
    assert_eq!(record.screenshots[0], capture(10), "a repeat moves up");
    assert_eq!(record.screenshots[1], capture(54));
    assert!(
        record
            .screenshots
            .iter()
            .all(|name| name.as_str() != "notes.png")
    );
    assert_eq!(record.counts["screenshot"], 57);
}

#[test]
fn a_close_drops_the_copies_it_removed() {
    let directory = tempfile::tempdir().unwrap();
    let client = client(directory.path());
    let session = session("closing");
    let (kept, gone) = (Token::new(), Token::new());
    let copied = |token: &Token| json!({"from": "/tmp/a.txt", "to": format!("{}/{token}/a.txt", handoff::ROOT)});

    noted(
        &client,
        &session,
        Kind::Open,
        "open",
        json!({"ok": false, "copied": [copied(&kept), copied(&gone)]}),
    );
    noted(
        &client,
        &session,
        Kind::Close,
        "close",
        json!({"ok": true, "removed": [format!("{}/{gone}", handoff::ROOT)]}),
    );

    let record = store(&client).read(session.key()).unwrap();
    assert_eq!(
        record.copies,
        [kept],
        "a failed launch's copies are kept until closed"
    );
}

#[test]
fn a_rolled_back_plan_leaves_the_record_and_a_preview_never_enters_it() {
    let directory = tempfile::tempdir().unwrap();
    let client = client(directory.path());
    let (applier, previewer) = (session("applier"), session("previewer"));
    let (plan, other) = (Token::new(), Token::new());

    environment(
        &client,
        &previewer,
        false,
        json!({"ok": true, "applied": false, "planId": other}),
    );
    environment(
        &client,
        &applier,
        true,
        json!({"ok": true, "planId": plan, "phase": "applied"}),
    );
    environment(
        &client,
        &applier,
        true,
        json!({"ok": true, "planId": "not-a-plan", "phase": "applied"}),
    );
    let record = store(&client).read(applier.key()).unwrap();
    assert_eq!(record.plans.len(), 1);
    assert_eq!(
        (&record.plans[0].plan_id, record.plans[0].phase),
        (&plan, Phase::Applied)
    );

    // A rollback preview answers with the transaction's own phase: it
    // looks at the plan, and changes and owns nothing.
    let preview = json!({"ok": true, "planId": plan, "phase": "applied", "applied": true,
        "rolledBack": false, "preview": true, "resumeRequired": false});
    environment(&client, &previewer, false, preview.clone());
    environment(&client, &previewer, true, preview);
    assert!(
        store(&client)
            .read(previewer.key())
            .unwrap()
            .plans
            .is_empty()
    );
    assert_eq!(store(&client).read(applier.key()).unwrap().plans.len(), 1);

    environment(
        &client,
        &applier,
        true,
        json!({"ok": true, "planId": plan, "phase": "rolledBack"}),
    );
    let record = store(&client).read(applier.key()).unwrap();
    assert!(record.plans.is_empty());
    assert_eq!(record.counts["environment"], 3);
}

#[test]
fn a_plan_rolled_back_by_anyone_leaves_every_record() {
    let directory = tempfile::tempdir().unwrap();
    let client = client(directory.path());
    let (applier, other) = (session("applier"), session("other"));
    let (plan, kept) = (Token::new(), Token::new());
    environment(
        &client,
        &applier,
        true,
        json!({"ok": true, "planId": plan, "phase": "applied"}),
    );
    environment(
        &client,
        &applier,
        true,
        json!({"ok": true, "planId": kept, "phase": "applied"}),
    );

    // Another session's failed rollback changes nothing.
    let failed =
        json!({"ok": false, "planId": plan, "phase": "rollingBack", "resumeRequired": true});
    environment(&client, &other, true, failed);
    assert_eq!(store(&client).read(applier.key()).unwrap().plans.len(), 2);

    // The person's rollback at a terminal: no session of its own.
    let rolled = Ok(json!({"ok": true, "planId": plan, "phase": "rolledBack", "rolledBack": true}));
    forget_rolled_back(&client, Kind::Environment { changes: true }, &rolled).unwrap();
    let record = store(&client).read(applier.key()).unwrap();
    assert_eq!(record.plans.len(), 1);
    assert_eq!(record.plans[0].plan_id, kept);
    assert_eq!(store(&client).read(other.key()).unwrap().plans.len(), 0);
}

#[test]
fn a_claude_session_after_clear_links_to_the_one_before_and_ends_it() {
    let directory = tempfile::tempdir().unwrap();
    let client = client(directory.path());
    let process = Instance::current().unwrap();
    let before = session("before").run_by(Some(process));
    let after = session("after").run_by(Some(process));

    noted(&client, &before, Kind::Other, "click", json!({"ok": true}));
    noted(&client, &after, Kind::Other, "click", json!({"ok": true}));

    let store = store(&client);
    let (before, after) = (
        store.read(before.key()).unwrap(),
        store.read(after.key()).unwrap(),
    );
    assert_eq!(after.previous, Some(before.key.clone()));
    assert!(before.ended.is_some());
    assert_eq!(after.ended, None);
    assert_eq!(after.host_process, Some(process));
}

#[test]
fn a_session_resumed_in_a_new_process_is_the_same_record_and_not_ended() {
    let directory = tempfile::tempdir().unwrap();
    let client = client(directory.path());
    let gone = session("resumed").run_by(Some(Instance::new(i32::MAX, [1, 0])));

    noted(&client, &gone, Kind::Other, "click", json!({"ok": true}));
    let ended = store(&client).read(gone.key()).unwrap();
    assert_eq!(
        ended.ended_at(),
        Some(ended.last_seen),
        "its process is gone"
    );

    let resumed = session("resumed").run_by(Instance::current());
    noted(&client, &resumed, Kind::Other, "click", json!({"ok": true}));
    let record = store(&client).read(gone.key()).unwrap();
    assert_eq!(record.ended_at(), None);
    assert_eq!(record.counts["click"], 2);
}

/// A process that is this one's ancestor: its parent.
fn parent() -> Instance {
    let pid = nix::unistd::getppid().as_raw();
    Instance::new(
        pid,
        crate::host_session::ancestry::process(pid).unwrap().start,
    )
}

#[test]
fn any_command_of_a_resumed_session_takes_its_new_process_before_it_acts() {
    let directory = tempfile::tempdir().unwrap();
    let client = client(directory.path());
    let gone = session("resumed").run_by(Some(Instance::new(i32::MAX, [1, 0])));
    noted(&client, &gone, Kind::Other, "click", json!({"ok": true}));
    let before = store(&client).read(gone.key()).unwrap();
    assert!(before.ended_at().is_some(), "its first process is gone");

    // A read records nothing of its own, but the session is seen, in the
    // host process this command runs in.
    let resumed = session("resumed").run_by(Some(parent()));
    assert_eq!(resumed.ancestor(), Some(parent()));
    touch(&client, &resumed).unwrap();
    let after = store(&client).read(gone.key()).unwrap();
    assert_eq!(after.host_process, Some(parent()));
    assert_eq!(after.ended_at(), None);
    assert!(after.active(now_ms()));
    assert_eq!(after.counts, before.counts, "nothing is counted");
}

#[test]
fn a_forged_id_naming_an_unrelated_process_neither_revives_nor_takes_a_record() {
    let directory = tempfile::tempdir().unwrap();
    let client = client(directory.path());
    let real = session("real").run_by(Some(Instance::new(i32::MAX, [1, 0])));
    noted(&client, &real, Kind::Other, "click", json!({"ok": true}));
    let mut ended = store(&client).read(real.key()).unwrap();
    ended.last_seen -= 60_000;
    ended.ended = Some(ended.last_seen);
    store(&client).write(&ended).unwrap();

    // A live process, but not one this command runs in: the command itself.
    let forged = session("real").run_by(Instance::current());
    assert_eq!(forged.ancestor(), None, "not this command's host");
    touch(&client, &forged).unwrap();
    assert_eq!(
        store(&client).read(real.key()).unwrap(),
        ended,
        "nothing changes: not its process, its end, nor when it was seen"
    );

    // A record that has not ended is seen again, and no more.
    let mut open = ended.clone();
    open.ended = None;
    store(&client).write(&open).unwrap();
    touch(&client, &forged).unwrap();
    let seen = store(&client).read(real.key()).unwrap();
    assert_eq!(seen.host_process, open.host_process);
    assert!(seen.last_seen > open.last_seen);
}

#[test]
fn a_touch_writes_only_what_changed_and_never_makes_a_record() {
    let directory = tempfile::tempdir().unwrap();
    let client = client(directory.path());
    let store = store(&client);
    let fresh = session("fresh").run_by(Some(parent()));

    touch(&client, &fresh).unwrap();
    assert_eq!(store.read(fresh.key()), None, "no record is made");

    noted(&client, &fresh, Kind::Other, "click", json!({"ok": true}));
    let written = store.read(fresh.key()).unwrap();
    store
        .touch(fresh.key(), Some(parent()), written.last_seen + 1000)
        .unwrap();
    assert_eq!(
        store.read(fresh.key()).unwrap(),
        written,
        "nothing to write"
    );

    let later = written.last_seen + 60_000;
    store.touch(fresh.key(), Some(parent()), later).unwrap();
    assert_eq!(store.read(fresh.key()).unwrap().last_seen, later);
}

#[test]
fn a_session_ended_by_clear_stays_ended_unless_resumed_in_another_process() {
    let directory = tempfile::tempdir().unwrap();
    let client = client(directory.path());
    let before = session("before").run_by(Some(parent()));
    let after = session("after").run_by(Some(parent()));
    noted(&client, &before, Kind::Other, "click", json!({"ok": true}));
    noted(&client, &after, Kind::Other, "click", json!({"ok": true}));
    let cleared = store(&client).read(before.key()).unwrap();
    assert!(cleared.ended.is_some());

    let later = cleared.last_seen + 60_000;
    store(&client)
        .touch(before.key(), Some(parent()), later)
        .unwrap();
    assert_eq!(
        store(&client).read(before.key()).unwrap(),
        cleared,
        "the same process, so still ended"
    );
    store(&client).touch(before.key(), None, later).unwrap();
    assert_eq!(store(&client).read(before.key()).unwrap(), cleared);

    let resumed = Instance::current().unwrap();
    store(&client)
        .touch(before.key(), Some(resumed), later)
        .unwrap();
    let revived = store(&client).read(before.key()).unwrap();
    assert_eq!(revived.ended, None, "resumed in a process of its own");
    assert_eq!(revived.host_process, Some(resumed));
}

#[test]
fn a_touch_never_waits_for_another_command_writing_the_records() {
    let directory = tempfile::tempdir().unwrap();
    let client = client(directory.path());
    let session = session("contended").run_by(Some(Instance::new(i32::MAX, [1, 0])));
    noted(&client, &session, Kind::Other, "click", json!({"ok": true}));
    let before = fs::read(store(&client).folder.path(session.key())).unwrap();

    let other = OpenOptions::new()
        .write(true)
        .open(client.output_dir().unwrap().join("sessions/.lock"))
        .unwrap();
    FileExt::lock_exclusive(&other).unwrap();
    let resumed = session.clone().run_by(Some(parent()));
    let started = Instant::now();
    let touched = touch(&client, &resumed);
    let waited = started.elapsed();
    let _ = FileExt::unlock(&other);

    assert_eq!(touched.unwrap_err().code, "busy");
    assert!(waited < SESSIONS.lock_wait, "it tried once: {waited:?}");
    assert_eq!(
        fs::read(store(&client).folder.path(session.key())).unwrap(),
        before
    );
}

/// A record of `id` last seen `days_ago`, written straight into `store`.
fn aged(store: &Store, id: &str, days_ago: u64, plans: usize) -> Record {
    let mut record = Record::new(&session(id), 0, None);
    record.last_seen = now_ms().saturating_sub(days_ago * 24 * 60 * 60 * 1000);
    record.started = record.last_seen;
    for _ in 0..plans {
        record.planned(Token::new(), Phase::Applied, record.last_seen);
    }
    store.write(&record).unwrap();
    record
}

#[test]
fn records_idle_a_week_go_unless_they_list_leftovers() {
    let directory = tempfile::tempdir().unwrap();
    let client = client(directory.path());
    let copies = directory.path().join("copies");
    fs::DirBuilder::new().mode(0o700).create(&copies).unwrap();
    let store = store(&client).copying_into(&copies);
    store.folder.make().unwrap();

    let mut copied = aged(&store, "copied", 8, 0);
    let token = Token::new();
    fs::DirBuilder::new()
        .mode(0o700)
        .create(copies.join(token.as_str()))
        .unwrap();
    copied.copies.push(token);
    store.write(&copied).unwrap();
    let planned = aged(&store, "planned", 8, 1);
    let idle = aged(&store, "idle", 8, 0);
    let recent = aged(&store, "recent", 1, 0);

    store
        .update(&session("new"), now_ms(), &client.controller, |_| {})
        .unwrap();

    assert!(store.read(&copied.key).is_some(), "its copy is still there");
    assert!(
        store.read(&planned.key).is_some(),
        "its plan is not rolled back"
    );
    assert!(store.read(&recent.key).is_some());
    assert_eq!(store.read(&idle.key), None);
}

#[test]
fn at_most_sixty_four_records_stay_and_those_with_leftovers_go_last() {
    let directory = tempfile::tempdir().unwrap();
    let client = client(directory.path());
    let store = store(&client);
    store.folder.make().unwrap();

    let with_leftovers: Vec<Record> = (0..3)
        .map(|n| aged(&store, &format!("left-{n}"), 6, 1))
        .collect();
    let others: Vec<Record> = (0..MAX_RECORDS)
        .map(|n| aged(&store, &format!("other-{n}"), 5 - (n as u64 % 5), 0))
        .collect();

    let new = session("newest");
    store
        .update(&new, now_ms(), &client.controller, |_| {})
        .unwrap();

    let left = store.listed(LISTED);
    assert_eq!(left.len(), MAX_RECORDS);
    assert!(
        left.iter().any(|record| record.key == *new.key()),
        "the new one stays"
    );
    for record in &with_leftovers {
        assert!(
            store.read(&record.key).is_some(),
            "the oldest, but with leftovers"
        );
    }
    let gone = others
        .iter()
        .filter(|record| store.read(&record.key).is_none())
        .count();
    assert_eq!(gone, 3 + 1);
}

#[test]
fn a_record_that_cannot_be_written_never_fails_the_command() {
    let directory = tempfile::tempdir().unwrap();
    let client = client(directory.path());
    let output = client.output_dir().unwrap().to_owned();
    fs::set_permissions(&output, fs::Permissions::from_mode(0o500)).unwrap();

    let reply = json!({"ok": true, "performed": 1});
    let result: Result<Value> = Ok(reply.clone());
    let noted = note(
        &client,
        &session("blocked"),
        Kind::Other,
        Some(&Doing::new("click")),
        &result,
    );
    fs::set_permissions(&output, fs::Permissions::from_mode(0o700)).unwrap();

    assert!(noted.is_err());
    assert_eq!(
        result.ok(),
        Some(reply),
        "the command's own result is untouched"
    );
    assert!(!output.join("sessions").exists());
}

#[test]
fn a_command_whose_record_is_being_written_elsewhere_returns_without_recording() {
    let directory = tempfile::tempdir().unwrap();
    let client = client(directory.path());
    let session = session("contended");
    noted(&client, &session, Kind::Other, "click", json!({"ok": true}));
    let before = fs::read(store(&client).folder.path(session.key())).unwrap();

    // Another command holds the records' lock, as one suspended midway
    // would, for as long as this one runs.
    let other = OpenOptions::new()
        .write(true)
        .open(client.output_dir().unwrap().join("sessions/.lock"))
        .unwrap();
    FileExt::lock_exclusive(&other).unwrap();

    let reply = json!({"ok": true, "performed": 1});
    let result: Result<Value> = Ok(reply.clone());
    let started = Instant::now();
    let noted = note(
        &client,
        &session,
        Kind::Other,
        Some(&Doing::new("click")),
        &result,
    );
    let waited = started.elapsed();
    let _ = FileExt::unlock(&other);

    assert_eq!(noted.unwrap_err().code, "busy");
    assert!(waited < Duration::from_secs(2), "{waited:?}");
    assert_eq!(
        result.ok(),
        Some(reply),
        "the command's own result is untouched"
    );
    assert_eq!(
        fs::read(store(&client).folder.path(session.key())).unwrap(),
        before
    );
}

#[test]
fn a_reader_never_sees_half_a_record() {
    let directory = tempfile::tempdir().unwrap();
    let client = client(directory.path());
    let session = session("busy");
    noted(&client, &session, Kind::Other, "click", json!({"ok": true}));
    let path = store(&client).folder.path(session.key());

    let writing = {
        let (client, session) = (client.clone(), session.clone());
        std::thread::spawn(move || {
            for n in 0..200 {
                let shot = json!({"ok": true, "path": format!("/o/{}", capture(n).as_str())});
                noted(&client, &session, Kind::Screenshot, "screenshot", shot);
            }
        })
    };
    let mut reads = 0;
    while !writing.is_finished() {
        if let Ok(bytes) = fs::read(&path) {
            serde_json::from_slice::<Record>(&bytes).unwrap();
            reads += 1;
        }
    }
    writing.join().unwrap();
    assert!(reads > 0);
}

#[test]
fn a_record_through_a_link_or_too_large_or_with_foreign_names_is_not_read() {
    let directory = tempfile::tempdir().unwrap();
    let client = client(directory.path());
    let session = session("linked");
    noted(&client, &session, Kind::Other, "click", json!({"ok": true}));
    let store = store(&client);
    let path = store.folder.path(session.key());

    let elsewhere = directory.path().join("elsewhere.json");
    fs::rename(&path, &elsewhere).unwrap();
    std::os::unix::fs::symlink(&elsewhere, &path).unwrap();
    assert_eq!(store.read(session.key()), None);
    assert!(store.all().is_empty());

    fs::remove_file(&path).unwrap();
    let mut large = fs::read_to_string(&elsewhere).unwrap();
    large.insert_str(
        1,
        &format!("\"padding\":\"{}\",", "x".repeat(SESSIONS.bytes as usize)),
    );
    fs::write(&path, large).unwrap();
    assert_eq!(store.read(session.key()), None);

    let mut foreign: Value = serde_json::from_slice(&fs::read(&elsewhere).unwrap()).unwrap();
    for (field, value) in [
        ("copies", json!(["../../Users/me/Documents"])),
        ("screenshots", json!(["/Users/me/Desktop/notes.png"])),
        ("hostSession", json!("a session with spaces")),
    ] {
        let mut record = foreign.clone();
        record[field] = value;
        fs::write(&path, record.to_string()).unwrap();
        assert_eq!(store.read(session.key()), None, "{field}");
    }
    foreign["plans"] = json!([{"planId": "nope", "phase": "applied", "at": 1}]);
    fs::write(&path, foreign.to_string()).unwrap();
    assert_eq!(store.read(session.key()), None);

    let sessions = client.output_dir().unwrap().join("sessions");
    fs::remove_dir_all(&sessions).unwrap();
    let real = directory.path().join("real-sessions");
    fs::DirBuilder::new().mode(0o700).create(&real).unwrap();
    std::os::unix::fs::symlink(&real, &sessions).unwrap();
    assert!(store.folder.make().is_err(), "a linked folder is refused");
    assert!(store.all().is_empty());
}

#[test]
fn a_status_poll_reads_the_records_only_when_they_changed() {
    let directory = tempfile::tempdir().unwrap();
    let client = client(directory.path());
    let store = store(&client);
    let kept = Mutex::new(None);
    let reads = std::cell::Cell::new(0);
    let counting = |store: &Store| {
        reads.set(reads.get() + 1);
        store.all()
    };

    assert!(store.recent_in(&kept, counting).is_empty());
    assert_eq!(reads.get(), 0, "no folder, nothing to read");

    noted(
        &client,
        &session("polled"),
        Kind::Other,
        "click",
        json!({"ok": true}),
    );
    assert_eq!(store.recent_in(&kept, counting).len(), 1);
    assert_eq!(store.recent_in(&kept, counting).len(), 1);
    assert_eq!(reads.get(), 1, "the second poll reads nothing");

    std::thread::sleep(Duration::from_millis(5));
    noted(
        &client,
        &session("another"),
        Kind::Other,
        "click",
        json!({"ok": true}),
    );
    assert_eq!(store.recent_in(&kept, counting).len(), 2);
    assert_eq!(reads.get(), 2);

    // Copies can go without a record changing: counted again in a while.
    if let Some(snapshot) = kept.lock().unwrap().as_mut() {
        snapshot.taken -= COPIES_FRESH;
    }
    store.recent_in(&kept, counting);
    assert_eq!(reads.get(), 3);
}

#[test]
fn a_status_read_counts_the_copy_folders_still_there() {
    let directory = tempfile::tempdir().unwrap();
    let client = client(directory.path());
    let copies = directory.path().join("copies");
    fs::DirBuilder::new().mode(0o700).create(&copies).unwrap();
    let store = store(&client).copying_into(&copies);
    store.folder.make().unwrap();
    let mut record = aged(&store, "copier", 0, 0);
    let (there, gone) = (Token::new(), Token::new());
    fs::DirBuilder::new()
        .mode(0o700)
        .create(copies.join(there.as_str()))
        .unwrap();
    record.copies = vec![there, gone];
    store.write(&record).unwrap();

    let seen = store.recent_in(&Mutex::new(None), Store::all);
    assert_eq!(seen[0].copies_left, 1);
}

#[test]
fn a_new_record_works_in_its_sessions_folder() {
    let directory = tempfile::tempdir().unwrap();
    let client = client(directory.path());
    let left = session("left").in_folder(Some("left"));
    noted(&client, &left, Kind::Other, "click", json!({"ok": true}));
    assert_eq!(
        store(&client).read(left.key()).unwrap().folder.as_deref(),
        Some("left")
    );

    let nowhere = session("nowhere");
    noted(&client, &nowhere, Kind::Other, "click", json!({"ok": true}));
    assert_eq!(store(&client).read(nowhere.key()).unwrap().folder, None);
}

#[test]
fn a_session_works_in_its_projects_folder_by_name_only() {
    let directory = tempfile::tempdir().unwrap();
    let project = directory.path().join("left");
    let inner = project.join("src/deep");
    fs::create_dir_all(&inner).unwrap();
    fs::create_dir(project.join(".git")).unwrap();

    assert_eq!(project_folder(&inner).as_deref(), Some("left"));
    let loose = directory.path().join("loose");
    fs::create_dir(&loose).unwrap();
    assert_eq!(project_folder(&loose).as_deref(), Some("loose"));
    assert_eq!(project_folder(Path::new("/")), None);
    if let Some(home) = std::env::var_os("HOME") {
        assert_eq!(project_folder(Path::new(&home)), None);
    }
}

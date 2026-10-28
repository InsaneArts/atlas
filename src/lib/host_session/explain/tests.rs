use super::*;
use crate::{
    control::Note,
    doing::Doing,
    error::Error,
    host_session::{
        Host, Process, ancestry,
        store::{Kind, note},
    },
    ui::hints,
};
use std::{fs, path::Path};

fn client(directory: &Path) -> Client {
    crate::transaction_test::client(directory)
}

fn session(host: Host, id: &str) -> HostSession {
    HostSession::new(host, id).unwrap()
}

/// Record that `session` worked in the helper, so it has a name.
fn recorded(client: &Client, session: &HostSession) {
    note(
        client,
        session,
        Kind::Other,
        Some(&Doing::new("click")),
        &Ok(json!({"ok": true})),
    )
    .unwrap();
}

/// The process that started this one, as a live command other than this one.
fn another_process() -> Process {
    let parent = nix::unistd::getppid().as_raw();
    Process::new(parent, ancestry::process(parent).unwrap().start)
}

/// A holder note naming `session`, as another command `process` left it.
fn holder(client: &Client, session: &HostSession, process: Process) {
    let path = client.controller.holder_path();
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let note = Note {
        session: Some(session.key.clone()),
        host: Some(session.host),
        folder: None,
        named: None,
        process,
        since: 1_761_660_300_000,
    };
    fs::write(path, serde_json::to_vec(&note).unwrap()).unwrap();
}

fn busy() -> Result<Value> {
    Err(Error::new("busy", "Another Atlas command kept control.").held_by(Holder::Agent))
}

#[test]
fn busy_names_the_session_that_holds_the_helper() {
    let directory = tempfile::tempdir().unwrap();
    let client = client(directory.path());
    let codex = session(Host::Codex, "right");
    recorded(&client, &codex);
    holder(&client, &codex, another_process());

    let error = reply(busy(), || Some(client.clone()), None).unwrap_err();
    let held = &error.details["heldBy"];
    assert_eq!(held["session"], codex.key.as_str());
    assert_eq!(held["host"], "codex");
    assert_eq!(held["since"], 1_761_660_300_000_u64);
    assert!(
        held["name"].as_str().unwrap().starts_with("Codex"),
        "{held}"
    );
    assert_eq!(error.code, "busy");
    assert_eq!(error.holder, Some(Holder::Agent));
}

#[test]
fn busy_names_a_holder_on_its_first_command_by_its_host_and_folder() {
    let directory = tempfile::tempdir().unwrap();
    let client = client(directory.path());
    let codex = session(Host::Codex, "first");
    let path = client.controller.holder_path();
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let note = Note {
        session: Some(codex.key.clone()),
        host: Some(codex.host),
        folder: Some("left".into()),
        named: None,
        process: another_process(),
        since: 1_761_660_300_000,
    };
    fs::write(path, serde_json::to_vec(&note).unwrap()).unwrap();

    let error = reply(busy(), || Some(client.clone()), None).unwrap_err();
    assert_eq!(
        error.details["heldBy"]["name"], "Codex · left",
        "as the status names it, with no record yet"
    );
}

#[test]
fn a_note_whose_process_has_ended_names_nobody() {
    let directory = tempfile::tempdir().unwrap();
    let client = client(directory.path());
    let codex = session(Host::Codex, "right");
    let alive = another_process();

    for gone in [
        Process::new(i32::MAX, alive.start),
        Process::new(alive.pid, [alive.start[0] + 1, 0]),
        Process::current().unwrap(),
    ] {
        holder(&client, &codex, gone);
        let error = reply(busy(), || Some(client.clone()), None).unwrap_err();
        assert!(!error.details.contains_key("heldBy"), "{gone:?}");
    }
}

#[test]
fn busy_from_the_person_never_names_a_session() {
    let directory = tempfile::tempdir().unwrap();
    let client = client(directory.path());
    holder(&client, &session(Host::Codex, "right"), another_process());

    let person = Err(Error::new("busy", crate::error::PERSON_HAS_CONTROL).held_by(Holder::Person));
    let error = reply(person, || Some(client.clone()), None).unwrap_err();
    assert!(error.details.is_empty());
}

/// A row `seconds_ago` of an agent's command in `app` for `session`.
fn row(seconds_ago: u64, session: &HostSession, app: &str) -> Value {
    let at = crate::client::timestamp() - u128::from(seconds_ago) * 1_000_000_000;
    json!({
        "at": at.to_string(), "operation": "input", "ok": true, "elapsedMs": 4.0,
        "actor": "agent", "verb": "click", "command": &crate::control::token()[..12],
        "app": app, "session": session.key,
    })
}

/// The activity log holding `rows`, oldest first as it is written.
fn logged(client: &Client, rows: &[Value]) {
    let text: String = rows.iter().map(|row| format!("{row}\n")).collect();
    fs::write(client.output_dir().unwrap().join("metadata.jsonl"), text).unwrap();
}

fn focus_changed(app: &str) -> Result<Value> {
    Ok(
        json!({"ok": false, "code": "focus-changed", "reason": "app",
        "found": {"app": app, "pid": 900}, "performed": 0, "partial": false, "uncertain": false}),
    )
}

#[test]
fn focus_changed_names_who_last_used_the_app_in_front() {
    let directory = tempfile::tempdir().unwrap();
    let client = client(directory.path());
    let (claude, codex, pi) = (
        session(Host::Claude, "left"),
        session(Host::Codex, "right"),
        session(Host::Pi, "middle"),
    );
    for each in [&claude, &codex, &pi] {
        recorded(&client, each);
    }
    let last_used = |rows: &[Value], app: &str| {
        logged(&client, rows);
        let value = reply(focus_changed(app), || Some(client.clone()), Some(&claude)).unwrap();
        value["lastUsedBy"]["session"].as_str().map(str::to_owned)
    };

    let rows = [
        row(60, &codex, "Calculator"),
        row(30, &pi, "calculator"),
        row(5, &claude, "Calculator"),
    ];
    assert_eq!(
        last_used(&rows, "Calculator").as_deref(),
        Some(pi.key.as_str())
    );
    let value = reply(
        focus_changed("Calculator"),
        || Some(client.clone()),
        Some(&claude),
    )
    .unwrap();
    assert!(
        value["lastUsedBy"]["name"]
            .as_str()
            .unwrap()
            .starts_with("Pi")
    );
    assert_eq!(value["found"]["app"], "Calculator", "found is unchanged");

    assert_eq!(
        last_used(&[row(11 * 60, &codex, "Calculator")], "Calculator"),
        None
    );
    assert_eq!(
        last_used(&[row(5, &claude, "Calculator")], "Calculator"),
        None
    );
    assert_eq!(last_used(&[row(5, &codex, "TextEdit")], "Calculator"), None);
}

/// A pending change claimed for `session`, left with no process carrying
/// it out.
fn abandoned(client: &Client, session: Option<&HostSession>) {
    let controller = client.controller.clone().labelled(session);
    drop(controller.quarantine().unwrap());
}

fn uncertain() -> Result<Value> {
    Err(Error::new(
        "uncertain",
        "The last action's outcome is unknown; Atlas waits until it is recovered.",
    ))
}

#[test]
fn an_unknown_outcome_this_session_caused_says_yours_and_how_to_recover() {
    let directory = tempfile::tempdir().unwrap();
    let client = client(directory.path());
    let claude = session(Host::Claude, "left");
    abandoned(&client, Some(&claude));

    let error = reply(uncertain(), || Some(client.clone()), Some(&claude)).unwrap_err();
    assert_eq!(error.details["yours"], true);
    assert_eq!(error.details["next"], hints::UNKNOWN_OWN);
    assert!(!error.details.contains_key("causedBy"));
    assert!(!error.uncertain);
}

#[test]
fn an_unknown_outcome_another_session_caused_says_whose() {
    let directory = tempfile::tempdir().unwrap();
    let client = client(directory.path());
    let (claude, codex) = (session(Host::Claude, "left"), session(Host::Codex, "right"));
    recorded(&client, &claude);
    abandoned(&client, Some(&claude));
    abandoned(&client, Some(&codex));

    let error = reply(uncertain(), || Some(client.clone()), Some(&codex)).unwrap_err();
    assert_eq!(error.details["yours"], false);
    let caused = error.details["causedBy"].as_array().unwrap();
    assert_eq!(caused.len(), 1);
    assert!(caused[0].as_str().unwrap().starts_with("Claude Code"));
    assert_eq!(
        error.details["next"],
        hints::UNKNOWN_OTHERS,
        "wait, or tell the person"
    );
}

#[test]
fn a_running_change_is_never_yours_to_recover() {
    let directory = tempfile::tempdir().unwrap();
    let client = client(directory.path());
    let claude = session(Host::Claude, "left");
    let running = client
        .controller
        .clone()
        .labelled(Some(&claude))
        .quarantine()
        .unwrap();

    let error = reply(uncertain(), || Some(client.clone()), Some(&claude)).unwrap_err();
    assert!(!error.details.contains_key("yours"));
    assert!(!error.details.contains_key("next"));
    drop(running);
}

#[test]
fn without_a_session_an_unknown_outcome_says_whose_but_never_yours() {
    let directory = tempfile::tempdir().unwrap();
    let client = client(directory.path());
    let claude = session(Host::Claude, "left");
    abandoned(&client, Some(&claude));

    let error = reply(uncertain(), || Some(client.clone()), None).unwrap_err();
    assert!(
        !error.details.contains_key("yours"),
        "a command with no session is not told it is not its own"
    );
    assert_eq!(error.details["causedBy"].as_array().unwrap().len(), 1);
    assert_eq!(error.details["next"], hints::UNKNOWN_OTHERS);

    let unlabelled = tempfile::tempdir().unwrap();
    let other = super::tests::client(unlabelled.path());
    abandoned(&other, None);
    let error = reply(uncertain(), || Some(other.clone()), Some(&claude)).unwrap_err();
    assert_eq!(
        error.details.keys().collect::<Vec<_>>(),
        ["next"],
        "whose it is is unknown"
    );
    assert_eq!(error.details["next"], hints::UNKNOWN_OTHERS);
}

#[test]
fn a_reply_whose_own_action_was_uncertain_is_left_as_it_is() {
    let directory = tempfile::tempdir().unwrap();
    let client = client(directory.path());
    let claude = session(Host::Claude, "left");
    abandoned(&client, Some(&claude));

    let own = Err(Error::new(
        "uncertain",
        "Input finished but native release was not confirmed",
    )
    .uncertain());
    let error = reply(own, || Some(client.clone()), Some(&claude)).unwrap_err();
    assert!(error.details.is_empty());
    let transport = Err(Error::new("transport", "The helper did not answer").uncertain());
    assert!(
        reply(transport, || Some(client.clone()), Some(&claude))
            .unwrap_err()
            .details
            .is_empty()
    );
}

#[test]
fn nothing_but_refusals_is_changed() {
    let answered = json!({"ok": true, "performed": 1, "code": "busy", "holder": "agent"});
    let asked = std::cell::Cell::new(false);
    let client = || {
        asked.set(true);
        None
    };
    assert_eq!(reply(Ok(answered.clone()), client, None).unwrap(), answered);
    assert!(!asked.get(), "no client is made for a reply that worked");

    let other = Err(Error::new("input", "Check the action"));
    assert!(reply(other, || None, None).unwrap_err().details.is_empty());
}

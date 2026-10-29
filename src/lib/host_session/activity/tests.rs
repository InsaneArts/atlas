use super::*;
use crate::{
    doing::Doing,
    host_session::{
        Process,
        store::{Kind, note},
    },
};
use std::fs;

fn client(directory: &Path) -> Client {
    crate::transaction_test::client(directory)
}

fn session(host: Host, id: &str) -> HostSession {
    HostSession::new(host, id).unwrap()
}

/// A capture name `n` nanoseconds after a fixed moment.
fn capture(n: u64) -> String {
    format!("{}-capture.png", 1_761_660_429_000_000_000 + n)
}

/// Write a row of `session`'s command doing `verb`, as the client does.
fn act(client: &Client, session: &HostSession, verb: &'static str, reply: Value) {
    let mut acting = client.clone().doing(Doing::new(verb));
    acting.host_session = Some(session.clone());
    let request = json!({"op": if verb == "screenshot" { "screenshot" } else { "input" }});
    acting.record_for_test(&request, &reply);
    let kind = if verb == "screenshot" {
        Kind::Screenshot
    } else {
        Kind::Other
    };
    note(client, session, kind, Some(&Doing::new(verb)), &Ok(reply)).unwrap();
}

/// A screenshot `session` took, kept in the output folder as `name`.
fn shot(client: &Client, session: &HostSession, name: &str) {
    let output = client.output_dir().unwrap();
    fs::write(output.join(name), b"png").unwrap();
    let path = output.join(name).to_string_lossy().into_owned();
    act(
        client,
        session,
        "screenshot",
        json!({"ok": true, "path": path, "capture": name}),
    );
}

/// A plan `session` applied, by its id.
fn applied(client: &Client, session: &HostSession) -> String {
    let plan = crate::control::token();
    let reply = json!({"ok": true, "planId": plan, "phase": "applied"});
    note(
        client,
        session,
        Kind::Environment { changes: true },
        None,
        &Ok(reply),
    )
    .unwrap();
    plan
}

/// Two sessions' work: Claude Code clicked and took a screenshot and
/// applied a plan; Codex took a screenshot.
fn two_sessions(client: &Client) -> (HostSession, HostSession, String) {
    let (claude, codex) = (session(Host::Claude, "left"), session(Host::Codex, "right"));
    act(client, &claude, "click", json!({"ok": true}));
    shot(client, &claude, &capture(1));
    let plan = applied(client, &claude);
    shot(client, &codex, &capture(2));
    (claude, codex, plan)
}

#[test]
fn an_install_or_an_unfinished_cleanup_is_a_leftover() {
    use crate::task::{Claims, Owned, State, Task, record::CleanupState, store::Store as Tasks};

    let directory = tempfile::tempdir().unwrap();
    let client = client(directory.path());
    let claude = session(Host::Claude, "installing");
    act(&client, &claude, "click", json!({"ok": true}));
    let output = client.output_dir().unwrap();
    let task = Task::of(claude.key(), Some("probe")).unwrap();
    Claims::in_store(Tasks::at(output), task.clone())
        .claim(
            &Owned::Install {
                id: Token::new(),
                app: "Probe".into(),
            },
            State::Made,
        )
        .unwrap();
    Tasks::at(output)
        .update(&task, 1, |record| {
            record.noted_cleanup(&Token::new(), CleanupState::Incomplete, 1);
            Ok(())
        })
        .unwrap();

    let one = list(&client, Some(&claude), false, None, 10).unwrap();
    assert_eq!(one["installs"], json!([{"app": "Probe"}]));
    assert_eq!(one["cleanup"], "incomplete");
    assert_eq!(one["tasks"][0]["leftovers"]["installs"], 1);
    assert!(!one.to_string().contains("Applications"), "never a path");

    let everyone = list(&client, None, true, None, 10).unwrap();
    let leftovers = &everyone["sessions"][0]["leftovers"];
    assert_eq!(
        (&leftovers["installs"], &leftovers["cleanup"]),
        (&json!(1), &json!("incomplete"))
    );
}

#[test]
fn a_sessions_reply_lists_the_tasks_it_named_with_what_each_left() {
    use crate::task::{Claims, Owned, State, Task, store::Store as Tasks};

    let directory = tempfile::tempdir().unwrap();
    let client = client(directory.path());
    let claude = session(Host::Claude, "left");
    let plan = Token::parse(&applied(&client, &claude)).unwrap();
    let output = client.output_dir().unwrap();
    let claims = |name: Option<&str>| {
        let task = Task::of(claude.key(), name).unwrap();
        (Claims::in_store(Tasks::at(output), task.clone()), task)
    };
    let (named, probe) = claims(Some("probe"));
    named
        .claim(&Owned::Plan { id: plan.clone() }, State::Made)
        .unwrap();
    named
        .claim(&Owned::Copy { id: Token::new() }, State::Made)
        .unwrap();
    let (default, _) = claims(None);
    default
        .claim(&Owned::Plan { id: Token::new() }, State::Made)
        .unwrap();

    let reply = list(&client, Some(&claude), false, None, 10).unwrap();

    assert_eq!(
        reply["tasks"],
        json!([{"key": probe.key(), "name": "probe",
            "leftovers": {"copies": 0, "plans": 1}}])
    );
    let everyone = list(&client, Some(&claude), true, None, 10).unwrap();
    assert!(
        !everyone.to_string().contains("probe"),
        "names stay in one session's reply"
    );
}

#[test]
fn activity_lists_this_sessions_rows_screenshots_copies_and_plans_only() {
    let directory = tempfile::tempdir().unwrap();
    let client = client(directory.path());
    let (claude, codex, plan) = two_sessions(&client);

    let reply = list(&client, Some(&claude), false, None, 10).unwrap();
    assert_eq!(reply["session"]["key"], claude.key.as_str());
    assert_eq!(reply["session"]["this"], true);
    assert_eq!(reply["session"]["hostSession"], "left");
    assert_eq!(
        reply["counts"],
        json!({"click": 1, "environment": 1, "screenshot": 1})
    );
    let recent = reply["recent"].as_array().unwrap();
    assert_eq!(recent.len(), 2);
    assert_eq!(recent[0]["capture"], capture(1));
    assert_eq!(recent[0]["says"], "took a screenshot");
    let shots = reply["screenshots"].as_array().unwrap();
    assert_eq!(shots.len(), 1);
    assert!(shots[0].as_str().unwrap().ends_with(&capture(1)));
    assert_eq!(
        reply["plans"],
        json!([{"planId": plan, "phase": "applied"}])
    );
    assert_eq!(reply["uncertain"], 0);
    assert!(reply.get("next").is_none());

    let text = reply.to_string();
    assert!(!text.contains(codex.key.as_str()));
    assert!(!text.contains(&capture(2)));
    assert!(!text.contains("right"));
}

#[test]
fn activity_after_compaction_finds_the_same_session() {
    let directory = tempfile::tempdir().unwrap();
    let client = client(directory.path());
    let before = HostSession {
        process: Some(Process::new(i32::MAX, [1, 0])),
        ..session(Host::Claude, "left")
    };
    act(&client, &before, "click", json!({"ok": true}));
    shot(&client, &before, &capture(1));

    let after = HostSession {
        process: Process::current(),
        ..session(Host::Claude, "left")
    };
    let earlier = list(&client, Some(&before), false, None, 10).unwrap();
    let reply = list(&client, Some(&after), false, None, 10).unwrap();
    assert_eq!(after.key, before.key);
    for field in ["counts", "recent", "screenshots"] {
        assert_eq!(reply[field], earlier[field], "{field}");
    }
}

#[test]
fn all_lists_every_session_with_counts_but_no_paths_or_host_ids() {
    let directory = tempfile::tempdir().unwrap();
    let client = client(directory.path());
    let (claude, codex, plan) = two_sessions(&client);

    let reply = list(&client, Some(&claude), true, None, 10).unwrap();
    assert_eq!(
        reply["session"]["key"],
        claude.key.as_str(),
        "it finds itself"
    );
    let sessions = reply["sessions"].as_array().unwrap();
    assert_eq!(sessions.len(), 2);
    assert_eq!(sessions[0]["key"], codex.key.as_str(), "newest first");
    assert_eq!(sessions[1]["leftovers"]["plans"], 1);
    assert_eq!(sessions[1]["screenshots"], 1);
    assert_eq!(sessions[0]["active"], true);
    assert_eq!(sessions[0]["holdsHelper"], false);
    for session in sessions {
        assert!(session.get("hostSession").is_none());
    }
    let recent = reply["recent"].as_array().unwrap();
    assert!(recent[0]["session"].as_str().unwrap().starts_with("Codex"));

    let text = serde_json::to_string(&reply["sessions"]).unwrap() + &reply["recent"].to_string();
    assert!(!text.contains(&plan));
    assert!(!text.contains("/"), "no paths: {text}");
    assert!(!text.contains("\"right\""));
}

#[test]
fn without_a_session_activity_lists_everyone() {
    let directory = tempfile::tempdir().unwrap();
    let client = client(directory.path());
    two_sessions(&client);

    let reply = list(&client, None, false, None, 10).unwrap();
    assert_eq!(reply["session"], Value::Null);
    assert_eq!(reply["sessions"].as_array().unwrap().len(), 2);
}

#[test]
fn an_unknown_key_is_refused_with_input() {
    let directory = tempfile::tempdir().unwrap();
    let client = client(directory.path());
    let (_, codex, _) = two_sessions(&client);

    let unknown = Key::of(Host::Pi, "nobody");
    let refused = list(&client, None, false, Some(&unknown), 10).unwrap_err();
    assert_eq!(refused.code, "input");
    assert!(refused.error.contains("atlas activity --all"));

    let other = list(&client, None, false, Some(&codex.key), 10).unwrap();
    assert_eq!(other["session"]["this"], false);
    assert_eq!(other["session"]["key"], codex.key.as_str());
}

#[test]
fn only_the_session_itself_is_told_its_host_id() {
    let directory = tempfile::tempdir().unwrap();
    let client = client(directory.path());
    let (claude, codex, _) = two_sessions(&client);

    let other = list(&client, Some(&claude), false, Some(codex.key()), 10).unwrap();
    assert_eq!(other["session"]["this"], false);
    assert!(other["session"].get("hostSession").is_none(), "{other}");
    assert!(!other.to_string().contains("\"right\""));

    let itself = list(&client, Some(&codex), false, Some(codex.key()), 10).unwrap();
    assert_eq!(itself["session"]["this"], true, "its own key is itself");
    assert_eq!(itself["session"]["hostSession"], "right");
}

/// A pending change claimed for `session`, left with no process carrying
/// it out.
fn abandoned(client: &Client, session: &HostSession) {
    drop(
        client
            .controller
            .clone()
            .labelled(Some(session))
            .quarantine()
            .unwrap(),
    );
}

#[test]
fn only_the_session_itself_is_told_to_recover_and_only_when_every_unknown_is_its_own() {
    let directory = tempfile::tempdir().unwrap();
    let client = client(directory.path());
    let (claude, codex, _) = two_sessions(&client);
    abandoned(&client, &claude);

    let own = list(&client, Some(&claude), false, None, 10).unwrap();
    assert_eq!(own["uncertain"], 1);
    assert_eq!(own["next"], crate::ui::hints::UNKNOWN_OWN);

    let asked = list(&client, Some(&codex), false, Some(claude.key()), 10).unwrap();
    assert_eq!(asked["uncertain"], 1, "another's count is said");
    assert!(asked.get("next").is_none(), "but never to recover it");
    let unlabelled = list(&client, None, false, Some(claude.key()), 10).unwrap();
    assert!(unlabelled.get("next").is_none());

    abandoned(&client, &codex);
    let both = list(&client, Some(&claude), false, None, 10).unwrap();
    assert_eq!(both["uncertain"], 1);
    assert!(
        both.get("next").is_none(),
        "another session's is unknown too"
    );
}

/// Every file under `folder` and its bytes.
fn snapshot(folder: &Path) -> Vec<(std::path::PathBuf, Vec<u8>)> {
    let mut found = Vec::new();
    let mut folders = vec![folder.to_owned()];
    while let Some(folder) = folders.pop() {
        for entry in fs::read_dir(folder).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                folders.push(path);
            } else {
                found.push((path.clone(), fs::read(&path).unwrap()));
            }
        }
    }
    found.sort();
    found
}

#[test]
fn activity_writes_nothing() {
    let directory = tempfile::tempdir().unwrap();
    let client = client(directory.path());
    let (claude, ..) = two_sessions(&client);
    let before = snapshot(directory.path());

    list(&client, Some(&claude), false, None, 10).unwrap();
    list(&client, Some(&claude), true, None, 10).unwrap();
    list(&client, None, false, None, 50).unwrap();

    assert_eq!(snapshot(directory.path()), before);
}

#[test]
fn activity_works_while_another_controller_holds_the_lock_and_after_a_stop() {
    let directory = tempfile::tempdir().unwrap();
    let client = client(directory.path());
    let (claude, ..) = two_sessions(&client);

    let held = client
        .controller
        .clone()
        .labelled(Some(&claude))
        .acquire(false)
        .unwrap();
    client.controller.note_stop(true).unwrap();
    let reply = list(&client, Some(&claude), true, None, 10).unwrap();
    assert_eq!(reply["sessions"].as_array().unwrap().len(), 2);
    let holding = reply["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|session| session["key"] == claude.key.as_str())
        .unwrap()
        .clone();
    assert_eq!(holding["holdsHelper"], true);
    drop(held);
}

#[test]
fn a_previous_session_with_leftovers_is_mentioned_and_one_without_is_not() {
    let directory = tempfile::tempdir().unwrap();
    let client = client(directory.path());
    let process = Process::current().unwrap();
    let labelled = |id: &str| HostSession {
        process: Some(process),
        ..session(Host::Claude, id)
    };

    let (first, second, third) = (labelled("first"), labelled("second"), labelled("third"));
    applied(&client, &first);
    act(&client, &second, "click", json!({"ok": true}));
    let reply = list(&client, Some(&second), false, None, 10).unwrap();
    assert_eq!(reply["previous"]["key"], first.key.as_str());
    assert_eq!(reply["previous"]["plans"], 1);

    act(&client, &third, "click", json!({"ok": true}));
    let reply = list(&client, Some(&third), false, None, 10).unwrap();
    assert!(reply.get("previous").is_none(), "the second left nothing");
}

#[test]
fn a_sessions_jobs_and_collections_are_listed_without_their_names_or_arguments() {
    use crate::{
        daemon::{
            job::JobState,
            protocol::{Expected, JobId},
        },
        task::{
            Task,
            record::{JobEntry, OutputEntry},
            store::Store as Tasks,
        },
    };

    let directory = tempfile::tempdir().unwrap();
    let client = client(directory.path());
    let codex = session(Host::Codex, "jobs");
    act(&client, &codex, "click", json!({"ok": true}));
    let output = client.output_dir().unwrap();
    let task = Task::of(codex.key(), None).unwrap();
    Tasks::at(output)
        .update(&task, 1, |record| {
            record.noted_job(JobEntry {
                id: JobId::parse("7c2e91a04b3f").unwrap(),
                name: "web".into(),
                started_at: 1,
                expected: Expected {
                    boot: "B".into(),
                    pid: 4312,
                    start: [1, 2],
                    pgid: 4312,
                },
                state: JobState::Running,
                how: None,
                ended_at: None,
                later: serde_json::Map::new(),
            });
            record.noted_output(OutputEntry {
                receipt: crate::collect::ReceiptId::parse("1761650000123-a1b2c3").unwrap(),
                at: 1,
                files: 2,
                bytes: 30,
                later: serde_json::Map::new(),
            });
            Ok(())
        })
        .unwrap();

    let one = list(&client, Some(&codex), false, None, 10).unwrap();
    assert_eq!(
        one["jobs"],
        json!([{"id": "7c2e91a04b3f", "name": "web", "state": "running"}])
    );
    assert_eq!(
        one["outputs"],
        json!([{"receipt": "1761650000123-a1b2c3", "count": 2, "bytes": 30}])
    );
}

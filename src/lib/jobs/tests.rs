use super::*;
use crate::{config::Config, control::Controller, doing::Doing, task::Task};
use std::{
    path::Path,
    sync::{Arc, Mutex},
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::UnixListener,
};

/// The requests a stand-in daemon received, in order.
type Heard = Arc<Mutex<Vec<Value>>>;

/// A client whose helper daemon is a stand-in at `dir/c.sock`, served by
/// this test's own account, with control kept in `dir/state`.
fn client_in(dir: &Path) -> Client {
    let text = "helper_uid = 60001\nsocket_path = \"c.sock\"\noutput_dir = \"out\"\n\
                identity_probe = \"probe\"\ninput_socket = \"i.sock\"\n";
    let config = Config::parse(text.as_bytes(), &dir.join("u.toml")).unwrap();
    let mut client = Client::new(config).unwrap();
    client.controller = Controller::at(60001, &dir.join("state"));
    client.transport.peer_uid = nix::unistd::getuid().as_raw();
    client
}

/// Answer every request on `socket` with what `answer` says, keeping each
/// request in the returned list; `None` hangs up without a reply.
fn stand_in(
    socket: &Path,
    answer: impl Fn(&Value) -> Option<Value> + Send + Sync + 'static,
) -> Heard {
    let listener = UnixListener::bind(socket).unwrap();
    let heard: Heard = Arc::default();
    let kept = heard.clone();
    let answer = Arc::new(answer);
    tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            let (reader, mut writer) = stream.into_split();
            let mut line = String::new();
            let mut reader = BufReader::new(reader);
            if reader.read_line(&mut line).await.is_err() {
                continue;
            }
            let request: Value = serde_json::from_str(&line).unwrap();
            kept.lock().unwrap().push(request.clone());
            if let Some(reply) = answer(&request) {
                let _ = writer.write_all(format!("{reply}\n").as_bytes()).await;
            }
        }
    });
    heard
}

fn job_view(id: &str, state: &str) -> Value {
    json!({"id": id, "name": "web", "task": null, "session": null, "uid": 60001,
        "boot": "B", "pid": 4312, "start": [1761650000, 123456], "pgid": 4312,
        "state": state, "startedAt": 10, "endsAt": 20, "logResets": 0})
}

const ID: &str = "7c2e91a04b3f";

fn id() -> JobId {
    JobId::parse(ID).unwrap()
}

fn agent(client: Client, verb: &'static str) -> Client {
    let mut client = client.doing(Doing::new(verb));
    let host = crate::host_session::HostSession::new(crate::host_session::Host::Codex, "jobs-test")
        .unwrap();
    client.task = Some(Task::of(host.key(), None).unwrap());
    client.host_session = Some(host);
    client
}

#[tokio::test]
async fn a_job_start_holds_control_only_until_it_is_acknowledged_and_notes_the_job() {
    let dir = tempfile::tempdir().unwrap();
    let client = agent(client_in(dir.path()), "job-start");
    let heard = stand_in(&client.config.socket_path, |request| {
        assert_eq!(request["action"], "start");
        Some(json!({"ok": true, "op": "job", "action": "start", "job": job_view(ID, "running")}))
    });
    let mut client = client;
    client.config.helper_uid = 60001;
    let ownership = client.controller.acquire_beside_commands().unwrap();
    let reply = started(
        &client,
        None,
        vec!["/usr/bin/nc".into(), "-lk".into(), "8765".into()],
        Duration::from_secs(60),
        Some("web".into()),
    )
    .await
    .unwrap();
    drop(ownership);
    assert_eq!(
        reply,
        json!({"ok": true, "id": ID, "name": "web", "pid": 4312, "state": "running", "endsAt": 20})
    );
    let sent = heard.lock().unwrap()[0].clone();
    assert_eq!(sent["uid"], 60001);
    assert_eq!(sent["name"], "web");
    assert_eq!(sent["task"], json!(client.task.as_ref().unwrap().key()));
    assert!(client.controller.pending().unwrap().is_empty(), "settled");
    assert!(client.controller.acquire(true).is_ok(), "nothing is held");

    let record = Store::at(client.output_dir().unwrap())
        .load(client.task.as_ref().unwrap().key())
        .unwrap();
    let entry = record.job(&id()).unwrap();
    assert_eq!(entry.expected.pid, 4312);
    assert_eq!(entry.state, JobState::Running);
    assert!(record.holds_anything(), "a running job keeps its record");
}

#[tokio::test]
async fn a_start_reply_that_does_not_prove_its_job_is_uncertain() {
    for job in [
        json!({"pgid": 4313}),
        json!({"uid": 501}),
        json!({"boot": ""}),
        json!({"start": [0, 0]}),
        json!({"pid": 1, "pgid": 1}),
        json!({"state": "launched"}),
        json!({"id": "nope"}),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let client = agent(client_in(dir.path()), "job-start");
        let mut view = job_view(ID, "running");
        for (field, value) in job.as_object().unwrap() {
            view[field] = value.clone();
        }
        stand_in(&client.config.socket_path, move |_| {
            Some(json!({"ok": true, "op": "job", "action": "start", "job": view}))
        });
        let error = started(
            &client,
            None,
            vec!["/bin/sleep".into(), "9".into()],
            Duration::from_secs(60),
            None,
        )
        .await
        .unwrap_err();
        assert_eq!(error.code, "protocol", "{job}");
        assert!(error.uncertain, "{job}");
        assert_eq!(client.controller.pending().unwrap().len(), 1, "{job}");
    }
}

#[tokio::test]
async fn a_lost_start_reply_stays_pending_for_every_agent() {
    let dir = tempfile::tempdir().unwrap();
    let client = agent(client_in(dir.path()), "job-start");
    stand_in(&client.config.socket_path, |_| None);
    let error = started(
        &client,
        None,
        vec!["/bin/sleep".into(), "9".into()],
        Duration::from_secs(60),
        None,
    )
    .await
    .unwrap_err();
    assert!(error.uncertain, "{error:?}");
    assert_eq!(client.controller.pending().unwrap().len(), 1);
    let other = client_in(dir.path());
    assert_eq!(
        other.controller.acquire(false).unwrap_err().code,
        "uncertain"
    );
}

#[tokio::test]
async fn a_job_stop_waits_without_control() {
    let dir = tempfile::tempdir().unwrap();
    let client = agent(client_in(dir.path()), "job-stop");
    let shows = Arc::new(Mutex::new(0));
    let counted = shows.clone();
    stand_in(&client.config.socket_path, move |request| {
        Some(match request["action"].as_str() {
            Some("stop") => json!({"ok": true, "op": "job", "action": "stop",
                "job": job_view(ID, "stopping"), "signalled": "SIGTERM"}),
            _ => {
                let mut count = counted.lock().unwrap();
                *count += 1;
                let mut view = job_view(ID, if *count < 3 { "stopping" } else { "ended" });
                if *count >= 3 {
                    view["how"] = json!("stopped");
                    view["signal"] = json!("SIGTERM");
                    view["left"] = json!([]);
                }
                json!({"ok": true, "op": "job", "action": "show", "job": view})
            }
        })
    });
    let expected: Expected = serde_json::from_value(
        json!({"boot": "B", "pid": 4312, "start": [1761650000, 123456], "pgid": 4312}),
    )
    .unwrap();
    let ownership = client.controller.acquire_beside_commands().unwrap();
    let acknowledged = stop_held(&client, &id(), &expected, Duration::from_secs(1))
        .await
        .unwrap();
    drop(ownership);
    assert!(client.controller.pending().unwrap().is_empty());

    let job = id();
    let waiting = waited_for(
        &client,
        &job,
        Duration::from_secs(1),
        &acknowledged,
        Found {
            expected,
            owner: None,
            checked: Checked::Helper,
        },
    );
    let other = client_in(dir.path());
    let (reply, taken) = tokio::join!(waiting, async {
        tokio::time::sleep(Duration::from_millis(100)).await;
        other.controller.acquire(false).map(drop)
    });
    assert!(taken.is_ok(), "another agent acts while the stop waits");
    assert_eq!(reply["state"], "ended", "{reply}");
    assert_eq!(reply["how"], "stopped");
    assert_eq!(reply["checkedAgainst"], "helper");
    assert!(*shows.lock().unwrap() >= 3);
}

#[tokio::test]
async fn another_tasks_job_is_refused_without_its_key() {
    let dir = tempfile::tempdir().unwrap();
    let client = agent(client_in(dir.path()), "job-stop");
    let theirs = crate::host_session::Key::parse("3f9a1c07b2e4").unwrap();
    let task = theirs.clone();
    stand_in(&client.config.socket_path, move |_| {
        let mut view = job_view(ID, "running");
        view["task"] = json!(task);
        Some(json!({"ok": true, "op": "job", "action": "show", "job": view}))
    });
    let error = expected_for(&client, &id(), None).await.unwrap_err();
    assert_eq!(error.code, "job");
    assert_eq!(error.details["reason"], "other-task");
    assert_eq!(error.details["task"], json!(theirs));
    assert!(
        error.error.contains("--task 3f9a1c07b2e4"),
        "{}",
        error.error
    );

    // Named, a task whose record does not list it is refused too.
    let error = expected_for(&client, &id(), Some(&theirs))
        .await
        .unwrap_err();
    assert_eq!(error.details["reason"], "unknown");
}

#[tokio::test]
async fn a_job_is_stopped_only_as_its_tasks_record_says() {
    let dir = tempfile::tempdir().unwrap();
    let client = agent(client_in(dir.path()), "job-stop");
    let mut view = job_view(ID, "running");
    view["task"] = json!(client.task.as_ref().unwrap().key());
    note(&client, None, entry(&view).unwrap()).unwrap();
    // The helper's own record says otherwise; the task's copy is what is sent.
    let heard = stand_in(&client.config.socket_path, |_| {
        Some(
            json!({"ok": false, "code": "job", "reason": "identity", "id": ID,
            "error": "The job is no longer the process Atlas started, so nothing was signalled",
            "uncertain": false}),
        )
    });
    let found = expected_for(&client, &id(), None).await.unwrap();
    assert_eq!(found.checked, Checked::Task);
    assert_eq!(found.expected.start, [1761650000, 123456]);
    let error = stop_held(&client, &id(), &found.expected, Duration::from_secs(5))
        .await
        .unwrap_err();
    assert_eq!((error.code, error.uncertain), ("job", false));
    assert_eq!(error.details["reason"], "identity");
    assert_eq!(heard.lock().unwrap()[0]["expect"]["pid"], 4312);
    assert!(
        client.controller.pending().unwrap().is_empty(),
        "nothing was signalled"
    );
}

#[tokio::test]
async fn a_stopped_helper_refuses_job_starts_and_stops_but_lists_them() {
    let dir = tempfile::tempdir().unwrap();
    let client = agent(client_in(dir.path()), "job-start");
    client.controller.note_stop(true).unwrap();
    let heard = stand_in(&client.config.socket_path, |_| {
        Some(json!({"ok": true, "op": "job", "action": "show", "jobs": [job_view(ID, "running")]}))
    });
    for request in [
        json!({"op": "job", "action": "start", "argv": ["/bin/sleep", "9"]}),
        json!({"op": "job", "action": "stop", "id": ID}),
    ] {
        let error = client.change(request).await.unwrap_err();
        assert_eq!(error.code, "stopped");
        assert!(!error.uncertain);
    }
    assert!(heard.lock().unwrap().is_empty(), "nothing was sent");

    let listed = list(&client, true).await.unwrap();
    assert_eq!(listed["jobs"][0]["id"], ID);
    assert_eq!(listed["jobs"][0]["mine"], false);
}

#[tokio::test]
async fn an_unknown_outcome_blocks_a_job_stop_but_not_a_list() {
    let dir = tempfile::tempdir().unwrap();
    let client = agent(client_in(dir.path()), "job-stop");
    drop(client.controller.quarantine().unwrap());
    stand_in(&client.config.socket_path, |_| {
        Some(json!({"ok": true, "op": "job", "action": "show", "jobs": []}))
    });
    assert_eq!(
        client
            .controller
            .acquire_beside_commands()
            .unwrap_err()
            .code,
        "uncertain"
    );
    assert_eq!(list(&client, false).await.unwrap()["jobs"], json!([]));
}

#[tokio::test]
async fn a_list_without_a_helper_shows_the_tasks_jobs_as_unknown() {
    let dir = tempfile::tempdir().unwrap();
    let client = agent(client_in(dir.path()), "job-start");
    let mut view = job_view(ID, "running");
    view["task"] = json!(client.task.as_ref().unwrap().key());
    note(&client, None, entry(&view).unwrap()).unwrap();
    let listed = list(&client, false).await.unwrap();
    assert_eq!(listed["jobs"][0]["state"], "unknown");
    assert!(listed["warnings"].is_array());
}

#[tokio::test]
async fn the_persons_stop_asks_the_helper_to_end_every_job() {
    let dir = tempfile::tempdir().unwrap();
    let mut person = client_in(dir.path());
    person.actor = crate::client::Actor::Viewer;
    let heard = stand_in(&person.config.socket_path, |_| {
        Some(json!({"ok": true, "op": "job", "action": "stop-all", "stopping": 1}))
    });
    let reply = person.hold(true).await.unwrap();
    assert_eq!(reply["held"], true);
    let asked = heard.lock().unwrap().clone();
    assert_eq!(
        asked,
        [json!({"op": "job", "action": "stop-all", "reason": "person"})]
    );
    assert!(
        !reply["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|warning| warning.as_str().unwrap().contains("background jobs"))
    );

    // A helper that cannot be told is said.
    let dir = tempfile::tempdir().unwrap();
    let mut person = client_in(dir.path());
    person.actor = crate::client::Actor::Viewer;
    stand_in(&person.config.socket_path, |_| None);
    let reply = person.hold(true).await.unwrap();
    assert!(
        reply["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|warning| warning == "Atlas could not reach the helper to end its background jobs"),
        "{reply}"
    );
}

#[test]
fn the_stop_reply_says_what_was_left() {
    let waited = Duration::from_millis(412);
    let mut ended = job_view(ID, "ended");
    ended["how"] = json!("stopped");
    ended["signal"] = json!("SIGTERM");
    ended["left"] = json!([]);
    assert_eq!(
        stopped(&id(), &ended, waited),
        json!({"ok": true, "id": ID, "state": "ended", "how": "stopped", "signal": "SIGTERM",
            "waitedMs": 412, "left": []})
    );

    ended["signal"] = json!("SIGKILL");
    ended["left"] = json!([{"pid": 4377, "name": "sleep"}]);
    let reply = stopped(&id(), &ended, waited);
    assert_eq!(reply["left"][0]["pid"], 4377);
    assert_eq!(reply["next"], LEFT_RUNNING);

    let mut stopping = job_view(ID, "stopping");
    stopping["remaining"] = json!(2);
    let reply = stopped(&id(), &stopping, Duration::from_secs(10));
    assert_eq!(
        reply,
        json!({"ok": false, "code": "job", "reason": "still-stopping", "id": ID,
            "state": "stopping", "remaining": 2, "waitedMs": 10000, "uncertain": false,
            "error": "The job has not ended yet; Atlas keeps stopping it",
            "next": "atlas job list shows when it has ended."})
    );
}

#[test]
fn stopping_every_job_says_how_many_and_which_are_left() {
    let mut reply = json!({"ok": true});
    Everyone {
        asked: 3,
        left: 1,
        refused: 0,
        error: None,
    }
    .say(&mut reply);
    assert_eq!(reply["jobsStopped"], 2);
    assert_eq!(
        reply["warnings"],
        json!(["1 background job still ran when Atlas stopped waiting"])
    );

    let mut reply = json!({"ok": true});
    Everyone {
        asked: 0,
        left: 0,
        refused: 0,
        error: Some("Connection refused".into()),
    }
    .say(&mut reply);
    assert!(
        reply["warnings"][0]
            .as_str()
            .unwrap()
            .starts_with("Atlas could not stop the helper's background jobs")
    );
}

#[tokio::test]
async fn a_jobs_log_is_read_without_control_and_an_older_helper_says_so() {
    let dir = tempfile::tempdir().unwrap();
    let client = client_in(dir.path());
    let _held = client.controller.acquire(false).unwrap();
    stand_in(&client.config.socket_path, |request| {
        Some(match request["id"].as_str() {
            Some(ID) => json!({"ok": true, "op": "job", "action": "log", "id": ID,
                "data": STANDARD.encode(b"hello \xff\n"), "size": 81234,
                "outputTruncated": true, "logResets": 0}),
            _ => json!({"ok": false, "code": "bad-request", "error": "unknown op 'job'",
                "uncertain": false}),
        })
    });
    let reply = log(&client, &id(), 8).await.unwrap();
    assert_eq!(reply["output"], "hello \u{fffd}\n");
    assert_eq!(reply["size"], 81234);
    assert_eq!(reply["outputTruncated"], true);

    let other = JobId::parse("000000000000").unwrap();
    let error = log(&client, &other, 8).await.unwrap_err();
    assert_eq!(error.code, "daemon");
    assert!(error.error.contains("run atlas restart"));
}

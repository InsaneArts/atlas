use super::*;
use crate::{
    host_session::{Host, HostSession},
    task::Task,
    test_support::{Helper, UID},
};
use serde_json::json;
use std::os::unix::fs::DirBuilderExt;

/// A minimal bundle named `name` in `directory`.
fn bundle_in(directory: &Path, name: &str) -> PathBuf {
    let bundle = directory.join(name);
    fs::create_dir_all(bundle.join("Contents/MacOS")).unwrap();
    fs::write(bundle.join("Contents/Info.plist"), "<plist/>").unwrap();
    bundle
}

#[tokio::test]
async fn an_install_without_a_task_is_refused() {
    let directory = tempfile::tempdir().unwrap();
    let client = crate::test_support::client(directory.path());
    let bundle = bundle_in(directory.path(), "Probe.app");

    let error = install(&client, &bundle).await.unwrap_err();
    assert_eq!((error.code, error.uncertain), (Code::Usage, false));
    assert!(error.error.contains("ATLAS_SESSION"), "{error}");
}

#[test]
fn only_a_real_app_bundle_is_installed() {
    let directory = tempfile::tempdir().unwrap();
    let good = bundle_in(directory.path(), "Probe.app");
    assert_eq!(bundle(&good).unwrap().1, "Probe.app");

    let hidden = bundle_in(directory.path(), ".Probe.app");
    let plain = directory.path().join("Probe");
    fs::DirBuilder::new().mode(0o700).create(&plain).unwrap();
    let empty = directory.path().join("Empty.app");
    fs::create_dir(&empty).unwrap();
    let link = directory.path().join("Link.app");
    std::os::unix::fs::symlink(&good, &link).unwrap();
    for refused in [
        hidden,
        plain,
        empty,
        link,
        directory.path().join("Missing.app"),
    ] {
        let error = bundle(&refused).unwrap_err();
        assert_eq!(error.code, "usage", "{}", refused.display());
    }
}

#[tokio::test]
async fn the_same_app_installed_twice_by_one_task_names_the_earlier_install() {
    let directory = tempfile::tempdir().unwrap();
    let mut client = crate::test_support::client(directory.path());
    let host = HostSession::new(Host::Codex, "twice").unwrap();
    let task = Task::of(host.key(), Some("probe")).unwrap();
    client.host_session = Some(host);
    client.task = Some(task.clone());
    let earlier = Token::new();
    let output = client.output_dir().unwrap().to_owned();
    Claims::in_store(Tasks::at(&output), task)
        .claim(
            &Owned::Install {
                id: earlier.clone(),
                app: "Probe".into(),
            },
            State::Made,
        )
        .unwrap();

    let bundle = bundle_in(directory.path(), "Probe.app");
    let error = install(&client, &bundle).await.unwrap_err();
    assert_eq!(error.code, "input");
    assert_eq!(error.details["installId"], json!(earlier));
}

/// A client of a stand-in helper with a task, both kept in
/// `directory`. The helper is the system's own `nobody`, an account
/// that exists everywhere, so requests can be staged for it.
fn tasked(directory: &Path) -> (Client, Helper, Task) {
    let nobody = nix::unistd::User::from_name("nobody").unwrap().unwrap();
    let (mut client, helper) = Helper::staging(directory, nobody.uid.as_raw());
    let host = HostSession::new(Host::Codex, "installing").unwrap();
    let task = Task::of(host.key(), Some("probe")).unwrap();
    client.host_session = Some(host);
    client.task = Some(task.clone());
    (client, helper, task)
}

/// The helper's answer to `request`: its apps, that it can read the
/// bundle, and `worker` for the environment worker.
fn answer(request: &Value, worker: &Option<Value>) -> Option<Value> {
    if request["op"] == "apps" {
        return Some(json!({"ok": true, "apps": []}));
    }
    if request["argv"][1] == "environment-worker" {
        return worker
            .as_ref()
            .map(|reply| crate::test_support::completed(&reply.to_string()));
    }
    Some(crate::test_support::completed("1\n"))
}

fn claimed(client: &Client, task: &Task) -> Vec<(Owned, State)> {
    Tasks::at(client.output_dir().unwrap())
        .load(task.key())
        .unwrap()
        .claims()
        .collect()
}

#[tokio::test]
async fn an_install_the_helper_made_is_claimed_made_for_its_task() {
    let directory = tempfile::tempdir().unwrap();
    let (client, helper, task) = tasked(directory.path());
    let bundle = bundle_in(directory.path(), "Probe.app");
    let worker = Some(json!({"ok": true, "installed": true, "app": "Probe"}));

    let reply = helper
        .serve(install(&client, &bundle), |request| {
            answer(request, &worker)
        })
        .await
        .unwrap();
    assert_eq!(reply["installed"], true);
    let claims = claimed(&client, &task);
    assert_eq!(claims.len(), 1);
    assert!(matches!(&claims[0], (Owned::Install { app, .. }, State::Made) if app == "Probe"));
    assert!(client.controller.pending().unwrap().is_empty());
}

#[tokio::test]
async fn a_lost_install_reply_keeps_its_record_for_cleanup() {
    let directory = tempfile::tempdir().unwrap();
    let (client, helper, task) = tasked(directory.path());
    let bundle = bundle_in(directory.path(), "Probe.app");

    let error = helper
        .serve(install(&client, &bundle), |request| answer(request, &None))
        .await
        .unwrap_err();
    assert!(error.uncertain, "{error}");
    let claims = claimed(&client, &task);
    assert!(matches!(&claims[0], (Owned::Install { .. }, State::Intent)));
    assert!(!client.controller.pending().unwrap().is_empty());
}

#[tokio::test]
async fn a_refused_install_leaves_nothing_claimed() {
    let directory = tempfile::tempdir().unwrap();
    let (client, helper, task) = tasked(directory.path());
    let bundle = bundle_in(directory.path(), "Probe.app");
    let refusal = json!({"ok": false, "code": "input",
        "error": "The helper already has Applications/Probe.app."});
    let reply = helper
        .serve(install(&client, &bundle), |request| {
            if request["argv"][1] == "environment-worker" {
                let mut refused = refusal.clone();
                refused["refusal"] = json!(
                    request["argv"][2]
                        .as_str()
                        .and_then(|packet| Path::new(packet).parent()?.file_name()?.to_str())
                        .unwrap_or_default()
                );
                return Some(crate::test_support::exited(
                    environment::WORKER_REFUSED,
                    &refused.to_string(),
                ));
            }
            answer(request, &None)
        })
        .await
        .unwrap();
    assert_eq!(
        (&reply["ok"], &reply["code"]),
        (&json!(false), &json!("input"))
    );
    assert!(reply.get("resumeRequired").is_none(), "{reply}");
    let claims = claimed(&client, &task);
    assert!(matches!(&claims[0], (Owned::Install { .. }, State::Gone)));
}

#[tokio::test]
async fn a_refused_app_list_installs_and_claims_nothing() {
    let directory = tempfile::tempdir().unwrap();
    let (mut client, helper) = Helper::client(directory.path(), UID);
    let host = HostSession::new(Host::Codex, "refused").unwrap();
    let task = Task::of(host.key(), None).unwrap();
    client.host_session = Some(host);
    client.task = Some(task.clone());
    let bundle = bundle_in(directory.path(), "Probe.app");

    let error = helper
        .serve(install(&client, &bundle), |_| {
            Some(json!({"ok": false, "code": "busy", "error": "Busy", "uncertain": false}))
        })
        .await
        .unwrap_err();
    assert_eq!((error.code, error.uncertain), (Code::Busy, false));
    let output = client.output_dir().unwrap();
    assert!(Tasks::at(output).load(task.key()).is_none());
}

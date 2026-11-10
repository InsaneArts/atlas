use super::*;
use crate::agent::tests::refused;

#[test]
fn quit_output_names_each_signalled_app_once() {
    assert_eq!(signalled(b"12\n 34 \nnot a pid\n"), [12, 34]);

    let process = |pid, name: &str| Process {
        pid,
        executable: format!("/{pid}"),
        name: name.to_owned(),
        protected: false,
        within: None,
    };

    let processes = [
        process(1, "Notes"),
        process(2, "Notes"),
        process(3, "Safari"),
    ];
    assert_eq!(names_by_quit(&processes, &[1, 2], true), ["Notes"]);
    assert_eq!(names_by_quit(&processes, &[1, 2], false), ["Safari"]);
}

/// TextEdit (812) as the helper lists it.
fn textedit_running() -> Value {
    json!({"ok": true, "apps": [{"pid": 812, "name": "TextEdit", "policy": "regular",
        "executable": "/System/Applications/TextEdit.app/Contents/MacOS/TextEdit"}]})
}

/// A copy root under `directory` holding two unlabelled copies opened
/// with TextEdit, which nothing else uses.
fn two_copies(directory: &Path) -> (std::path::PathBuf, [std::path::PathBuf; 2]) {
    use std::os::unix::fs::DirBuilderExt;

    let root = directory.join("copies");
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(&root)
        .unwrap();
    let copies = ["a", "b"].map(|name| {
        let folder = root.join(name);
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(&folder)
            .unwrap();
        std::fs::write(folder.join("notes.rtf"), "hello").unwrap();
        std::fs::write(folder.join(".opened-with"), "TextEdit").unwrap();
        folder
    });
    (root, copies)
}

/// Whether `request` is the script that quits apps.
fn quitting(request: &Value) -> bool {
    request["op"] == "run" && request["argv"][2] == QUIT
}

#[tokio::test]
async fn a_close_whose_first_list_is_refused_quits_nothing_and_keeps_every_copy() {
    use crate::test_support::Helper;
    use atlas_core::test_support::UID;

    let directory = tempfile::tempdir().unwrap();
    let (client, helper) = Helper::client(directory.path(), UID);
    let (root, copies) = two_copies(directory.path());
    let mut signalled = false;

    let error = helper
        .serve(
            close_in(&client, Closing::App("TextEdit"), &root),
            |request| {
                signalled |= quitting(request);
                Some(refused("busy"))
            },
        )
        .await
        .unwrap_err();

    assert_eq!((error.code, error.uncertain), (Code::Busy, false));
    assert!(!signalled, "nothing is quit without a list");
    assert!(copies.iter().all(|copy| copy.exists()));
}

#[tokio::test]
async fn a_close_whose_list_fails_after_the_signal_keeps_the_copies_and_says_what_was_signalled() {
    use crate::test_support::{Helper, completed};
    use atlas_core::test_support::UID;

    let directory = tempfile::tempdir().unwrap();
    let (client, helper) = Helper::client(directory.path(), UID);
    let (root, copies) = two_copies(directory.path());
    let mut lists = 0;

    let error = helper
        .serve(
            close_in(&client, Closing::App("TextEdit"), &root),
            |request| {
                if quitting(request) {
                    return Some(completed("812\n"));
                }
                lists += 1;
                Some(if lists == 1 {
                    textedit_running()
                } else {
                    refused("apps")
                })
            },
        )
        .await
        .unwrap_err();

    assert_eq!((error.code, error.uncertain), (Code::Apps, false));
    assert_eq!(error.details["signalled"], json!(["TextEdit"]));
    assert!(
        error.error.contains("Their copies were kept"),
        "{}",
        error.error
    );
    assert!(copies.iter().all(|copy| copy.exists()));
}

#[tokio::test]
async fn a_malformed_list_never_reads_as_not_running() {
    use crate::test_support::Helper;
    use atlas_core::test_support::UID;

    let directory = tempfile::tempdir().unwrap();
    let (client, helper) = Helper::client(directory.path(), UID);
    let (root, copies) = two_copies(directory.path());

    let error = helper
        .serve(close_in(&client, Closing::App("TextEdit"), &root), |_| {
            Some(json!({"ok": true, "apps": null}))
        })
        .await
        .unwrap_err();

    assert_eq!((error.code, error.uncertain), (Code::Protocol, false));
    assert!(copies.iter().all(|copy| copy.exists()));
}

#[tokio::test]
async fn an_empty_list_still_means_not_running() {
    use crate::test_support::Helper;
    use atlas_core::test_support::UID;

    let directory = tempfile::tempdir().unwrap();
    let (client, helper) = Helper::client(directory.path(), UID);
    let (root, copies) = two_copies(directory.path());

    let reply = helper
        .serve(close_in(&client, Closing::App("TextEdit"), &root), |_| {
            Some(json!({"ok": true, "apps": []}))
        })
        .await
        .unwrap();

    assert_eq!(reply["reason"], "TextEdit is not running");
    assert_eq!(reply["closed"], false);
    assert_eq!(reply["removed"].as_array().map(Vec::len), Some(2));
    assert!(copies.iter().all(|copy| !copy.exists()));
}

#[tokio::test]
async fn removed_copies_are_settled_in_the_task_that_claimed_them() {
    use crate::{
        host_session::{Host, HostSession},
        task::{Claims, Owned, State, Task, store::Store},
        test_support::Helper,
    };
    use atlas_core::{files::Token, test_support::UID};
    use std::os::unix::fs::DirBuilderExt;

    let directory = tempfile::tempdir().unwrap();
    let (mut client, helper) = Helper::client(directory.path(), UID);
    let session = HostSession::new(Host::Codex, "closing").unwrap();
    let task = Task::of(session.key(), Some("probe")).unwrap();
    client.host_session = Some(session.clone());
    client.task = Some(task.clone());
    let output = client.output_dir().unwrap().to_owned();
    let (root, _) = two_copies(directory.path());
    let token = Token::new();
    let folder = root.join(token.as_str());
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(&folder)
        .unwrap();
    std::fs::write(folder.join("notes.rtf"), "hello").unwrap();
    std::fs::write(folder.join(".opened-with"), "TextEdit").unwrap();
    std::fs::write(folder.join(".session"), format!("{}\n", session.key())).unwrap();
    let copy = Owned::Copy { id: token };
    Claims::in_store(Store::at(&output), task.clone())
        .claim(&copy, State::Made)
        .unwrap();

    let reply = helper
        .serve(close_in(&client, Closing::App("TextEdit"), &root), |_| {
            Some(json!({"ok": true, "apps": []}))
        })
        .await
        .unwrap();

    assert_eq!(reply["removed"].as_array().map(Vec::len), Some(3));
    let record = Store::at(&output).load(task.key()).unwrap();
    assert_eq!(record.claims().collect::<Vec<_>>(), [(copy, State::Gone)]);
}

#[tokio::test]
async fn a_close_keeps_the_apps_identity_after_it_has_quit() {
    use crate::{
        activity_log,
        doing::Doing,
        test_support::{Helper, completed},
    };
    use atlas_core::test_support::UID;

    let directory = tempfile::tempdir().unwrap();
    let (client, helper) = Helper::client(directory.path(), UID);
    let client = client.doing(Doing::new("close").app(Some("TextEdit")));
    let mut signalled = false;
    let reply = helper
        .serve(
            close_in(
                &client,
                Closing::App("TextEdit"),
                &directory.path().join("copies"),
            ),
            |request| {
                Some(if quitting(request) {
                    signalled = true;
                    completed("812\n")
                } else if signalled {
                    json!({"ok": true, "apps": []})
                } else {
                    let mut apps = textedit_running();
                    apps["apps"][0]["bundleId"] = json!("com.apple.TextEdit");
                    apps["apps"][0]["title"] = json!("PRIVATE DOCUMENT TITLE");
                    apps
                })
            },
        )
        .await
        .unwrap();
    assert_eq!(reply["closed"], true);
    let rows = activity_log::recent(client.output_dir().unwrap(), 10);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].bundle_id.as_deref(), Some("com.apple.TextEdit"));
    assert_eq!(rows[0].app.as_deref(), Some("TextEdit"));
    assert_eq!(rows[0].label, None);
    let log =
        std::fs::read_to_string(client.output_dir().unwrap().join(activity_log::FILE)).unwrap();
    assert!(!log.contains("PRIVATE DOCUMENT TITLE"));
}

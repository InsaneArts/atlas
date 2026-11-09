use super::*;
use crate::agent::tests::refused;
use crate::handoff::Copied;
use atlas_core::files::Token;
use std::{ffi::OsString, os::unix::ffi::OsStringExt, path::PathBuf};

#[test]
fn a_failed_launch_keeps_copied_paths_or_explicit_null_for_non_unicode_paths() {
    let finished = Finished::decode(&json!({"timedOut": true}), OUTPUT);
    for invalid in [None, Some("from"), Some("to")] {
        let mut copied = vec![Copied {
            from: PathBuf::from("/original/note.txt"),
            to: PathBuf::from("/copies/note.txt"),
            token: Token::new(),
        }];
        let expected = match invalid {
            None => json!([{"from": "/original/note.txt", "to": "/copies/note.txt"}]),
            Some(field) => {
                let path = PathBuf::from(OsString::from_vec(b"/note-\xff.txt".to_vec()));
                if field == "from" {
                    copied[0].from = path;
                } else {
                    copied[0].to = path;
                }
                Value::Null
            }
        };
        let error = not_opened(&finished, &copied);
        assert_eq!(
            serde_json::to_value(error).unwrap(),
            json!({"ok": false, "code": "wait-failed", "uncertain": false,
                "error": "macOS did not finish opening that in time; it may still be launching: \
                              run atlas wait --app NAME, or look with atlas apps",
                "timedOut": true, "opened": false, "copied": expected})
        );
    }
    assert!(
        serde_json::to_value(not_opened(&finished, &[]))
            .unwrap()
            .get("copied")
            .is_none()
    );
}

#[test]
fn an_app_open_in_another_account_says_so_and_not_that_it_may_be_missing() {
    let said = "_LSOpenURLsWithCompletionHandler() failed for the application \
                /Applications/Tailscale.app with error -10829.\n";
    let finished = Finished::decode(&crate::test_support::exited(1, said), OUTPUT);
    assert_eq!(
        serde_json::to_value(not_opened(&finished, &[])).unwrap(),
        json!({"ok": false, "code": "spawn-failed", "uncertain": false,
            "error": "That app runs in one account at a time, and it is already open in another \
                      account on this Mac.",
            "next": atlas_core::error::advice::OPEN_IN_ANOTHER_ACCOUNT, "opened": false})
    );

    let missing = "Unable to find application named 'with error -10829.'";
    let finished = Finished::decode(&crate::test_support::exited(1, missing), OUTPUT);
    let error = serde_json::to_value(not_opened(&finished, &[])).unwrap();
    assert_eq!(
        error["error"], missing,
        "a name is never taken for the error"
    );
    assert!(error.get("next").is_none());
}

/// Web pages for a browser already running open in a window of their own,
/// so they never land as a tab in another agent's window; anything else
/// opens as it always did.
#[tokio::test]
async fn web_pages_for_a_running_browser_open_in_a_window_of_their_own() {
    use crate::test_support::{Helper, completed};
    use atlas_core::test_support::UID;

    let chrome = json!([{"pid": 7, "name": "Google Chrome", "bundleId": "com.google.Chrome"}]);
    let url = "https://example.com/".to_owned();
    let cases = [
        (
            chrome.clone(),
            vec![url.clone()],
            json!([
                "/usr/bin/open",
                "-n",
                "-a",
                "Google Chrome",
                "--args",
                "--new-window",
                url
            ]),
        ),
        (
            json!([]),
            vec![url.clone()],
            json!(["/usr/bin/open", "-a", "Google Chrome", "--", url]),
        ),
        (
            chrome,
            vec!["notes.txt".to_owned()],
            json!(["/usr/bin/open", "-a", "Google Chrome", "--", "notes.txt"]),
        ),
    ];
    for (apps, targets, expected) in cases {
        let directory = tempfile::tempdir().unwrap();
        let (client, helper) = Helper::client(directory.path(), UID);
        let copies = directory.path().join("copies");
        std::fs::create_dir(&copies).unwrap();
        let mut launched: Vec<Value> = Vec::new();

        helper
            .serve(
                open_in(&client, Some("Google Chrome"), &targets, &[], &copies),
                |request| {
                    if request["op"] == "apps" {
                        return Some(json!({"ok": true, "apps": apps}));
                    }
                    if request["argv"][0] == "/usr/bin/open" {
                        launched.push(request["argv"].clone());
                    }
                    Some(completed(""))
                },
            )
            .await
            .unwrap();

        assert_eq!(launched, [expected], "{apps} {targets:?}");
    }
}

#[test]
fn a_running_app_is_found_by_its_name_or_by_its_bundles_path() {
    let listed = AppList::decode(json!({"ok": true, "apps": [
        {"pid": 7, "name": "Google Chrome", "bundleId": "com.google.Chrome",
         "executable": "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"},
        {"pid": 8, "name": "Google Chrome Helper", "bundleId": "com.google.Chrome.helper",
         "executable": "/Applications/Google Chrome.app/Contents/Frameworks/Helper.app/Contents/MacOS/Helper"},
    ]}))
    .unwrap();
    let bundle = |app: &str| running_app(&listed, app).and_then(AppRow::bundle_id);

    assert_eq!(bundle("google chrome"), Some("com.google.Chrome"));
    assert_eq!(
        bundle("/Applications/Google Chrome.app"),
        Some("com.google.Chrome")
    );
    assert_eq!(bundle("/Applications/Google Chrome Canary.app"), None);
    assert_eq!(
        bundle("/Applications/Google"),
        None,
        "a path it only begins with"
    );
}

#[test]
fn a_launch_refused_for_a_finished_job_names_its_app() {
    let said = "The application /System/Applications/Calculator.app cannot be opened for an \
                unexpected reason, error=Error Domain=RBSRequestErrorDomain Code=5 \"Launch \
                failed.\" UserInfo={NSUnderlyingError=0x7482c10990 {Error \
                Domain=OSLaunchdErrorDomain Code=37 \"Operation already in progress\"}}";
    assert_eq!(
        stale_launch(said),
        Some("/System/Applications/Calculator.app")
    );

    let other = said.replace("Code=37", "Code=3");
    assert_eq!(stale_launch(&other), None, "only error 37");
    assert_eq!(
        stale_launch("Unable to find application named 'Nope'"),
        None
    );
}

#[tokio::test]
async fn an_app_opened_by_name_is_listed_for_the_task_and_nothing_else_is() {
    use crate::{
        host_session::{Host, Key},
        task::{Owned, State, Task, store::Store},
        test_support::{Helper, completed},
    };
    use atlas_core::test_support::UID;

    let directory = tempfile::tempdir().unwrap();
    let (mut client, helper) = Helper::client(directory.path(), UID);
    let task = Task::of(&Key::of(Host::Codex, "opening"), Some("probe")).unwrap();
    client.task = Some(task.clone());
    let copies = directory.path().join("copies");
    std::fs::create_dir(&copies).unwrap();
    let answer = |request: &Value| {
        Some(if request["op"] == "apps" {
            json!({"ok": true, "apps": []})
        } else {
            completed("")
        })
    };

    for (app, targets) in [
        (None, vec!["https://example.com".to_owned()]),
        (Some("TextEdit"), Vec::new()),
    ] {
        let opened = helper
            .serve(open_in(&client, app, &targets, &[], &copies), answer)
            .await
            .unwrap();
        assert_eq!(opened["opened"], true);
    }

    let output = client.output_dir().unwrap().to_owned();
    let claims: Vec<_> = Store::at(&output)
        .load(task.key())
        .unwrap()
        .claims()
        .collect();
    assert_eq!(
        claims,
        [(
            Owned::Launch {
                app: "TextEdit".to_owned()
            },
            State::Made
        )]
    );
}

#[tokio::test]
async fn an_open_whose_list_is_refused_copies_prunes_and_launches_nothing() {
    use crate::test_support::Helper;
    use atlas_core::test_support::UID;

    let directory = tempfile::tempdir().unwrap();
    let (client, helper) = Helper::client(directory.path(), UID);
    let unreadable = directory.path().join("notes.rtf");
    std::fs::write(&unreadable, "hello").unwrap();
    let targets = [unreadable.to_string_lossy().into_owned()];
    let mut ran = Vec::new();

    let error = helper
        .serve(
            open_in(&client, Some("TextEdit"), &targets, &[], directory.path()),
            |request| {
                if request["op"] == "run" {
                    ran.push(request.clone());
                }
                Some(refused("busy"))
            },
        )
        .await
        .unwrap_err();

    assert_eq!((error.code, error.uncertain), (Code::Busy, false));
    assert!(
        ran.is_empty(),
        "nothing was checked, copied or launched: {ran:?}"
    );
}

#[tokio::test]
async fn an_app_launch_services_cannot_find_opens_from_the_tasks_own_install() {
    use crate::{
        host_session::{Host, HostSession},
        task::{Claims, Owned, State, Task, store::Store},
        test_support::{Helper, completed, exited},
    };
    use atlas_core::files::Token;

    let directory = tempfile::tempdir().unwrap();
    let nobody = nix::unistd::User::from_name("nobody").unwrap().unwrap();
    let (mut client, helper) = Helper::staging(directory.path(), nobody.uid.as_raw());
    let session = HostSession::new(Host::Codex, "installed").unwrap();
    let task = Task::of(session.key(), Some("probe")).unwrap();
    client.host_session = Some(session);
    client.task = Some(task.clone());
    let output = client.output_dir().unwrap().to_owned();
    let install = Token::new();
    Claims::in_store(Store::at(&output), task.clone())
        .claim(
            &Owned::Install {
                id: install.clone(),
                app: "Probe".into(),
            },
            State::Made,
        )
        .unwrap();
    let copies = directory.path().join("copies");
    std::fs::create_dir(&copies).unwrap();
    let path = "/Users/helper/Applications/Probe.app";
    let mut launched: Vec<Value> = Vec::new();

    let opened = helper
        .serve(
            open_in(&client, Some("Probe"), &[], &[], &copies),
            |request| {
                if request["op"] == "apps" {
                    return Some(json!({"ok": true, "apps": []}));
                }
                if request["argv"][1] == "environment-worker" {
                    let listed = json!({"ok": true, "installs": [{"installId": install,
                            "phase": "installed", "verified": true, "path": path,
                            "root": {"device": 1, "inode": 2}}]});
                    return Some(completed(&listed.to_string()));
                }
                if request["argv"][0] == "/usr/bin/open" {
                    launched.push(request["argv"].clone());
                    return Some(if request["argv"][2] == "Probe" {
                        exited(1, "Unable to find application named 'Probe'\n")
                    } else {
                        completed("")
                    });
                }
                Some(completed(""))
            },
        )
        .await
        .unwrap();

    assert_eq!(opened["opened"], true, "{opened}");
    assert_eq!(
        launched,
        [
            json!(["/usr/bin/open", "-a", "Probe"]),
            json!(["/usr/bin/open", "-a", path])
        ]
    );
    let claims: Vec<_> = Store::at(&output)
        .load(task.key())
        .unwrap()
        .claims()
        .collect();
    assert!(
        !claims
            .iter()
            .any(|(owned, _)| matches!(owned, Owned::Launch { .. })),
        "the install's own cleanup quits it: {claims:?}"
    );
}

#[tokio::test]
async fn an_app_no_task_installed_is_not_found_by_name_and_nothing_else_is_tried() {
    use crate::test_support::{Helper, completed, exited};
    use atlas_core::test_support::UID;

    let directory = tempfile::tempdir().unwrap();
    let (client, helper) = Helper::client(directory.path(), UID);
    let copies = directory.path().join("copies");
    std::fs::create_dir(&copies).unwrap();
    let mut launched = 0;

    let opened = helper
        .serve(
            open_in(&client, Some("Probe"), &[], &[], &copies),
            |request| {
                if request["op"] == "apps" {
                    return Some(json!({"ok": true, "apps": []}));
                }
                if request["argv"][0] == "/usr/bin/open" {
                    launched += 1;
                    return Some(exited(1, "Unable to find application named 'Probe'\n"));
                }
                Some(completed(""))
            },
        )
        .await
        .unwrap();

    assert_eq!(
        (&opened["opened"], &opened["code"]),
        (&json!(false), &json!("spawn-failed"))
    );
    assert_eq!(launched, 1);
}

#[tokio::test]
async fn an_open_records_the_launched_apps_identity_without_keeping_its_window_title() {
    use crate::{
        activity_log,
        doing::Doing,
        test_support::{Helper, completed},
    };
    use atlas_core::test_support::UID;

    let directory = tempfile::tempdir().unwrap();
    let (client, helper) = Helper::client(directory.path(), UID);
    let client = client.doing(Doing::open(Some("TextEdit"), &[]));
    let mut launched = false;
    let reply = helper
        .serve(
            open_in(
                &client,
                Some("TextEdit"),
                &[],
                &[],
                &directory.path().join("copies"),
            ),
            |request| {
                Some(if request["op"] == "apps" {
                    if launched {
                        json!({"ok": true, "apps": [{"pid": 812, "name": "TextEdit",
                    "bundleId": "com.apple.TextEdit", "title": "PRIVATE DOCUMENT TITLE"}]})
                    } else {
                        json!({"ok": true, "apps": []})
                    }
                } else {
                    launched = true;
                    completed("")
                })
            },
        )
        .await
        .unwrap();
    assert_eq!(reply["opened"], true);
    let rows = activity_log::recent(client.output_dir().unwrap(), 10);
    assert_eq!(
        rows.len(),
        1,
        "the completion replaces the lower-level run steps"
    );
    assert_eq!(rows[0].app.as_deref(), Some("TextEdit"));
    assert_eq!(rows[0].bundle_id.as_deref(), Some("com.apple.TextEdit"));
    assert_eq!(rows[0].label, None);
    let log =
        std::fs::read_to_string(client.output_dir().unwrap().join(activity_log::FILE)).unwrap();
    assert!(!log.contains("PRIVATE DOCUMENT TITLE"));
}

#[tokio::test]
async fn an_open_stays_successful_when_its_identity_read_fails_or_the_session_changes() {
    use crate::{
        activity_log,
        doing::Doing,
        test_support::{Helper, completed},
    };
    use atlas_core::test_support::UID;

    for changed in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let (client, helper) = Helper::client(directory.path(), UID);
        let helper = if changed {
            helper.moving_after(2)
        } else {
            helper
        };
        let client = client.doing(Doing::open(Some("TextEdit"), &[]));
        let mut lists = 0;
        let reply = helper
            .serve(
                open_in(
                    &client,
                    Some("TextEdit"),
                    &[],
                    &[],
                    &directory.path().join("copies"),
                ),
                |request| {
                    Some(if request["op"] == "apps" {
                        lists += 1;
                        if lists == 1 {
                            json!({"ok": true, "apps": []})
                        } else {
                            refused("apps")
                        }
                    } else {
                        completed("")
                    })
                },
            )
            .await
            .unwrap();
        assert_eq!(reply["opened"], true);
        let rows = activity_log::recent(client.output_dir().unwrap(), 10);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].bundle_id, None);
        assert!(rows[0].worked());
        assert_eq!(lists, if changed { 1 } else { 2 });
    }
}

use super::*;
use crate::agent::tests::refused;
use crate::handoff::Copied;
use atlas_core::files::Token;
use std::{ffi::OsString, os::unix::ffi::OsStringExt, path::PathBuf, time::Duration};

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

#[tokio::test]
async fn opening_a_store_or_a_file_url_to_one_is_refused_before_control_is_taken_or_the_helper_asked()
 {
    let directory = tempfile::tempdir().unwrap();
    let client = crate::test_support::client(directory.path());
    let copies = directory.path().join("copies");
    let store = directory.path().join("logins.json");
    std::fs::write(&store, b"{}").unwrap();
    let url = format!("file://{}", directory.path().join(".npm%72c").display());
    let escaped_host = format!(
        "file://%6cocalhost{}",
        directory.path().join(".env").display()
    );
    let numeric_host = format!(
        "file://127.0.0.1{}",
        directory.path().join(".npmrc").display()
    );
    let lone_percent = format!("file://{}", directory.path().join("100%/.env").display());
    for target in [
        store.display().to_string(),
        url,
        escaped_host,
        numeric_host,
        lone_percent,
    ] {
        let error = open_in(&client, None, std::slice::from_ref(&target), &[], &copies)
            .await
            .unwrap_err();
        assert_eq!(error.code, Code::Credentials, "{target}: {error}");
    }
    assert!(!copies.exists(), "old copies were pruned");
    assert!(
        !directory.path().join("state").exists(),
        "control was taken"
    );
}

#[tokio::test]
async fn opening_a_link_named_like_a_store_is_refused_though_it_leads_elsewhere() {
    let directory = tempfile::tempdir().unwrap();
    let client = crate::test_support::client(directory.path());
    let settings = directory.path().join("settings");
    std::fs::write(&settings, b"").unwrap();
    let link = directory.path().join("cookies.sqlite");
    std::os::unix::fs::symlink(&settings, &link).unwrap();
    let error = open_in(
        &client,
        None,
        &[link.display().to_string()],
        &[],
        &directory.path().join("copies"),
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, Code::Credentials, "{error}");
}

#[tokio::test]
async fn opening_a_file_reference_url_is_refused_before_the_helper_is_asked() {
    let directory = tempfile::tempdir().unwrap();
    let client = crate::test_support::client(directory.path());
    let reference = "file:///.file/id=6571367.1234".to_owned();
    let error = open_in(
        &client,
        None,
        std::slice::from_ref(&reference),
        &[],
        &directory.path().join("copies"),
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, Code::Usage, "{error}");
    assert!(
        !directory.path().join("state").exists(),
        "control was taken"
    );
}

#[tokio::test]
async fn opening_a_path_named_like_a_store_that_does_not_exist_is_refused_before_control_is_taken()
{
    let directory = tempfile::tempdir().unwrap();
    let client = crate::test_support::client(directory.path());
    for missing in [
        directory.path().join("missing/.env"),
        directory.path().join("odd://missing/.env"),
        PathBuf::from(".env-missing-here/.npmrc"),
        PathBuf::from(".npmrc"),
    ] {
        let missing = missing.display().to_string();
        let error = open_in(
            &client,
            None,
            std::slice::from_ref(&missing),
            &[],
            &directory.path().join("copies"),
        )
        .await
        .unwrap_err();
        assert_eq!(error.code, Code::Credentials, "{missing}: {error}");
    }
    assert!(
        !directory.path().join("state").exists(),
        "control was taken"
    );
}

#[test]
fn every_file_named_and_every_path_that_exists_is_checked_as_given() {
    let targets = ["odd:/.env", "https://example.com", ".npmrc"].map(str::to_owned);
    let wanted = [
        handoff::Target::Path(PathBuf::from("/real/settings")),
        handoff::Target::Name("https://example.com".into()),
        handoff::Target::Name(".npmrc".into()),
    ];
    let given = given_paths(None, &targets, &wanted);
    assert_eq!(
        given,
        [Path::new("odd:/.env"), Path::new(".npmrc")],
        "a URL is no file"
    );
    let app = [handoff::Target::Name("Safari".into())];
    assert!(
        given_paths(Some("Safari"), &[], &app).is_empty(),
        "an app's name is the helper's"
    );
}

#[tokio::test]
async fn a_persons_launch_is_never_sent_when_their_lease_runs_out_while_the_helper_is_verified() {
    use crate::test_support::{Helper, completed};
    use atlas_control::{Lease, SharedLease};
    use atlas_core::test_support::UID;
    let directory = tempfile::tempdir().unwrap();
    let (client, helper) = Helper::client(directory.path(), UID);
    let lease: SharedLease =
        std::sync::Arc::new(tokio::sync::Mutex::new(Lease::new(Duration::from_secs(10))));
    let token = lease.lock().await.acquire(&client.controller).unwrap();
    let ownership = lease.lock().await.ownership(&token).unwrap();

    // The lease is held still until the helper has been verified for the
    // launch, and runs out then: before the launch is sent.
    let verified = helper.proofs();
    let mut held = lease.clone().lock_owned().await;
    let expiring = async {
        while helper.proofs() == verified {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
        held.expire_for_test();
        drop(held);
    };
    let mut sent = Vec::new();
    let launching = helper.serve(
        async {
            let (refused, ()) = tokio::join!(
                open_for_person(
                    &client,
                    &ownership,
                    (&lease, &token),
                    "Google Chrome",
                    "https://github.com",
                    true,
                ),
                expiring,
            );
            refused
        },
        |request| {
            sent.push(request.clone());
            Some(completed(""))
        },
    );
    let refused = launching.await.unwrap_err();

    assert_eq!(refused.code, Code::LeaseLost);
    assert!(helper.proofs() > verified, "the helper was verified first");
    assert!(sent.is_empty(), "nothing was sent: {sent:?}");
    assert!(client.controller.pending().unwrap().is_empty());
    drop(ownership);
    lease.lock().await.release();
}

#[tokio::test]
async fn a_browsers_own_page_opens_for_the_person_once_its_new_window_shows() {
    use crate::test_support::{Helper, completed};
    use atlas_control::{Lease, SharedLease};
    use atlas_core::test_support::UID;
    let directory = tempfile::tempdir().unwrap();
    let (client, helper) = Helper::client(directory.path(), UID);
    let lease: SharedLease =
        std::sync::Arc::new(tokio::sync::Mutex::new(Lease::new(Duration::from_secs(10))));
    let token = lease.lock().await.acquire(&client.controller).unwrap();
    let ownership = lease.lock().await.ownership(&token).unwrap();
    let page = "chrome://settings/clearBrowserData";

    // Chrome ignores its own pages as an argument: an empty window opens
    // first, then the page goes to it as a URL once the window shows.
    let mut sent = Vec::new();
    let mut opened = 0;
    let mut reads_after = 0;
    let launched = helper
        .serve(
            open_for_person(
                &client,
                &ownership,
                (&lease, &token),
                "Google Chrome",
                page,
                true,
            ),
            |request| {
                if request["op"] == "windows" {
                    if opened > 0 {
                        reads_after += 1;
                    }
                    let shown = usize::from(opened > 0 && reads_after > 1);
                    let windows: Vec<Value> = (0..1 + shown)
                        .map(|number| {
                            json!({"number": number, "pid": 812, "owner": "Google Chrome",
                            "title": null, "layer": 0, "alpha": 1.0,
                            "bounds": {"x": 0, "y": 0, "width": 100, "height": 100}})
                        })
                        .collect();
                    return Some(json!({"ok": true, "starts": {}, "windows": windows}));
                }
                if request["argv"][0] == "/usr/bin/open" {
                    sent.push((request["argv"].clone(), reads_after));
                    opened += 1;
                }
                Some(completed(""))
            },
        )
        .await
        .unwrap();

    assert_eq!(launched["opened"], true);
    assert_eq!(
        sent,
        [
            (
                json!([
                    "/usr/bin/open",
                    "-n",
                    "-a",
                    "Google Chrome",
                    "--args",
                    "--new-window"
                ]),
                0
            ),
            (
                json!(["/usr/bin/open", "-a", "Google Chrome", "--", page]),
                2
            ),
        ],
        "the page went once the second read showed the new window"
    );
    drop(ownership);
    lease.lock().await.release();
}

#[tokio::test]
async fn a_web_page_opens_for_the_person_in_a_new_window_at_once() {
    use crate::test_support::{Helper, completed};
    use atlas_control::{Lease, SharedLease};
    use atlas_core::test_support::UID;
    let directory = tempfile::tempdir().unwrap();
    let (client, helper) = Helper::client(directory.path(), UID);
    let lease: SharedLease =
        std::sync::Arc::new(tokio::sync::Mutex::new(Lease::new(Duration::from_secs(10))));
    let token = lease.lock().await.acquire(&client.controller).unwrap();
    let ownership = lease.lock().await.ownership(&token).unwrap();

    let mut sent = Vec::new();
    helper
        .serve(
            open_for_person(
                &client,
                &ownership,
                (&lease, &token),
                "Google Chrome",
                "https://example.com",
                true,
            ),
            |request| {
                assert_ne!(request["op"], "windows", "no window is waited for");
                if request["argv"][0] == "/usr/bin/open" {
                    sent.push(request["argv"].clone());
                }
                Some(completed(""))
            },
        )
        .await
        .unwrap();

    assert_eq!(
        sent,
        [json!([
            "/usr/bin/open",
            "-n",
            "-a",
            "Google Chrome",
            "--args",
            "--new-window",
            "https://example.com"
        ])]
    );
    drop(ownership);
    lease.lock().await.release();
}

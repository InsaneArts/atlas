use super::*;
use crate::test_support::{Helper, completed, exited};
use atlas_core::test_support::UID;

#[test]
fn only_ready_tools_and_installed_apps_make_the_aggregate_ready() {
    for (state, expected) in [
        (ItemState::Ready, true),
        (ItemState::Installed, true),
        (ItemState::Missing, false),
        (ItemState::Failing, false),
        (ItemState::Unknown, false),
    ] {
        assert_eq!(state.ready(), expected);
        assert_eq!(
            [ItemState::Ready, state, ItemState::Installed]
                .into_iter()
                .all(ItemState::ready),
            expected
        );
    }
}

fn reply(exit: i64) -> Value {
    json!({"exitCode": exit, "timedOut": false, "signal": null})
}

#[test]
fn tools_are_classified_by_a_completed_invocation() {
    assert_eq!(
        tool_status(&reply(0), b"\nmarkdown2pdf 1.6.1\nmore"),
        (ItemState::Ready, "markdown2pdf 1.6.1".into())
    );

    let timed_out = json!({"exitCode": 0, "timedOut": true, "signal": "SIGKILL"});
    assert_eq!(
        tool_status(&timed_out, b"x").0,
        ItemState::Unknown,
        "never ready on a timeout"
    );
    assert_eq!(
        tool_status(&reply(134), b"dyld: Library not loaded: @rpath/libx.dylib").1,
        "A required library is missing in the helper account"
    );
    assert_eq!(
        tool_status(&reply(126), b"Bad CPU type in executable").0,
        ItemState::Failing
    );
    assert_eq!(
        tool_status(&reply(127), b"env: python3: No such file").0,
        ItemState::Failing,
        "present but broken, not missing"
    );
    assert_eq!(
        tool_status(&reply(2), b"usage: tool [-x]").0,
        ItemState::Ready
    );
    assert_eq!(
        tool_status(&reply(1), b"error: no config").0,
        ItemState::Failing
    );
    assert_eq!(first_line(&[b'x'; 500]).len(), 200);
}

#[test]
fn app_facts_accept_only_the_probe_protocol() {
    assert_eq!(
        app_facts("id=com.apple.Preview\nstate=preferences\nstate=container\n"),
        Some((
            "com.apple.Preview".into(),
            vec!["preferences".into(), "container".into()]
        ))
    );
    assert_eq!(
        app_facts("id=com.apple.Chess\n"),
        Some(("com.apple.Chess".into(), vec![]))
    );

    for output in [
        "",
        "state=preferences\n",
        "id=com.apple.x\nnoise\n",
        "id=../evil\n",
        "id=a\nid=b\n",
        "id=com.apple.x\nstate=other\n",
    ] {
        assert_eq!(app_facts(output), None, "{output:?}");
    }
}

#[test]
fn only_the_first_usable_bundle_candidate_is_inspected() {
    let candidates = [
        ("/Applications/A.app", false),
        ("/Users/h/Applications/A.app", true),
        ("/System/Applications/A.app", true),
    ]
    .map(|(path, readable)| {
        let mut access = crate::environment::discovery::Access::default();
        access.directory = true;
        access.readable = readable;
        access.executable = true;
        PathObservation {
            path: path.into(),
            access,
        }
    });
    assert_eq!(
        usable_bundle(&candidates),
        Some("/Users/h/Applications/A.app")
    );
    assert_eq!(usable_bundle(&candidates[..1]), None);
    assert_eq!(usable_bundle(&[]), None);
}

/// A client of a stand-in helper, both kept in `directory`. The helper
/// is the system's own `nobody`, an account that exists everywhere, so
/// requests can be staged for it.
fn helped(directory: &std::path::Path) -> (Client, Helper) {
    let nobody = nix::unistd::User::from_name("nobody").unwrap().unwrap();
    Helper::staging(directory, nobody.uid.as_raw())
}

/// Answer as a helper where every path discovery looks at is a runnable
/// file and a readable folder, and every app is Finder.
fn found(request: &Value) -> Option<Value> {
    let argv = request["argv"].as_array().unwrap();
    let output = if argv[2].as_str().unwrap().contains("plutil") {
        "id=com.apple.finder\n".to_owned()
    } else {
        "-fdrx\n".repeat(argv.len() - 4)
    };

    Some(completed(&output))
}

#[tokio::test]
async fn a_tool_version_probe_still_needs_control() {
    let directory = tempfile::tempdir().unwrap();
    let (client, helper) = helped(directory.path());
    let _held = client.controller.acquire(false).unwrap();
    let mut heard = Vec::new();

    let error = helper
        .serve(readiness(&client, &["cat".into()], &[]), |request| {
            heard.push(request["argv"].clone());
            found(request)
        })
        .await
        .unwrap_err();
    assert_eq!(error.code, "busy");
    assert_eq!(heard.len(), 1, "only discovery reached the helper");
    assert_eq!(heard[0][0], "/bin/sh");
    assert!(client.controller.pending().unwrap().is_empty());
}

/// Without an environment profile a tool's `--version` runs on the
/// helper's PATH, and that run still holds control and stays pending
/// until its reply settles it.
#[tokio::test]
async fn the_path_version_probe_runs_under_control() {
    if !atlas_core::test_support::isolated(
        "readiness::tests::the_path_version_probe_runs_under_control",
    ) {
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let (client, helper) = Helper::client(directory.path(), UID);
    let mut fallbacks = 0;

    // Profile staging has its own account and ACL tests in environment.
    // This test controls its result without starting timed subprocesses.
    let missing = async { Err(Error::new(crate::environment::NO_PROFILE, "No profile")) };
    let (reply, context) = helper
        .serve(
            probe_using(&client, "/Users/helper", "cat", missing),
            |request| {
                let argv = request["argv"].as_array().unwrap();
                assert_eq!(argv[0], "/usr/bin/env");
                fallbacks += 1;
                assert_eq!(argv[2..], [json!("cat"), json!("--version")]);
                assert!(!client.controller.pending().unwrap().is_empty());
                assert_eq!(client.controller.acquire(false).unwrap_err().code, "busy");
                Some(completed("cat 1.0\n"))
            },
        )
        .await
        .unwrap();
    assert_eq!(fallbacks, 1);
    assert_eq!(context, "path");
    assert_eq!(
        tool_status(&reply, &run_output(&reply, 16384).unwrap()).0,
        ItemState::Ready
    );
    assert!(client.controller.pending().unwrap().is_empty());
}

#[tokio::test]
async fn a_profile_probe_success_or_failure_is_returned_without_a_path_fallback() {
    let directory = tempfile::tempdir().unwrap();
    let client = crate::test_support::client(directory.path());
    let expected = completed("cat 1.0\n");
    let (reply, context) = probe_using(&client, "/Users/helper", "cat", async {
        Ok(expected.clone())
    })
    .await
    .unwrap();
    assert_eq!(reply, expected);
    assert_eq!(context, "environment");

    for code in [Code::Permission, Code::Busy, Code::Environment] {
        let error = probe_using(&client, "/Users/helper", "cat", async {
            Err(Error::new(code, "Profile probe failed"))
        })
        .await
        .unwrap_err();
        assert_eq!(error.code, code);
        assert_eq!(error.error, "Profile probe failed");
    }
    assert!(client.controller.pending().unwrap().is_empty());
}

#[tokio::test]
async fn readiness_for_apps_alone_needs_no_control() {
    let directory = tempfile::tempdir().unwrap();
    let (client, helper) = helped(directory.path());
    let _held = client.controller.acquire(false).unwrap();

    let mut probes = 0;
    let report = helper
        .serve(readiness(&client, &[], &["Finder".into()]), |request| {
            probes += 1;
            found(request)
        })
        .await
        .unwrap();
    assert_eq!(
        report,
        json!({
            "ok": true,
            "ready": true,
            "notVerified": ["application launch", "application sign-in"],
            "items": [{
                "kind": "application", "name": "Finder", "status": "installed",
                "bundle": "/Applications/Finder.app", "bundleIdentifier": "com.apple.finder",
                "helperState": "none", "helperStateKinds": [], "signIn": "unknown",
                "detail": "Account data and sign-ins belong to the helper; check the app itself.",
            }],
        })
    );
    assert_eq!(probes, 2, "one discovery and one bundle inspection");
    assert_eq!(helper.requests(), 12);
    assert_eq!(helper.proofs(), 5);
    assert!(client.controller.pending().unwrap().is_empty());
}

#[tokio::test]
async fn a_present_tool_with_a_failed_probe_is_not_ready() {
    let directory = tempfile::tempdir().unwrap();
    let (client, helper) = helped(directory.path());
    let mut probes = 0;
    let report = helper
        .serve(readiness(&client, &["broken".into()], &[]), |request| {
            probes += 1;
            if request["argv"][1] == "environment-worker" {
                Some(exited(127, "env: broken: No such file\n"))
            } else {
                found(request)
            }
        })
        .await
        .unwrap();
    assert_eq!(
        report,
        json!({
            "ok": true, "ready": false, "notVerified": [],
            "items": [{
                "kind": "tool", "name": "broken", "status": "failing",
                "detail": "It could not run: env: broken: No such file",
                "context": "environment", "exitCode": 127,
            }],
        })
    );
    assert_eq!(probes, 2, "presence does not replace the version probe");
    assert!(client.controller.pending().unwrap().is_empty());
}

#[tokio::test]
async fn tool_reports_preserve_version_usage_and_malformed_output_results() {
    for (reply, status, detail, exit, ready) in [
        (completed("tool 1.0\nmore"), "ready", "tool 1.0", 0, true),
        (
            exited(2, "usage: tool [-x]\n"),
            "ready",
            "Runs, though it does not answer --version",
            2,
            true,
        ),
        (
            json!({"ok": true, "exitCode": 0, "timedOut": false, "signal": null,
            "events": [{"event": "output", "data": "invalid base64"}]}),
            "unknown",
            "The --version output was malformed or too long",
            0,
            false,
        ),
        (
            completed(&"x".repeat(16385)),
            "unknown",
            "The --version output was malformed or too long",
            0,
            false,
        ),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let (client, helper) = helped(directory.path());
        let mut probes = 0;
        let report = helper
            .serve(readiness(&client, &["tool".into()], &[]), |request| {
                probes += 1;
                if request["argv"][1] == "environment-worker" {
                    Some(reply.clone())
                } else {
                    found(request)
                }
            })
            .await
            .unwrap();
        assert_eq!(
            report,
            json!({
                "ok": true, "ready": ready, "notVerified": [],
                "items": [{
                    "kind": "tool", "name": "tool", "status": status,
                    "detail": detail, "context": "environment", "exitCode": exit,
                }],
            })
        );
        assert_eq!(probes, 2);
        assert!(client.controller.pending().unwrap().is_empty());
    }
}

#[tokio::test]
async fn missing_selections_keep_their_order_without_execution() {
    let directory = tempfile::tempdir().unwrap();
    let (client, helper) = helped(directory.path());
    let _held = client.controller.acquire(false).unwrap();
    let mut probes = 0;
    let report = helper
        .serve(
            readiness(&client, &["absent".into()], &["Absent".into()]),
            |request| {
                probes += 1;
                let paths = request["argv"].as_array().unwrap().len() - 4;
                Some(completed(&"-\n".repeat(paths)))
            },
        )
        .await
        .unwrap();
    assert_eq!(
        report,
        json!({
            "ok": true, "ready": false,
            "notVerified": ["application launch", "application sign-in"],
            "items": [
                {"kind": "tool", "name": "absent", "status": "missing",
                 "detail": "Not found where the helper looks; copy it in with atlas environment, or install it for every account"},
                {"kind": "application", "name": "Absent", "status": "missing",
                 "detail": "Not found where the helper looks; copy it in with atlas environment, or install it for every account"},
            ],
        })
    );
    assert_eq!(probes, 1);
    assert_eq!(helper.requests(), 9);
    assert!(client.controller.pending().unwrap().is_empty());
}

#[tokio::test]
async fn a_failed_status_after_discovery_leaves_the_helper_home_unknown() {
    let directory = tempfile::tempdir().unwrap();
    let (client, helper) = helped(directory.path());
    let helper = helper.unavailable_after(7);
    let mut probes = 0;
    let error = helper
        .serve(readiness(&client, &["absent".into()], &[]), |request| {
            probes += 1;
            let paths = request["argv"].as_array().unwrap().len() - 4;
            Some(completed(&"-\n".repeat(paths)))
        })
        .await
        .unwrap_err();
    assert_eq!(
        json!(error),
        json!({
            "ok": false, "code": "identity", "uncertain": false,
            "error": "The helper home is unknown",
        })
    );
    assert_eq!(probes, 1);
    assert_eq!(helper.requests(), 8);
    assert_eq!(helper.proofs(), 3);
    assert!(client.controller.pending().unwrap().is_empty());
}

#[tokio::test]
async fn an_unknown_bundle_observation_stays_unknown() {
    for reply in [exited(3, ""), completed("not bundle facts\n"), Value::Null] {
        let directory = tempfile::tempdir().unwrap();
        let (client, helper) = helped(directory.path());
        let mut probes = 0;
        let report = helper
            .serve(readiness(&client, &[], &["Finder".into()]), |request| {
                probes += 1;
                if request["argv"][2].as_str().unwrap().contains("plutil") {
                    Some(reply.clone())
                } else {
                    found(request)
                }
            })
            .await
            .unwrap();
        assert_eq!(
            report,
            json!({
                "ok": true, "ready": false,
                "notVerified": ["application launch", "application sign-in"],
                "items": [{
                    "kind": "application", "name": "Finder", "status": "unknown",
                    "bundle": "/Applications/Finder.app",
                    "detail": "The bundle could not be inspected in the helper",
                }],
            })
        );
        assert_eq!(probes, 2);
        assert_eq!(helper.requests(), 12);
        assert!(client.controller.pending().unwrap().is_empty());
    }
}

#[tokio::test]
async fn an_unknown_app_does_not_hide_another_apps_account_facts() {
    let directory = tempfile::tempdir().unwrap();
    let (client, helper) = helped(directory.path());
    let mut probes = 0;
    let report = helper
        .serve(
            readiness(&client, &[], &["Broken".into(), "Finder".into()]),
            |request| {
                probes += 1;
                if request["argv"][2].as_str().unwrap().contains("plutil") {
                    if request["argv"][4] == "/Applications/Broken.app" {
                        Some(exited(3, ""))
                    } else {
                        Some(completed(
                            "id=com.apple.finder\nstate=preferences\nstate=container\n",
                        ))
                    }
                } else {
                    found(request)
                }
            },
        )
        .await
        .unwrap();
    assert_eq!(
        report,
        json!({
            "ok": true, "ready": false,
            "notVerified": ["application launch", "application sign-in"],
            "items": [
                {"kind": "application", "name": "Broken", "status": "unknown",
                 "bundle": "/Applications/Broken.app",
                 "detail": "The bundle could not be inspected in the helper"},
                {"kind": "application", "name": "Finder", "status": "installed",
                 "bundle": "/Applications/Finder.app", "bundleIdentifier": "com.apple.finder",
                 "helperState": "present", "helperStateKinds": ["preferences", "container"],
                 "signIn": "unknown",
                 "detail": "Account data and sign-ins belong to the helper; check the app itself."},
            ],
        })
    );
    assert_eq!(probes, 3);
    assert_eq!(helper.requests(), 15);
    assert!(client.controller.pending().unwrap().is_empty());
}

#[test]
fn helper_path_prefers_helper_local_tools() {
    let path = helper_path("/Users/h");
    assert!(path.starts_with("/Users/h/.local/bin:/Users/h/.cargo/bin:"));
    assert!(path.ends_with("/usr/sbin:/sbin"));
}

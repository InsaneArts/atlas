use super::*;
use crate::test_support::{Helper, completed};
use atlas_core::test_support::UID;
use std::{
    ffi::OsStr,
    os::unix::{ffi::OsStrExt, fs::PermissionsExt},
};

/// The arguments a probe was given, after its fixed script.
fn arguments(request: &Value) -> &[Value] {
    &request["argv"].as_array().unwrap()[4..]
}

/// Answer a probe as a helper where every path is a runnable file and
/// every variable is defined.
fn found(request: &Value) -> Option<Value> {
    let line = if request["argv"][2] == PATH_PROBE {
        "-frx\n"
    } else {
        "defined\n"
    };

    Some(completed(&line.repeat(arguments(request).len())))
}

/// Discover one tool, one variable and one reference path.
async fn discovered(client: &Client) -> Result<Value> {
    discover(
        client,
        &["cat".into()],
        &[],
        &["HOME".into()],
        &[PathBuf::from("/Users/Shared")],
    )
    .await
}

#[test]
fn a_non_utf8_host_path_panics_during_candidate_assembly() {
    let host = PathBuf::from(OsStr::from_bytes(b"/host/\xff/tool"));
    for kind in [Kind::Tool, Kind::Application] {
        let panic = std::panic::catch_unwind(|| {
            Candidate::new(
                kind,
                "selected",
                Some(host.clone()),
                vec!["/helper/selected".into()],
            );
        })
        .expect_err("a host path with invalid UTF-8 must fail before any discovery probe");
        assert_eq!(
            panic.downcast_ref::<String>().unwrap(),
            "called `Result::unwrap()` on an `Err` value: Error(\"path contains invalid UTF-8 characters\", line: 0, column: 0)"
        );
    }
}

#[tokio::test]
async fn a_short_or_long_probe_result_is_refused_before_association() {
    for (probe, line) in [(Probe::Paths, "-frx\n"), (Probe::Variables, "defined\n")] {
        for count in [1, 3] {
            let directory = tempfile::tempdir().unwrap();
            let (client, helper) = Helper::client(directory.path(), UID);
            let args = ["first".into(), "second".into()];
            let error = helper
                .serve(probe.run(&client, &args), |_| {
                    Some(completed(&line.repeat(count)))
                })
                .await
                .unwrap_err();
            assert_eq!(error.code, "environment");
            assert_eq!(error.error, "Incomplete discovery probe response");
            assert!(!error.uncertain);
            assert!(client.controller.pending().unwrap().is_empty());
        }
    }
}

fn mixed_candidates() -> Vec<Candidate> {
    vec![
        Candidate::new(
            Kind::Tool,
            "first",
            Some("/host/first".into()),
            vec!["/helper/first-a".into(), "/helper/first-b".into()],
        ),
        Candidate::new(Kind::Tool, "second", None, vec!["/helper/second".into()]),
        Candidate::new(
            Kind::Application,
            "third",
            None,
            vec![
                "/helper/third-a".into(),
                "/helper/third-b".into(),
                "/helper/third-c".into(),
            ],
        ),
    ]
}

fn mixed_observations() -> Vec<Access> {
    ["-f", "-fx", "-drx", "-frx", "-drx", "-r", "-rw", "-dwx"]
        .into_iter()
        .map(|flags| Access::from_flags(flags).unwrap())
        .collect()
}

#[test]
fn each_candidate_retains_its_own_access_observations() {
    let mut candidates = mixed_candidates();
    associate(
        &mut candidates,
        &["/reference/first".into(), "/reference/second".into()],
        mixed_observations(),
    )
    .unwrap();
    let reports: Vec<_> = candidates.iter().map(Candidate::report).collect();
    assert_eq!(
        json!(reports),
        json!([
            {"kind": "tool", "name": "first", "host": "/host/first", "executed": false,
             "available": true, "helperCandidates": [
                {"path": "/helper/first-a", "access": {
                    "file": true, "directory": false, "readable": false, "writable": false, "executable": false}},
                {"path": "/helper/first-b", "access": {
                    "file": true, "directory": false, "readable": false, "writable": false, "executable": true}}
             ]},
            {"kind": "tool", "name": "second", "host": null, "executed": false,
             "available": false, "helperCandidates": [
                {"path": "/helper/second", "access": {
                    "file": false, "directory": true, "readable": true, "writable": false, "executable": true}}
             ]},
            {"kind": "application", "name": "third", "host": null, "launched": false,
             "accountData": "separate", "signIn": "not checked", "available": true, "helperCandidates": [
                {"path": "/helper/third-a", "access": {
                    "file": true, "directory": false, "readable": true, "writable": false, "executable": true}},
                {"path": "/helper/third-b", "access": {
                    "file": false, "directory": true, "readable": true, "writable": false, "executable": true}},
                {"path": "/helper/third-c", "access": {
                    "file": false, "directory": false, "readable": true, "writable": false, "executable": false}}
             ]}
        ])
    );
}

#[test]
fn references_retain_their_position_after_mixed_candidates() {
    let references = associate(
        &mut mixed_candidates(),
        &["/reference/first".into(), "/reference/second".into()],
        mixed_observations(),
    )
    .unwrap();
    assert_eq!(
        json!(references),
        json!([
            {"path": "/reference/first", "access": {
                "file": false, "directory": false, "readable": true, "writable": true, "executable": false}},
            {"path": "/reference/second", "access": {
                "file": false, "directory": true, "readable": false, "writable": true, "executable": true}}
        ])
    );
}

#[test]
fn association_refuses_short_and_long_batches_without_changing_candidates() {
    for count in [5, 7] {
        let mut candidates = mixed_candidates();
        let before: Vec<_> = candidates.iter().map(Candidate::report).collect();
        let observed = (0..count)
            .map(|_| Access::from_flags("-frx").unwrap())
            .collect();
        let error = associate(&mut candidates, &[], observed).unwrap_err();
        assert_eq!(error.error, "Incomplete discovery probe response");
        let after: Vec<_> = candidates.iter().map(Candidate::report).collect();
        assert_eq!(before, after);
    }
}

#[tokio::test]
async fn empty_probe_arguments_make_no_request() {
    let directory = tempfile::tempdir().unwrap();
    let client = crate::test_support::client(directory.path());
    for probe in [Probe::Paths, Probe::Variables] {
        assert!(probe.run(&client, &[]).await.unwrap().is_empty());
    }
}

#[tokio::test]
async fn discovery_preserves_complete_reports_with_empty_selection_groups() {
    for groups in [
        (true, true, true, true),
        (false, false, true, false),
        (false, false, false, true),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let (client, helper) = Helper::client(directory.path(), UID);
        let tools = if groups.0 {
            vec!["atlas-discovery-absent-tool-fixture".into()]
        } else {
            vec![]
        };
        let apps = if groups.1 {
            vec!["Atlas Discovery Absent App Fixture.app".into()]
        } else {
            vec![]
        };
        let environment = if groups.2 {
            vec!["HOME".into(), "ATLAS_DISCOVERY_UNDEFINED_FIXTURE".into()]
        } else {
            vec![]
        };
        let path = directory.path().join("missing-reference");
        let references = if groups.3 { vec![path.clone()] } else { vec![] };
        let mut calls = Vec::new();
        let report = helper
            .serve(
                discover(&client, &tools, &apps, &environment, &references),
                |request| {
                    calls.push(request.clone());
                    let output = if request["argv"][2] == PATH_PROBE {
                        "-\n".repeat(arguments(request).len())
                    } else {
                        "defined\nundefined\n".into()
                    };
                    Some(completed(&output))
                },
            )
            .await
            .unwrap();
        let absent = json!({"file": false, "directory": false, "readable": false, "writable": false, "executable": false});
        let mut selections = Vec::new();
        let mut expected_paths = Vec::new();
        if let Some(name) = tools.first() {
            let paths: Vec<_> = [
                "/Users/helper/.local/bin",
                "/Users/helper/.cargo/bin",
                "/opt/homebrew/bin",
                "/usr/local/bin",
                "/usr/bin",
                "/bin",
                "/usr/sbin",
                "/sbin",
            ]
            .into_iter()
            .map(|dir| Path::new(dir).join(name))
            .collect();
            expected_paths.extend(paths.clone());
            selections.push(json!({"kind": "tool", "name": name, "host": null, "executed": false, "available": false,
                "helperCandidates": paths.iter().map(|path| json!({"path": path, "access": absent})).collect::<Vec<_>>()}));
        }
        if let Some(name) = apps.first() {
            let paths: Vec<_> = [
                "/Applications",
                "/System/Applications",
                "/System/Library/CoreServices",
                "/Users/helper/Applications",
            ]
            .into_iter()
            .map(|dir| Path::new(dir).join(name))
            .collect();
            expected_paths.extend(paths.clone());
            selections.push(json!({"kind": "application", "name": name, "host": null, "launched": false,
                "accountData": "separate", "signIn": "not checked", "available": false,
                "helperCandidates": paths.iter().map(|path| json!({"path": path, "access": absent})).collect::<Vec<_>>()}));
        }
        expected_paths.extend_from_slice(&references);
        let variables: Vec<_> = environment.iter().map(|name| json!({"name": name,
            "hostDefined": std::env::var_os(name).is_some(), "helperInheritedDefined": name == "HOME"})).collect();
        let references: Vec<_> = references
            .iter()
            .map(|path| json!({"path": path, "host": absent, "helper": absent}))
            .collect();
        assert_eq!(
            report,
            json!({
                "ok": true, "readOnly": true, "helperUID": UID, "selections": selections,
                "environment": variables, "references": references,
                "scope": "Selected executable/app presence and access only. Shell startup files were not loaded; inherited variables may differ from Terminal. Dependencies, app data and sign-ins require separate live verification. No selected program was executed and no file contents or variable values were read into the report."
            })
        );
        let mut calls = calls.iter();
        if !expected_paths.is_empty() {
            let request = calls.next().unwrap();
            assert_eq!(request["argv"][2], PATH_PROBE);
            assert_eq!(json!(arguments(request)), json!(expected_paths));
        }
        if !environment.is_empty() {
            let request = calls.next().unwrap();
            assert_eq!(request["argv"][2], ENV_PROBE);
            assert_eq!(json!(arguments(request)), json!(environment));
        }
        assert!(calls.next().is_none());
    }
}

#[tokio::test]
async fn discovery_works_while_another_controller_holds_the_lock() {
    let directory = tempfile::tempdir().unwrap();
    let (client, helper) = Helper::client(directory.path(), UID);
    let _held = client.controller.acquire(false).unwrap();

    let report = helper.serve(discovered(&client), found).await.unwrap();
    assert_eq!(report["ok"], true);
    assert_eq!(report["selections"][0]["available"], true);
    assert_eq!(report["environment"][0]["helperInheritedDefined"], true);
    assert!(client.controller.pending().unwrap().is_empty());
}

#[tokio::test]
async fn discovery_works_while_an_earlier_outcome_is_unknown() {
    let directory = tempfile::tempdir().unwrap();
    let (client, helper) = Helper::client(directory.path(), UID);
    let unknown = client.controller.quarantine().unwrap();
    let generation = unknown.generation().to_owned();
    drop(unknown);

    let report = helper.serve(discovered(&client), found).await.unwrap();
    assert_eq!(report["ok"], true);
    assert_eq!(client.controller.pending().unwrap(), [generation]);
}

#[tokio::test]
async fn a_lost_discovery_reply_leaves_no_marker() {
    let directory = tempfile::tempdir().unwrap();
    let (client, helper) = Helper::client(directory.path(), UID);

    let error = helper
        .serve(discovered(&client), |_| None)
        .await
        .unwrap_err();
    assert!(!error.uncertain, "{error:?}");
    assert!(client.controller.pending().unwrap().is_empty());
    assert!(client.controller.acquire(false).is_ok());
}

#[tokio::test]
async fn a_discovery_probe_that_times_out_leaves_no_marker() {
    let directory = tempfile::tempdir().unwrap();
    let (client, helper) = Helper::client(directory.path(), UID);
    let timed_out = json!({"ok": true, "exitCode": null, "timedOut": true, "signal": "SIGKILL"});

    let error = helper
        .serve(discovered(&client), |_| Some(timed_out.clone()))
        .await
        .unwrap_err();
    assert_eq!(error.code, "environment");
    assert!(!error.uncertain);
    assert!(client.controller.pending().unwrap().is_empty());
}

#[tokio::test]
async fn a_helper_that_changes_during_discovery_still_fails_it() {
    let directory = tempfile::tempdir().unwrap();
    let (client, helper) = Helper::client(directory.path(), UID);
    let helper = helper.moving_after(1);

    let error = helper.serve(discovered(&client), found).await.unwrap_err();
    assert_eq!(error.code, "identity");
    assert_eq!(
        error.error,
        "Helper changed during discovery; repeat the inspection"
    );
    assert!(client.controller.pending().unwrap().is_empty());
}

#[tokio::test]
async fn bad_discovery_output_still_fails_it() {
    let directory = tempfile::tempdir().unwrap();
    let (client, helper) = Helper::client(directory.path(), UID);

    let unknown_flag = |request: &Value| Some(completed(&"-q\n".repeat(arguments(request).len())));
    let error = helper
        .serve(discovered(&client), unknown_flag)
        .await
        .unwrap_err();
    assert_eq!(error.code, "environment");
    assert_eq!(error.error, "Invalid discovery probe response");

    let short = |request: &Value| Some(completed(&"-frx\n".repeat(arguments(request).len() - 1)));
    let error = helper.serve(discovered(&client), short).await.unwrap_err();
    assert_eq!(error.code, "environment");
    assert_eq!(error.error, "Incomplete discovery probe response");
}

#[tokio::test]
async fn a_helper_on_screen_refuses_discovery_with_its_own_code() {
    let directory = tempfile::tempdir().unwrap();
    let (client, helper) = Helper::client(directory.path(), UID);
    let on_screen = json!({"ok": false, "code": "on-console",
        "error": "The helper is on screen", "uncertain": false});

    let error = helper
        .serve(discovered(&client), |_| Some(on_screen.clone()))
        .await
        .unwrap_err();
    assert_eq!(error.code, "on-console");
    assert!(!error.uncertain);
}

#[tokio::test]
async fn discovery_writes_no_activity_row() {
    let directory = tempfile::tempdir().unwrap();
    let (client, helper) = Helper::client(directory.path(), UID);

    helper.serve(discovered(&client), found).await.unwrap();
    assert!(
        !client
            .config
            .output_dir
            .join(crate::activity_log::FILE)
            .exists()
    );
}

#[tokio::test]
async fn discovery_runs_only_its_two_fixed_scripts() {
    let directory = tempfile::tempdir().unwrap();
    let (client, helper) = Helper::client(directory.path(), UID);
    let mut heard = Vec::new();

    helper
        .serve(discovered(&client), |request| {
            heard.push(request.clone());
            found(request)
        })
        .await
        .unwrap();

    let [paths, variables] = heard.as_slice() else {
        panic!("discovery runs one path probe and one variable probe: {heard:?}");
    };
    for (request, script) in [(paths, PATH_PROBE), (variables, ENV_PROBE)] {
        let fixed = &request["argv"].as_array().unwrap()[..4];
        assert_eq!(
            fixed,
            [json!("/bin/sh"), json!("-c"), json!(script), json!("sh")]
        );
    }

    let paths = arguments(paths);
    assert!(
        paths[..paths.len() - 1]
            .iter()
            .all(|path| { path.as_str().is_some_and(|path| path.ends_with("/cat")) })
    );
    assert_eq!(paths.last().unwrap(), "/Users/Shared");
    assert_eq!(arguments(variables), [json!("HOME")]);
}

#[test]
fn inspection_does_not_execute_a_selected_program() {
    let directory = tempfile::tempdir().unwrap();
    let tool = directory.path().join("tool with spaces");
    fs::write(&tool, b"#!/bin/sh\nexit 127\n").unwrap();
    fs::set_permissions(&tool, fs::Permissions::from_mode(0o700)).unwrap();
    let result = access(&tool);
    assert!(result.file && result.executable);
    fs::set_permissions(&tool, fs::Permissions::from_mode(0o600)).unwrap();
    assert!(!access(&tool).executable);
}

#[test]
fn selections_and_probe_results_are_bounded() {
    assert!(name_problem("Google Chrome").is_none());
    assert!(name_problem("../other").is_some());
    assert!(name_problem("tool\nname").is_some());
    assert!(!variable("KEY=value"));
    assert!(!variable("-PATH"));
    assert!(variable("OLLAMA_MODELS"));
    assert!(Access::from_flags("-frx").unwrap().executable);
    assert!(Access::from_flags("unexpected content").is_err());
}

#[test]
fn a_reference_requires_read_access_and_directory_search_before_optional_writes() {
    for (flags, readable, writable) in [
        ("-", false, false),
        ("-rwx", false, false),
        ("-fr", true, false),
        ("-frw", true, true),
        ("-dr", false, false),
        ("-drx", true, false),
        ("-drwx", true, true),
        ("-fdrw", false, false),
    ] {
        let access = Access::from_flags(flags).unwrap();
        assert_eq!(access.permits_reference(false), readable, "{flags}");
        assert_eq!(access.permits_reference(true), writable, "{flags}");
    }
}

/// A refused selection names the item at fault and what was expected.
#[test]
fn a_refused_selection_names_the_item_at_fault() {
    let names = |items: &[&str]| items.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    let paths = |items: &[&str]| items.iter().map(PathBuf::from).collect::<Vec<_>>();
    let said = |tools: &[&str], apps: &[&str], variables: &[&str], references: &[&str]| {
        selection(
            &names(tools),
            &names(apps),
            &names(variables),
            &paths(references),
        )
        .unwrap_err()
        .error
    };

    assert_eq!(
        said(&[], &[], &[], &[]),
        "Select at least one tool, app, variable or reference path"
    );
    assert_eq!(
        said(&["/bin/sh"], &[], &[], &[]),
        "A tool is named by its bare command name such as cat; this one is a path: \"/bin/sh\""
    );
    assert_eq!(
        said(&[""], &[], &[], &[]),
        "A tool is named by its bare command name such as cat; this one is empty: \"\""
    );
    assert_eq!(
        said(&["sh"], &["Sa\nfari"], &[], &[]),
        "An app is named by its name such as Safari; this one contains a control character: \"Sa\\nfari\""
    );
    assert_eq!(
        said(&["sh"], &["/Applications/Safari.app"], &[], &[]),
        "An app is named by its name such as Safari; this one is a path: \"/Applications/Safari.app\""
    );
    assert_eq!(
        said(&[], &[], &["1PATH"], &[]),
        "A variable name is letters, digits and underscores, not starting with a digit: 1PATH"
    );
    assert_eq!(
        said(&[], &[], &[], &["/tmp/../etc"]),
        "A reference path is absolute, without ..: /tmp/../etc"
    );

    let many: Vec<String> = (0..17).map(|i| format!("tool{i}")).collect();
    assert!(
        selection(&many, &[], &[], &paths(&[]))
            .unwrap_err()
            .error
            .starts_with("Select at most 16 tools")
    );
    assert!(selection(&names(&["cat"]), &[], &[], &paths(&["/usr/bin"])).is_ok());
}

use super::*;
use atlas_core::error::Code;

#[test]
fn replacing_a_job_keeps_its_executor_claim_after_the_caller_dies() {
    if !atlas_core::test_support::isolated(
        "setup::job::tests::replacing_a_job_keeps_its_executor_claim_after_the_caller_dies",
    ) {
        return;
    }
    for mode in ["retire", "install", "restore"] {
        crate::test_support::interrupted_executor(
            "setup::job::tests::a_job_installation_test_caller",
            mode,
        );
    }
}

#[tokio::test]
async fn helper_installation_and_restarts_defer_before_dispatching_active_work() {
    use crate::host_session::{Host, HostSession, store::Store};

    let directory = tempfile::tempdir().unwrap();
    let client = crate::test_support::client(directory.path());
    let host = HostSession::new(Host::Named, "worker").unwrap();
    Store::at(client.output_dir().unwrap())
        .update(
            &host,
            atlas_core::time::now_ms(),
            &client.controller,
            |_| {},
        )
        .unwrap();
    let helper = atlas_core::account::Account {
        uid: 7001,
        name: "helper".into(),
        home: directory.path().join("home"),
    };
    let layout = Layout::new(helper.uid, helper.home.clone());
    let agent = Agent {
        label: LABEL.into(),
        program: directory.path().join("atlas"),
        uid: helper.uid,
        controller_uid: nix::unistd::getuid().as_raw(),
        socket: layout.socket(),
    };
    let checked = async {
        let error =
            prepare_and_install(&client, "controller", agent.clone(), &layout.logs(), |_| {
                panic!("dispatched setup while active")
            })
            .await
            .unwrap_err();
        assert_eq!(error.error, "worker is active in the helper.");
        let error = replace_job(&client, &helper, agent, &layout, |_| {
            panic!("replaced a job while active")
        })
        .await
        .unwrap_err();
        assert_eq!(error.code, Code::Busy);
        assert_eq!(restart_helper(&client).await.unwrap_err().code, Code::Busy);
    };
    tokio::time::timeout(Duration::from_secs(1), checked)
        .await
        .unwrap();
    assert!(client.controller.pending().unwrap().is_empty());
}

#[test]
fn preparing_and_installing_a_helper_excludes_teardown_after_the_caller_dies() {
    if !atlas_core::test_support::isolated(
        "setup::job::tests::preparing_and_installing_a_helper_excludes_teardown_after_the_caller_dies",
    ) {
        return;
    }
    for mode in ["prepare", "fresh-install"] {
        crate::test_support::interrupted_executor(
            "setup::job::tests::a_job_installation_test_caller",
            mode,
        );
    }
}

/// A subprocess entry point; all effects stay in its explicit temporary directory.
#[tokio::test]
async fn a_job_installation_test_caller() {
    let Some(directory) = atlas_core::test_support::directory() else {
        return;
    };
    let client = crate::test_support::client(&directory);
    let helper = atlas_core::account::Account {
        uid: 7001,
        name: "helper".into(),
        home: directory.join("home"),
    };
    let layout = Layout::new(helper.uid, helper.home.clone());
    let agent = Agent {
        label: LABEL.into(),
        program: directory.join("atlas"),
        uid: helper.uid,
        controller_uid: nix::unistd::getuid().as_raw(),
        socket: layout.socket(),
    };
    let mode = std::env::var("ATLAS_MUTATION_TEST_MODE").unwrap();
    let run = |request: &Request| {
        let pause = matches!(
            (mode.as_str(), request),
            ("retire", Request::Retire { .. })
                | ("install" | "fresh-install", Request::Install { .. })
                | ("restore", Request::Restore { .. })
                | ("prepare", Request::Prepare { .. })
        );
        if pause {
            crate::test_support::pause_executor(&directory, crate::test_support::WORKER);
        }
        if mode == "restore" && matches!(request, Request::Install { .. }) {
            return Err(setup::account::problem("installation was refused"));
        }
        Ok(json!({"ok": true, "jobs": {LABEL: {"plistSha256": "b".repeat(64)}}}))
    };
    if matches!(mode.as_str(), "prepare" | "fresh-install") {
        let _ = prepare_and_install(&client, "controller", agent, &layout.logs(), run).await;
    } else {
        let _ = replace_job(&client, &helper, agent, &layout, run).await;
    }
}

#[test]
fn installation_acknowledgements_settle_only_their_own_setup_mutations() {
    if !atlas_core::test_support::isolated(
        "setup::job::tests::installation_acknowledgements_settle_only_their_own_setup_mutations",
    ) {
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let client = crate::test_support::client(directory.path());
    let controller = &client.controller;
    let other = controller.quarantine().unwrap();
    drop(other);
    setup_transaction(controller, || Ok(())).unwrap();
    assert_eq!(controller.pending().unwrap().len(), 1);
    let outcome: Result<()> = setup_transaction(controller, || {
        Err(setup::account::problem("reply lost").uncertain())
    });
    assert!(outcome.is_err());
    assert_eq!(controller.pending().unwrap().len(), 2);
}

use setup::executor::setup_transaction;

/// An earlier job is retired only when its definition differs, and
/// restored when the new one cannot go in.
#[test]
fn an_earlier_job_is_set_aside_and_restored_on_failure() {
    let agent = Agent {
        label: LABEL.to_owned(),
        program: "/Applications/Atlas.app/Contents/MacOS/atlas".into(),
        uid: 503,
        controller_uid: 501,
        socket: "/Users/atlas/.atlas/helper/503.sock".into(),
    };
    let logs = Path::new("/Users/atlas/.atlas/logs");
    let ours = agent.plist_sha256(logs);

    let attempt_with = |earlier: Option<&str>, install_fails: bool, restore_fails: bool| {
        let actions = std::cell::RefCell::new(Vec::new());
        let run = |request: &Request| -> Result<Value> {
            let action = serde_json::to_value(request).unwrap()["action"].clone();
            actions
                .borrow_mut()
                .push(action.as_str().unwrap().to_owned());

            match request {
                Request::Inspect { .. } => {
                    Ok(json!({"ok": true, "jobs": {LABEL: {"plistSha256": earlier}}}))
                }
                Request::Install { .. } if install_fails => Err(setup::account::problem("refused")),
                Request::Restore { .. } if restore_fails => {
                    Err(setup::account::problem("no backup"))
                }
                _ => Ok(json!({"ok": true})),
            }
        };
        let outcome = install_job(run, agent.clone(), logs);
        (outcome.map_err(|error| error.error), actions.into_inner())
    };
    let attempt = |earlier: Option<&str>, install_fails: bool| {
        let (outcome, actions) = attempt_with(earlier, install_fails, false);
        (outcome.is_ok(), actions)
    };

    assert_eq!(
        attempt(None, false),
        (true, vec!["inspect".into(), "install".into()])
    );
    assert_eq!(
        attempt(Some(&ours), false),
        (true, vec!["inspect".into(), "install".into()])
    );

    let other = "0".repeat(64);
    assert_eq!(
        attempt(Some(&other), false),
        (
            true,
            vec!["inspect".into(), "retire".into(), "install".into()]
        )
    );
    assert_eq!(
        attempt(Some(&other), true),
        (
            false,
            vec![
                "inspect".into(),
                "retire".into(),
                "install".into(),
                "retire".into(),
                "restore".into()
            ]
        )
    );

    // A job that cannot be put back is said, not dropped.
    let (outcome, _) = attempt_with(Some(&other), true, true);
    let error = outcome.unwrap_err();
    assert!(error.starts_with("refused;"), "{error}");
    assert!(
        error.ends_with("could not be put back: no backup"),
        "{error}"
    );
}

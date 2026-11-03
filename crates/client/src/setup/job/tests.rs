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
    let paused = mode.clone();
    let run = move |request: &Request| {
        let pause = matches!(
            (paused.as_str(), request),
            ("retire", Request::Retire { .. })
                | ("install" | "fresh-install", Request::Install { .. })
                | ("restore", Request::Restore { .. })
                | ("prepare", Request::Prepare { .. })
        );
        if pause {
            crate::test_support::pause_executor(&directory, crate::test_support::WORKER);
        }
        if paused == "restore" && matches!(request, Request::Install { .. }) {
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

/// The helper job a test installs for a helper at home in `directory`,
/// with the layout it lives in.
fn test_agent(directory: &Path) -> (atlas_core::account::Account, Agent, Layout) {
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
    (helper, agent, layout)
}

/// A setup run in the helper that finds an earlier job and installs the
/// new one when `installs`, or else is refused and puts the earlier one
/// back, noting each action in `actions` and holding the first `phase`
/// until `released` says go on, once `entered` was told it began.
fn held_at(
    phase: &'static str,
    installs: bool,
    actions: &std::sync::Arc<std::sync::Mutex<Vec<String>>>,
    entered: std::sync::mpsc::Sender<()>,
    released: std::sync::mpsc::Receiver<()>,
) -> impl Fn(&Request) -> Result<Value> + Send + 'static {
    let actions = std::sync::Arc::clone(actions);
    let held = std::sync::atomic::AtomicBool::new(false);
    move |request| {
        let action = serde_json::to_value(request).unwrap()["action"]
            .as_str()
            .unwrap()
            .to_owned();
        actions.lock().unwrap().push(action.clone());
        if action == phase && !held.swap(true, std::sync::atomic::Ordering::Relaxed) {
            let _ = entered.send(());
            let _ = released.recv();
        }
        match request {
            Request::Inspect { .. } => {
                Ok(json!({"ok": true, "jobs": {LABEL: {"plistSha256": "0".repeat(64)}}}))
            }
            Request::Install { .. } if !installs => {
                Err(setup::account::problem("installation was refused"))
            }
            _ => Ok(json!({"ok": true})),
        }
    }
}

/// Whether another controller is refused because this one holds control.
fn excluded(controller: &atlas_control::Controller) -> bool {
    controller.acquire(false).err().map(|error| error.code) == Some(Code::Busy)
}

const ROLLED_BACK: [&str; 5] = ["inspect", "retire", "install", "retire", "restore"];

/// The client of a helper whose daemon is a stand-in at its socket, and
/// that client's controller. Active work is not checked for.
fn answered_client(directory: &Path) -> (Client, atlas_control::Controller) {
    let mut client = crate::test_support::client(directory);
    client.allow_disrupt = true;
    client.transport.peer_uid = nix::unistd::getuid().as_raw();
    std::fs::create_dir_all(client.transport.path.parent().unwrap()).unwrap();
    let controller = client.controller.clone();
    (client, controller)
}

/// A daemon listening at `socket`, bound before anything asks it.
fn listening(socket: &Path) -> tokio::net::UnixListener {
    tokio::net::UnixListener::bind(socket).unwrap()
}

/// A daemon that tells `asked` of each request, and answers it with the
/// next of `answers` once the test gives one.
async fn stand_in(
    listener: tokio::net::UnixListener,
    asked: std::sync::mpsc::Sender<()>,
    mut answers: tokio::sync::mpsc::UnboundedReceiver<Value>,
) {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    loop {
        let (peer, _) = listener.accept().await.unwrap();
        let mut peer = BufReader::new(peer);
        let mut line = String::new();
        peer.read_line(&mut line).await.unwrap();
        asked.send(()).unwrap();
        let reply = answers.recv().await.unwrap();
        peer.get_mut()
            .write_all(format!("{reply}\n").as_bytes())
            .await
            .unwrap();
    }
}

/// The stand-in's answer when daemon process `pid` runs.
fn daemon(pid: u32) -> Value {
    json!({"ok": true, "user": {"uid": 1}, "daemon": {"pid": pid}})
}

/// Whether another controller, once nobody holds control, is refused until
/// recovery settles an unknown outcome.
fn needs_recovery(controller: &atlas_control::Controller) -> bool {
    controller
        .clone()
        .waiting(crate::test_support::WATCHDOG)
        .acquire(false)
        .err()
        .map(|error| error.code)
        == Some(Code::Uncertain)
}

#[test]
fn a_blocked_helper_installation_leaves_the_runtime_responsive_and_retains_exclusion_through_rollback()
 {
    use crate::test_support::{Requester, WATCHDOG};
    for phase in ["prepare", "inspect", "retire", "install", "restore"] {
        let directory = tempfile::tempdir().unwrap();
        let client = crate::test_support::client(directory.path());
        let controller = client.controller.clone();
        let (_, agent, layout) = test_agent(directory.path());
        let actions = std::sync::Arc::default();
        let (entered, entry) = std::sync::mpsc::channel();
        let (release, released) = std::sync::mpsc::channel();
        let run = held_at(phase, false, &actions, entered, released);
        let requester = Requester::start(move || async move {
            prepare_and_install(&client, "controller", agent, &layout.logs(), run).await
        });

        entry.recv_timeout(WATCHDOG).unwrap();
        let responsive = requester.responsive();
        if !responsive {
            let _ = release.send(());
        }
        assert!(responsive, "{phase}: the runtime waited for setup");
        assert!(excluded(&controller), "{phase}: another controller got in");
        assert_eq!(controller.pending().unwrap().len(), 1, "{phase}");

        release.send(()).unwrap();
        let error = requester.outcome().unwrap_err();
        assert_eq!(error.error, "installation was refused", "{phase}");
        let mut done = vec!["prepare"];
        done.extend(ROLLED_BACK);
        assert_eq!(*actions.lock().unwrap(), done, "{phase}");
        let (_next, unknown) = controller.acquire_for_cleanup().unwrap();
        assert!(unknown, "{phase}: an unacknowledged setup stays pending");
    }
}

#[test]
fn a_cancelled_helper_replacement_remains_owned_until_its_executor_and_readiness_checks_finish() {
    use crate::test_support::{Requester, WATCHDOG};
    for phase in ["install", "restore"] {
        let directory = tempfile::tempdir().unwrap();
        let client = crate::test_support::client(directory.path());
        let controller = client.controller.clone();
        let (helper, agent, layout) = test_agent(directory.path());
        let actions = std::sync::Arc::default();
        let (entered, entry) = std::sync::mpsc::channel();
        let (release, released) = std::sync::mpsc::channel();
        let run = held_at(phase, false, &actions, entered, released);
        let requester = Requester::start(move || async move {
            replace_job(&client, &helper, agent, &layout, run).await
        });

        entry.recv_timeout(WATCHDOG).unwrap();
        let responsive = requester.responsive();
        if !responsive {
            let _ = release.send(());
        }
        assert!(
            responsive,
            "{phase}: the runtime waited for the replacement"
        );
        requester.abandon();
        assert!(
            excluded(&controller),
            "{phase}: control ended with the request"
        );
        assert_eq!(controller.pending().unwrap().len(), 1, "{phase}");
        assert!(
            controller.unresolved().unwrap().is_empty(),
            "{phase}: the claim ended with the request"
        );

        release.send(()).unwrap();
        let (_next, unknown) = controller
            .clone()
            .waiting(WATCHDOG)
            .acquire_for_cleanup()
            .unwrap();
        assert_eq!(*actions.lock().unwrap(), ROLLED_BACK, "{phase}");
        assert!(unknown, "{phase}: an unverified replacement stays pending");
        assert_eq!(controller.unresolved().unwrap().len(), 1, "{phase}");
    }
}

#[test]
fn only_a_new_daemon_answering_clears_a_replacement() {
    use crate::test_support::{Requester, WATCHDOG};
    let directory = tempfile::tempdir().unwrap();
    let (client, controller) = answered_client(directory.path());
    drop(controller.quarantine().unwrap());
    let other = controller.pending().unwrap();
    let (helper, agent, layout) = test_agent(directory.path());
    let socket = client.transport.path.clone();
    let (asked, asks) = std::sync::mpsc::channel();
    let (answer, answers) = tokio::sync::mpsc::unbounded_channel();
    let requester = Requester::start(move || async move {
        let listener = listening(&socket);
        let replaced = replace_job(&client, &helper, agent, &layout, |_| {
            Ok(json!({"ok": true}))
        });
        tokio::select! {
            replaced = replaced => replaced,
            () = stand_in(listener, asked, answers) => unreachable!(),
        }
    });

    asks.recv_timeout(WATCHDOG).unwrap();
    answer.send(daemon(100)).unwrap();
    for unready in [json!({"ok": false, "error": "not yet"}), daemon(100)] {
        asks.recv_timeout(WATCHDOG).unwrap();
        assert_eq!(controller.pending().unwrap().len(), 2);
        assert!(excluded(&controller), "control ended before readiness");
        answer.send(unready).unwrap();
    }
    asks.recv_timeout(WATCHDOG).unwrap();
    answer.send(daemon(200)).unwrap();

    requester.outcome().unwrap();
    assert_eq!(controller.pending().unwrap(), other);
}

#[test]
fn a_helper_installation_is_settled_only_for_a_requester_that_hears_it_succeed() {
    use crate::test_support::{Requester, WATCHDOG};
    for abandoned in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let client = crate::test_support::client(directory.path());
        let controller = client.controller.clone();
        let (_, agent, layout) = test_agent(directory.path());
        let actions = std::sync::Arc::default();
        let (entered, entry) = std::sync::mpsc::channel();
        let (release, released) = std::sync::mpsc::channel();
        let run = held_at("install", true, &actions, entered, released);
        let requester = Requester::start(move || async move {
            prepare_and_install(&client, "controller", agent, &layout.logs(), run).await
        });

        entry.recv_timeout(WATCHDOG).unwrap();
        let responsive = requester.responsive();
        if !responsive {
            let _ = release.send(());
        }
        assert!(responsive, "{abandoned}: the runtime waited for setup");
        assert!(
            excluded(&controller),
            "{abandoned}: another controller got in"
        );
        if !abandoned {
            release.send(()).unwrap();
            requester.outcome().unwrap();
            assert!(controller.pending().unwrap().is_empty());
            assert!(controller.acquire(false).is_ok());
            continue;
        }

        requester.abandon();
        assert!(excluded(&controller), "control ended with the request");
        assert!(controller.unresolved().unwrap().is_empty(), "claim let go");
        release.send(()).unwrap();
        assert!(
            needs_recovery(&controller),
            "an installation nobody heard of was settled"
        );
        assert_eq!(
            *actions.lock().unwrap(),
            ["prepare", "inspect", "retire", "install"]
        );
        assert_eq!(controller.unresolved().unwrap().len(), 1);
    }
}

#[test]
fn a_replacement_abandoned_while_installing_or_awaiting_its_daemon_stays_pending() {
    use crate::test_support::{Requester, WATCHDOG};
    for during in ["installation", "readiness"] {
        let directory = tempfile::tempdir().unwrap();
        let (client, controller) = answered_client(directory.path());
        let (helper, agent, layout) = test_agent(directory.path());
        let actions = std::sync::Arc::default();
        let (entered, entry) = std::sync::mpsc::channel();
        let (release, released) = std::sync::mpsc::channel();
        let held = if during == "installation" {
            "install"
        } else {
            "none"
        };
        let run = held_at(held, true, &actions, entered, released);
        let socket = client.transport.path.clone();
        let (asked, asks) = std::sync::mpsc::channel();
        let (answer, answers) = tokio::sync::mpsc::unbounded_channel();
        let requester = Requester::start(move || async move {
            let listener = listening(&socket);
            tokio::select! {
                replaced = replace_job(&client, &helper, agent, &layout, run) => replaced,
                () = stand_in(listener, asked, answers) => unreachable!(),
            }
        });

        asks.recv_timeout(WATCHDOG).unwrap();
        answer.send(daemon(100)).unwrap();
        if during == "installation" {
            entry.recv_timeout(WATCHDOG).unwrap();
        } else {
            // The first look for the new daemon, left unanswered.
            asks.recv_timeout(WATCHDOG).unwrap();
        }
        let responsive = requester.responsive();
        if !responsive {
            let _ = release.send(());
        }
        assert!(responsive, "{during}: the runtime waited");
        assert!(excluded(&controller), "{during}: another controller got in");
        assert_eq!(controller.pending().unwrap().len(), 1, "{during}");
        assert!(controller.unresolved().unwrap().is_empty(), "{during}");

        requester.abandon();
        if during == "installation" {
            assert!(excluded(&controller), "control ended with the request");
            assert!(controller.unresolved().unwrap().is_empty(), "claim let go");
            release.send(()).unwrap();
        }
        assert!(
            needs_recovery(&controller),
            "{during}: a replacement no new daemon confirmed was settled"
        );
        assert_eq!(
            *actions.lock().unwrap(),
            ["inspect", "retire", "install"],
            "{during}"
        );
        assert_eq!(controller.unresolved().unwrap().len(), 1, "{during}");
    }
}

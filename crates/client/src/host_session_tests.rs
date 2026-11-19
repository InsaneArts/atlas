use super::{liveness::Snapshot, recovery::owns_every_unknown, *};
use atlas_control::Cause;
use atlas_core::{
    process::{self, Instance},
    test_support::{
        self,
        host::{claim, denied, peer_resolved, process, resolve_peer},
    },
    time::now_ms,
};
use serde_json::json;

#[test]
fn a_session_owns_the_unknown_only_when_every_one_is_its_own_and_none_runs() {
    let (own, other) = (Key::of(Host::Pi, "own"), Key::of(Host::Pi, "other"));
    let cause = |session: Option<&Key>, running: bool| Cause {
        generation: atlas_core::files::token(),
        running,
        command: false,
        session: session.cloned(),
    };

    assert!(!owns_every_unknown(&[], &own), "nothing is unknown");
    assert!(owns_every_unknown(&[cause(Some(&own), false)], &own));
    assert!(!owns_every_unknown(&[cause(Some(&own), true)], &own));
    assert!(!owns_every_unknown(
        &[cause(Some(&own), false), cause(Some(&other), false)],
        &own
    ));
    assert!(!owns_every_unknown(
        &[cause(Some(&own), false), cause(None, false)],
        &own
    ));
}

#[test]
fn a_running_command_is_nobodys_unknown_outcome_and_recovery_waits_for_it() {
    let (own, other) = (Key::of(Host::Pi, "own"), Key::of(Host::Pi, "other"));
    let unknown = Cause {
        generation: atlas_core::files::token(),
        running: false,
        command: false,
        session: Some(own.clone()),
    };
    let command = Cause {
        generation: atlas_core::files::token(),
        running: true,
        command: true,
        session: Some(other),
    };
    let of = Recovery::of;
    let beside = [unknown.clone(), command.clone()];

    assert!(owns_every_unknown(&beside, &own));
    assert_eq!(
        of(&beside, Asker::Agent(Some(&own))),
        Some(Recovery::OwnOnceCommandEnds)
    );
    assert_eq!(
        of(&beside, Asker::Person),
        Some(Recovery::PersonOnceCommandEnds)
    );
    assert_eq!(
        of(&[unknown], Asker::Agent(Some(&own))),
        Some(Recovery::Own)
    );
    assert_eq!(
        of(std::slice::from_ref(&command), Asker::Person),
        None,
        "work under way"
    );
    assert!(!owns_every_unknown(&[command], &own), "nothing of its own");
    for step in [
        Recovery::OwnOnceCommandEnds.step(),
        Recovery::PersonOnceCommandEnds.step(),
    ] {
        assert!(step.contains("once the running command ends"), "{step}");
    }
    assert!(Recovery::OwnOnceCommandEnds.own());
}

#[test]
fn recovery_says_to_restart_only_when_every_unknown_is_the_askers_own() {
    let (own, other) = (Key::of(Host::Pi, "own"), Key::of(Host::Pi, "other"));
    let cause = |session: Option<&Key>| Cause {
        generation: atlas_core::files::token(),
        running: false,
        command: false,
        session: session.cloned(),
    };
    let asker = Asker::Agent(Some(&own));
    assert_eq!(Recovery::of(&[], asker), None, "only waiting");
    for (causes, yours) in [
        (vec![cause(Some(&own))], true),
        (vec![cause(Some(&own)), cause(Some(&other))], false),
        (vec![cause(Some(&own)), cause(None)], false),
        (vec![cause(Some(&other))], false),
    ] {
        let recovery = Recovery::of(&causes, asker).unwrap();
        assert_eq!(recovery.own(), yours, "{recovery:?}");
        assert_eq!(
            recovery.step().contains("atlas restart"),
            yours,
            "{}",
            recovery.step()
        );
    }
}

#[test]
fn only_the_session_that_claimed_every_unknown_change_recovers_it_itself() {
    let (own, other) = (Key::of(Host::Pi, "own"), Key::of(Host::Pi, "other"));
    let cause = |session: Option<&Key>, running: bool| Cause {
        generation: atlas_core::files::token(),
        running,
        command: false,
        session: session.cloned(),
    };
    let of = Recovery::of;

    assert_eq!(
        of(&[], Asker::Agent(Some(&own))),
        None,
        "nothing is pending"
    );
    assert_eq!(
        of(&[cause(Some(&own), true)], Asker::Person),
        None,
        "a change still being carried out is work in progress"
    );
    let mine = [cause(Some(&own), false)];
    assert_eq!(of(&mine, Asker::Agent(Some(&own))), Some(Recovery::Own));
    assert_eq!(
        of(&mine, Asker::Agent(Some(&other))),
        Some(Recovery::Others)
    );
    assert_eq!(of(&mine, Asker::Agent(None)), Some(Recovery::Others));
    assert_eq!(of(&mine, Asker::Person), Some(Recovery::Person));
    for shared in [
        [cause(Some(&own), false), cause(Some(&other), false)],
        [cause(Some(&own), false), cause(None, false)],
        [cause(Some(&own), false), cause(Some(&own), true)],
    ] {
        assert_eq!(
            of(&shared, Asker::Agent(Some(&own))),
            Some(Recovery::Others)
        );
    }

    assert!(Recovery::Own.step().contains("atlas restart"));
    assert!(Recovery::Person.step().contains("atlas restart"));
    let others = Recovery::Others.step();
    assert!(
        !others.contains("atlas "),
        "no command for another's action"
    );
    assert!(others.contains("tell the person"), "{others}");
}

#[tokio::test]
async fn a_daemon_proven_exit_allows_resume_when_local_process_reads_are_denied() {
    let previous = Instance::new(1, [1, 0]);
    assert_eq!(previous.state(), process::State::Unknown);
    let claims = atlas_core::host::verification::claims(&json!([{
        "host": "codex", "id": "thread", "hint": null, "previous": previous
    }]))
    .unwrap();
    let host = resolve_peer(&claims, Some(90), test_support::UID, process).unwrap();
    let session = HostSession::verify_with(Some(denied()), &claims, async {
        Ok(json!({"ok": true, "hostSession": host}))
    })
    .await
    .unwrap();
    let old = denied().run_by(Some(previous));
    let mut record = record::Record::new(&old, 1, None);
    record.ended = Some(2);
    let (changed, reads) =
        process::measure_states(|| record.touched(&session, session.ancestor(), 3));
    assert!(changed);
    assert_eq!(reads, 0, "attachment uses the daemon's exact exit proof");
    assert_eq!(record.host_process, session.process());
    assert_eq!(record.ended, None);
}

#[tokio::test]
async fn a_live_stopped_or_unknown_previous_host_cannot_be_replaced() {
    let previous = Instance::new(1, [1, 0]);
    let mut claims = claim();
    claims[0].previous = Some(previous);
    for state in [
        process::State::Running,
        process::State::Stopped,
        process::State::Unknown,
    ] {
        let host = test_support::host::peer_resolved(
            &claims,
            Some(process(90).unwrap().instance),
            test_support::UID,
            |pid| {
                if pid != previous.pid {
                    return Ok(process(pid));
                }
                if state == process::State::Unknown {
                    return Err(std::io::Error::from_raw_os_error(libc::EPERM).into());
                }
                let mut info = process(1).unwrap();
                info.instance = previous;
                info.stopped = state == process::State::Stopped;
                Ok(Some(info))
            },
        )
        .unwrap();
        assert_eq!(host.previous, None);
        let session = HostSession::verify_with(Some(denied()), &claims, async {
            Ok(json!({"ok": true, "hostSession": host}))
        })
        .await
        .unwrap();
        let old = denied().run_by(Some(previous));
        let mut record = record::Record::new(&old, 1, None);
        record.ended = Some(2);
        assert!(!record.touched(&session, session.ancestor(), 3));
        assert_eq!(record.host_process, Some(previous));
        assert_eq!(record.ended, Some(2));
    }
}

#[tokio::test]
async fn a_denied_cli_walk_attaches_an_apps_only_record_from_the_verified_socket_peer() {
    let local = denied();
    assert!(local.process().is_none());
    let host = resolve_peer(&claim(), Some(90), test_support::UID, process).unwrap();
    let session = HostSession::verify_with(Some(local), &claim(), async {
        Ok(json!({"ok": true, "hostSession": host}))
    })
    .await
    .unwrap();
    assert_eq!(session.process(), Some(process(80).unwrap().instance));
    assert_eq!(
        session.ancestor(),
        session.process(),
        "a denied local reread must not erase daemon proof"
    );
    assert_eq!(
        session.folder(),
        None,
        "an unreadable host must not borrow the CLI folder"
    );
    let directory = tempfile::tempdir().unwrap();
    let client = crate::test_support::client(directory.path());
    store::touch(&client, &session).unwrap();
    let record = store::Store::at(client.output_dir().unwrap())
        .read(session.key())
        .unwrap();
    assert_eq!(record.host_process, session.process());
    assert!(record.counts.is_empty());
    assert!(record.copies.is_empty() && record.plans.is_empty() && record.screenshots.is_empty());
    let now = now_ms();
    let unknown = Snapshot::sample(session.process(), |_| process::State::Unknown)
        .lifetime(&record, now, false);
    assert!(unknown.ended.is_none());
    let exited =
        Snapshot::sample(session.process(), |_| process::State::Gone).lifetime(&record, now, false);
    assert!(exited.ended.is_some());
    assert!(!exited.live && !exited.active);
}

#[tokio::test]
async fn a_vms_host_is_verified_by_this_process_and_a_failed_check_keeps_the_local_session() {
    let claims = claim();
    let verified = verified_here(None, claims.clone(), |claims| {
        json!({"ok": true, "hostSession": peer_resolved(
            &claims,
            Some(Instance::new(90, [100, 7])),
            test_support::UID,
            |pid| Ok(process(pid)),
        )})
    })
    .await
    .unwrap();
    let local = denied();
    let kept = verified_here(Some(local.clone()), claims, |claims| {
        json!({"ok": true, "hostSession": peer_resolved(
            &claims,
            Some(Instance::new(90, [100, 7])),
            test_support::UID,
            |_| Ok(None),
        )})
    })
    .await;

    assert_eq!(verified.host(), Host::Codex);
    assert_eq!(verified.process(), Some(Instance::new(80, [100, 7])));
    assert_eq!(kept, Some(local), "unverified, as it was");
}

/// A keeper that is trusted whoever listens.
struct Trusting;

impl crate::link::Trust for Trusting {
    fn trusts(
        &self,
        _: &atlas_core::desktop::DesktopId,
        _: &std::path::Path,
        _: &std::os::unix::net::UnixStream,
    ) -> atlas_core::error::Result<()> {
        Ok(())
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_vms_host_is_verified_without_a_word_to_its_guest() {
    let directory = tempfile::Builder::new().tempdir_in("/tmp").unwrap();
    let socket = directory.path().join("keeper.sock");
    let listener = std::os::unix::net::UnixListener::bind(&socket).unwrap();
    listener.set_nonblocking(true).unwrap();
    let mut client = crate::test_support::vm_client(directory.path(), None);
    client.link = crate::link::Link::Keeper(crate::link::Keeper::at(
        std::sync::Arc::new(client.selected.clone()),
        atlas_core::desktop::DesktopId::parse(crate::test_support::VM_ID).unwrap(),
        socket,
        std::time::Duration::from_secs(5),
        std::sync::Arc::new(Trusting),
    ));

    let _ = from_env_verified(&client).await;

    assert!(
        listener.accept().is_err(),
        "nothing was asked of the keeper or the guest"
    );
}

#[test]
fn a_recovery_step_on_a_vm_says_atlas_settles_it_and_recovers_only_that_vm_never_the_menu_bar() {
    use crate::overview::Called;
    let every = [
        Recovery::Own,
        Recovery::OwnOnceCommandEnds,
        Recovery::Others,
        Recovery::Person,
        Recovery::PersonOnceCommandEnds,
    ];

    for recovery in every {
        let step = recovery.step_for(Called::Vm("work"));

        assert_eq!(recovery.step_for(Called::Helper), recovery.step());
        assert_eq!(
            step,
            "An action on work may not have finished; Atlas settles it on its own once work \
             answers. If it stays, run atlas doctor --desktop work or atlas restart --desktop \
             work."
        );
        assert!(!step.to_lowercase().contains("helper"), "{step}");
        assert!(!step.contains("menu bar"), "{step}");
        assert_eq!(
            step.matches("atlas restart").count(),
            step.matches("atlas restart --desktop work").count(),
            "{step}"
        );
    }
}

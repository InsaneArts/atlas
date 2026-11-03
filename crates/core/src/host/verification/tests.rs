use super::*;
use crate::{
    protocol,
    test_support::{
        self,
        host::{claim, denied, process, resolve_peer},
    },
};

#[tokio::test]
async fn a_locally_verified_replacement_still_asks_for_the_previous_hosts_exit() {
    let mut claims = claim();
    claims[0].previous = Some(Instance::new(60, [1, 0]));
    let mut local = denied();
    local.process = Some(process(80).unwrap().instance);
    local.verified = true;
    let host = resolve_peer(&claims, Some(90), test_support::UID, process).unwrap();
    let session = HostSession::verify_with(Some(local), &claims, async {
        Ok(json!({"ok": true, "hostSession": host}))
    })
    .await
    .unwrap();
    assert_eq!(session.replaces, claims[0].previous);
}

#[test]
fn a_denied_daemon_walk_or_missing_peer_pid_grants_no_identity() {
    assert!(
        super::resolve_peer(
            &claim(),
            Some(Instance::new(90, [100, 7])),
            test_support::UID,
            |_| Err(std::io::Error::from_raw_os_error(libc::EPERM).into())
        )
        .is_none()
    );
    assert!(resolve_peer(&claim(), None, test_support::UID, process).is_none());
    for denied in [90, 80, 70] {
        assert!(
            resolve_peer(&claim(), Some(90), test_support::UID, |pid| {
                if pid == denied { None } else { process(pid) }
            })
            .is_none()
        );
    }
}

#[test]
fn a_foreign_host_or_socket_peer_grants_no_identity() {
    assert!(resolve_peer(&claim(), Some(90), test_support::UID + 1, process).is_none());
    assert!(
        resolve_peer(&claim(), Some(90), test_support::UID, |pid| {
            let mut info = process(pid)?;
            if pid == 80 {
                info.uid = 0;
            }
            Some(info)
        })
        .is_none()
    );
}

#[test]
fn a_peer_or_host_reused_during_verification_grants_no_identity() {
    for changed in [90, 80] {
        let calls = std::cell::Cell::new(0);
        assert!(
            resolve_peer(&claim(), Some(90), test_support::UID, |pid| {
                let mut info = process(pid)?;
                if pid == changed {
                    calls.set(calls.get() + 1);
                    if calls.get() > 1 {
                        info.instance.start[1] += 1;
                    }
                }
                Some(info)
            })
            .is_none()
        );
    }
}

#[test]
fn the_daemon_keeps_nested_codex_hosts_ambiguous() {
    assert!(
        resolve_peer(&claim(), Some(90), test_support::UID, |pid| {
            let mut info = process(pid)?;
            if pid == 70 {
                info.uid = test_support::UID;
                info.name = "codex".into();
            }
            Some(info)
        })
        .is_none()
    );
}

#[test]
fn a_daemon_walk_has_the_same_ancestor_limit_as_a_local_walk() {
    let calls = std::cell::Cell::new(0);
    assert!(
        resolve_peer(&claim(), Some(90), test_support::UID, |pid| {
            calls.set(calls.get() + 1);
            let mut info = process(80).unwrap();
            info.instance.pid = pid;
            info.ppid = pid + 1;
            Some(info)
        })
        .is_none()
    );
    assert_eq!(calls.get(), ANCESTORS + 1);
}

#[tokio::test(start_paused = true)]
async fn a_blocked_or_older_daemon_preserves_unknown_metadata_and_returns_within_the_bound() {
    let local = denied();
    assert_eq!(
        HostSession::verify_with(Some(local.clone()), &claim(), async {
            Err(std::io::Error::from_raw_os_error(libc::EPERM).into())
        })
        .await,
        Some(local.clone())
    );
    for response in [
        json!({"ok": false, "code": "bad_request"}),
        json!({"ok": true, "hostSession": null}),
    ] {
        assert_eq!(
            HostSession::verify_with(Some(local.clone()), &claim(), async { Ok(response) }).await,
            Some(local.clone())
        );
    }
    let began = tokio::time::Instant::now();
    assert_eq!(
        HostSession::verify_with(Some(local.clone()), &claim(), std::future::pending()).await,
        Some(local)
    );
    assert_eq!(began.elapsed(), Duration::from_millis(250));
}

#[test]
fn an_unmatched_daemon_reply_cannot_attach_a_different_conversation() {
    let local = denied();
    let mut host = resolve_peer(&claim(), Some(90), test_support::UID, process).unwrap();
    host.id = HostId::parse("other").unwrap();
    assert_eq!(
        HostSession::accept_verified(Some(local.clone()), &claim(), &json!(host)),
        Some(local)
    );
}

#[test]
fn host_claims_are_bounded_and_validated_before_the_daemon_reads_processes() {
    let valid = json!({"op": "host-session", "claims": claim()});
    assert!(matches!(
        protocol::parse(&serde_json::to_vec(&valid).unwrap()),
        Ok(protocol::Request::HostSession(_))
    ));
    for invalid in [
        json!([]),
        json!([claim()[0].clone(), claim()[0].clone()]),
        json!([{"host": "codex", "id": "../escape", "hint": null}]),
        json!([{"host": "named", "id": "thread", "hint": null}]),
        json!([{"host": "pi", "id": "thread", "hint": -1}]),
    ] {
        assert!(
            protocol::parse(
                &serde_json::to_vec(&json!({"op": "host-session", "claims": invalid})).unwrap()
            )
            .is_err()
        );
    }
}

#[tokio::test]
async fn the_kernel_reports_the_socket_peers_pid_and_account() {
    let (socket, _other) = tokio::net::UnixStream::pair().unwrap();
    let peer = socket.peer_cred().unwrap();
    assert_eq!(peer.uid(), nix::unistd::geteuid().as_raw());
    assert_eq!(peer.pid(), Some(nix::unistd::getpid().as_raw()));
    assert_eq!(
        process::info(peer.pid().unwrap())
            .unwrap()
            .unwrap()
            .instance,
        Instance::current().unwrap()
    );
}

#[tokio::test]
async fn a_locally_verified_host_never_waits_for_daemon_verification() {
    let mut local = denied();
    local.process = Some(process(80).unwrap().instance);
    local.verified = true;
    let result = HostSession::verify_with(Some(local.clone()), &claim(), async {
        panic!("a verified local identity needs no daemon call")
    })
    .await;
    assert_eq!(result, Some(local));
}

#[test]
fn a_foreign_ancestor_does_not_hide_a_same_account_host_from_the_daemon() {
    let claims = claim();
    let facts = |pid| process(pid).filter(|info| info.uid == test_support::UID);
    let local = HostSession::resolve_for(
        |name| claims.iter().find_map(|claim| claim.env(name)),
        || ancestry::ancestors_from(80, ANCESTORS, facts),
        facts,
        test_support::UID,
    )
    .unwrap();
    assert!(
        local.process().is_none(),
        "the same-account walk stops before reaching launchd"
    );
    assert!(
        resolve_peer(&claims, Some(90), test_support::UID, process)
            .unwrap()
            .process
            == process(80).unwrap().instance
    );
}

#[test]
fn the_daemon_verifies_each_supported_host_with_the_same_ancestry_rule() {
    for (host, command, hint) in [
        (Host::Claude, "claude", Some(80)),
        (Host::Codex, "codex", None),
        (Host::Opencode, "node", Some(80)),
        (Host::Pi, "node", Some(80)),
    ] {
        let claims = vec![HostClaim {
            host,
            id: HostId::parse("thread").unwrap(),
            hint,
            previous: None,
        }];
        let verified = resolve_peer(&claims, Some(90), test_support::UID, |pid| {
            let mut info = process(pid)?;
            if pid == 80 {
                info.name = command.into();
            }
            Some(info)
        })
        .unwrap();
        assert_eq!(verified.host, host);
        assert_eq!(verified.process, process(80).unwrap().instance);
    }
}

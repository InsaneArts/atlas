use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_agents_request_for_the_persons_control_is_refused_before_the_vm_is_asked() {
    let dir = tempfile::Builder::new().tempdir_in("/tmp").unwrap();
    let (agent, keeper) = crate::test_support::vm_through_keeper(dir.path());
    let owner = Token::new();
    let proof = PersonProof {
        epoch: Epoch {
            uid: 502,
            capture_pid: 123,
            input_pid: 456,
            security_session_id: 42,
            display_id: 7,
            display: atlas_core::session::Display {
                width: 1440.0,
                height: 900.0,
                scale: 2.0,
            },
        },
        generation: crate::test_support::GENERATION,
    };
    let lease = std::sync::Arc::new(tokio::sync::Mutex::new(atlas_control::Lease::new(
        std::time::Duration::from_secs(10),
    )));
    let click = serde_json::json!([{"type": "click", "x": 10, "y": 10}]);

    let refused = [
        agent.person_proof_on_vm().await.unwrap_err(),
        agent
            .person_acquire_on_vm(owner.as_str(), &proof, "capability")
            .await
            .unwrap_err(),
        agent
            .person_input_on_vm(
                (owner.as_str(), proof.generation),
                (&click, &proof.epoch, Instant::now()),
                &lease,
                "capability",
            )
            .await
            .unwrap_err(),
        agent
            .person_renew_on_vm(owner.as_str(), proof.generation, "capability")
            .await
            .unwrap_err(),
        agent
            .person_open_on_vm(
                (&lease, owner.as_str()),
                proof.generation,
                ("Google Chrome", "https://github.com"),
                "capability",
            )
            .await
            .unwrap_err(),
    ];

    for refused in refused {
        assert_eq!(refused.code, Code::Denied, "{refused:?}");
        assert_eq!(
            refused.error,
            "Only the person takes control of work, from Atlas's own window."
        );
    }
    assert!(keeper.asked.lock().unwrap().is_empty(), "nothing asked");
    assert!(agent.controller.pending().unwrap().is_empty());
}

#[tokio::test]
async fn second_logins_control_never_takes_the_vms_way() {
    let dir = tempfile::tempdir().unwrap();
    let mut client = crate::test_support::client_in(dir.path());
    client.actor = Actor::Viewer;

    let refused = client.person_proof_on_vm().await.unwrap_err();

    assert_eq!(refused.error, client.unreachable().error);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_lease_released_while_its_page_connects_cancels_the_page_unsent() {
    let dir = tempfile::Builder::new().tempdir_in("/tmp").unwrap();
    let (mut client, keeper) = crate::test_support::vm_through_keeper(dir.path());
    client.actor = Actor::Viewer;
    let lease = std::sync::Arc::new(tokio::sync::Mutex::new(atlas_control::Lease::new(
        std::time::Duration::from_secs(10),
    )));
    let owner = lease
        .lock()
        .await
        .acquire_beside(&client.controller, atlas_control::Beside::Nothing)
        .unwrap();
    let released = lease.clone();
    *keeper.connecting.lock().unwrap() = Some(Box::new(move || {
        released.blocking_lock().release();
    }));

    let refused = client
        .person_open_on_vm(
            (&lease, &owner),
            crate::test_support::GENERATION,
            ("Google Chrome", "https://github.com"),
            "capability",
        )
        .await
        .unwrap_err();

    assert_eq!(refused.code, Code::LeaseLost, "{refused:?}");
    assert!(!refused.uncertain, "{refused:?}");
    assert!(
        keeper.asked_for("change").is_empty(),
        "nothing was sent: {:?}",
        keeper.asked.lock().unwrap()
    );
    assert!(client.controller.pending().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_lost_page_reply_stays_pending_until_the_guest_answers_it() {
    let dir = tempfile::Builder::new().tempdir_in("/tmp").unwrap();
    let (mut client, keeper) = crate::test_support::vm_through_keeper(dir.path());
    client.actor = Actor::Viewer;
    keeper.input_answers.lock().unwrap().push((
        "person-open",
        serde_json::json!({"ok": false, "code": "transport", "uncertain": true, "sent": true,
            "error": "The change's lane to the guest ended before its final reply."}),
    ));
    let lease = std::sync::Arc::new(tokio::sync::Mutex::new(atlas_control::Lease::new(
        std::time::Duration::from_secs(10),
    )));
    let owner = lease
        .lock()
        .await
        .acquire_beside(&client.controller, atlas_control::Beside::Nothing)
        .unwrap();

    let lost = client
        .person_open_on_vm(
            (&lease, &owner),
            crate::test_support::GENERATION,
            ("Google Chrome", "https://github.com"),
            "capability",
        )
        .await
        .unwrap_err();

    assert!(lost.uncertain, "{lost:?}");
    assert!(!lost.error.contains("helper"), "{lost:?}");
    let asked = keeper.asked_for("change");
    assert_eq!(asked.len(), 1);
    assert_eq!(asked[0]["lane"], "person");
    let pending = client.controller.pending().unwrap();
    assert_eq!(pending, [asked[0]["change"].as_str().unwrap()]);
}

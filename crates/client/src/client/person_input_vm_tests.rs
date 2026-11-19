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

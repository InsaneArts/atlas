use super::*;
use atlas_core::session::key::PeerKey;
use serde_json::json;

/// The guest's key in these tests.
fn guest() -> PeerKey {
    PeerKey::from_bytes([1; 32])
}

/// The station's key in these tests.
fn station() -> PeerKey {
    PeerKey::from_bytes([2; 32])
}

fn paired() -> Paired {
    Paired {
        station: station().fingerprint(),
        guest: guest(),
        uid: 502,
        boot: "boot-0".to_owned(),
        change: atlas_core::files::Token::new(),
    }
}

fn vm() -> Desktop {
    Desktop::from(atlas_core::vm::Vm {
        id: atlas_core::desktop::DesktopId::parse(crate::test_support::VM_ID).unwrap(),
    })
}

/// The facts of the guest's own screen.
fn screen() -> Value {
    json!({"ok": true, "uid": 502, "hasGraphicAccess": true, "parentPID": 123,
        "onConsole": true, "securitySessionID": 42, "displayID": 7, "width": 1440.0,
        "height": 900.0, "scale": 2.0})
}

/// A keeper's whole observation, for `challenge`, of the paired guest on
/// its own screen in a VM Atlas VM runs with its window closed.
fn answer(challenge: &str) -> Value {
    json!({
        "ok": true, "op": "observe", "generation": 7,
        "peer": guest(), "station": station(),
        "status": {
            "hello": {"ok": true, "op": "hello", "user": {"uid": 502},
                "session": {"managerName": "Aqua", "onConsole": true},
                "display": {"width": 1440.0, "height": 900.0, "scale": 2.0},
                "permissions": {"accessibility": true, "postEvents": true,
                    "screenCapture": true},
                "machine": {"mode": "machine", "boot": "boot-1", "virtual": true},
                "daemon": {"pid": 123, "version": "1.0.0", "build": "b1"},
                "remoteLogin": "off", "account": {"admin": false},
                "agents": {"admitted": true, "held": null},
                "input": {"standing": "ready", "held": null}},
            "inputService": {"ok": true, "protocolVersion": 1, "pid": 456,
                "version": "1.0.0", "build": "b1", "ready": true, "accessibility": true,
                "session": screen()},
            "sessionProof": {"ok": true, "challenge": challenge, "proof": screen()},
        },
        "runner": {"ok": true, "power": "running", "confirmed": true, "window": "closed"},
    })
}

/// What `answer` proves, asked for `challenge`, held to `paired`.
fn proved(answer: &Value, challenge: &str, paired: &Paired) -> Result<MachineSession> {
    Observed::decode(answer, challenge)?.prove(&vm(), paired, None)
}

/// A change made to an observation.
type Change = fn(&mut Value);

#[test]
fn a_whole_observation_proves_the_paired_vm_with_its_input() {
    let session = proved(&answer("c1"), "c1", &paired()).unwrap();

    assert!(session.capture);
    assert!(session.input);
    assert_eq!(
        (session.epoch.capture_pid, session.epoch.input_pid),
        (123, 456)
    );
    assert_eq!(session.epoch.display.scale, 2.0);
    let observed = Observed::decode(&answer("c1"), "c1").unwrap();
    assert_eq!(observed.generation, 7);
    assert_eq!(observed.status["sessionProof"]["parentPID"], 123);
}

#[test]
fn each_mismatch_is_a_refusal_of_its_own() {
    let refusals: [(&str, Change, Code); 17] = [
        (
            "another account",
            |it| it["status"]["hello"]["user"]["uid"] = json!(503),
            Code::Identity,
        ),
        (
            "another guest key",
            |it| it["peer"] = json!(PeerKey::from_bytes([3; 32])),
            Code::Identity,
        ),
        (
            "another station",
            |it| it["station"] = json!(PeerKey::from_bytes([4; 32])),
            Code::Identity,
        ),
        (
            "not machine mode",
            |it| it["status"]["hello"]["machine"]["mode"] = json!("user"),
            Code::Identity,
        ),
        (
            "not virtual",
            |it| it["status"]["hello"]["machine"]["virtual"] = json!(false),
            Code::Identity,
        ),
        (
            "virtual not said",
            |it| it["status"]["hello"]["machine"]["virtual"] = json!(null),
            Code::Identity,
        ),
        (
            "the runner not running",
            |it| it["runner"]["power"] = json!("stopped"),
            Code::Identity,
        ),
        (
            "the runner unconfirmed",
            |it| it["runner"]["confirmed"] = json!(false),
            Code::Identity,
        ),
        (
            "no runner",
            |it| it["runner"] = json!({"ok": false, "error": "gone"}),
            Code::Identity,
        ),
        (
            "the window open",
            |it| it["runner"]["window"] = json!("open"),
            Code::OnConsole,
        ),
        (
            "the window unknown",
            |it| it["runner"]["window"] = json!("minimized"),
            Code::OnConsole,
        ),
        (
            "the window not said",
            |it| it["runner"]["window"] = json!(null),
            Code::OnConsole,
        ),
        (
            "off its console",
            |it| it["status"]["hello"]["session"]["onConsole"] = json!(false),
            Code::Identity,
        ),
        (
            "the proof's parent another",
            |it| it["status"]["sessionProof"]["proof"]["parentPID"] = json!(124),
            Code::Identity,
        ),
        (
            "another display",
            |it| it["status"]["sessionProof"]["proof"]["width"] = json!(1280.0),
            Code::Identity,
        ),
        (
            "the proof's scale alone",
            |it| it["status"]["sessionProof"]["proof"]["scale"] = json!(1.0),
            Code::Identity,
        ),
        (
            "posture holding",
            |it| it["status"]["hello"]["account"]["admin"] = json!(true),
            Code::Platform,
        ),
    ];
    for (what, change, code) in refusals {
        let mut said = answer("c1");
        change(&mut said);

        let refused = proved(&said, "c1", &paired()).unwrap_err();

        assert_eq!(refused.code, code, "{what}: {}", refused.error);
    }
    let mut another = paired();
    another.uid = 503;
    assert!(
        proved(&answer("c1"), "c1", &another).is_err(),
        "another uid paired"
    );
}

#[test]
fn input_counts_only_from_its_own_epoch_and_the_daemons_very_build() {
    let input_false: [(&str, Change); 8] = [
        ("another epoch", |it| {
            it["status"]["inputService"]["session"]["securitySessionID"] = json!(43)
        }),
        ("no process", |it| {
            it["status"]["inputService"]["pid"] = json!(null)
        }),
        ("another build", |it| {
            it["status"]["inputService"]["build"] = json!("b2")
        }),
        ("no build of its own", |it| {
            it["status"]["inputService"]["build"] = json!(null)
        }),
        ("no build of the daemon's", |it| {
            it["status"]["hello"]["daemon"]["build"] = json!(null)
        }),
        ("Accessibility not allowed", |it| {
            it["status"]["inputService"]["accessibility"] = json!(false)
        }),
        ("its scale alone", |it| {
            it["status"]["inputService"]["session"]["scale"] = json!(1.0)
        }),
        ("no input hello at all", |it| {
            it["status"]["inputService"] = json!({"available": false, "error": "The lane ended."})
        }),
    ];
    for (what, change) in input_false {
        let mut said = answer("c1");
        change(&mut said);

        let session = proved(&said, "c1", &paired()).unwrap();

        assert!(!session.input, "{what}");
        assert!(session.capture, "{what}");
    }
}

#[test]
fn a_proof_of_another_challenge_is_refused_and_two_proofs_take_two() {
    let replayed = Observed::decode(&answer("earlier"), "now").unwrap_err();
    assert_eq!(replayed.code, Code::Identity);
    assert!(replayed.error.contains("did not match its challenge"));

    let first = proved(&answer("one"), "one", &paired()).unwrap();
    let second = proved(&answer("two"), "two", &paired()).unwrap();
    assert_eq!(first.epoch, second.epoch);
}

#[test]
fn a_proof_that_failed_is_refused_in_its_own_words_and_no_observation_is_taken() {
    let mut failed = answer("c1");
    failed["status"]["sessionProof"] =
        json!({"ok": false, "code": "identity", "error": "The proof worker failed"});
    let refused = Observed::decode(&failed, "c1").unwrap_err();
    assert!(
        refused.error.contains("The proof worker failed"),
        "{}",
        refused.error
    );

    let mut silent = answer("c1");
    silent["status"]["hello"] =
        json!({"ok": false, "code": "transport", "error": "The lane ended."});
    let refused = Observed::decode(&silent, "c1").unwrap_err();
    assert_eq!(refused.code, Code::Transport);
    assert!(
        refused.error.contains("The lane ended."),
        "{}",
        refused.error
    );

    for malformed in [
        json!({"ok": true, "op": "hello"}),
        json!({"ok": true, "op": "observe", "generation": 7, "peer": "nope"}),
        json!({"ok": false, "code": "busy"}),
    ] {
        let refused = Observed::decode(&malformed, "c1").unwrap_err();
        assert_eq!(refused.code, Code::Protocol, "{malformed}");
    }
}

#[test]
fn only_a_running_confirmed_virtual_machine_is_eligible_and_only_a_closed_window_hands_off() {
    let observation = |virtual_machine: Value| {
        Observation::decode(&json!({"ok": true, "machine": {"virtual": virtual_machine}}))
    };
    let running = Runner {
        power: Some(Power::Running),
        confirmed: Some(true),
        window: Some(Window::Closed),
        ..Runner::default()
    };

    assert!(eligible(&running, &observation(json!(true))));
    assert!(!eligible(&running, &observation(json!(false))));
    assert!(!eligible(&running, &observation(json!(null))));
    for power in [Power::Starting, Power::Stopped, Power::Unknown] {
        let runner = Runner {
            power: Some(power),
            ..running.clone()
        };
        assert!(!eligible(&runner, &observation(json!(true))), "{power:?}");
    }
    let unconfirmed = Runner {
        confirmed: None,
        ..running.clone()
    };
    assert!(!eligible(&unconfirmed, &observation(json!(true))));

    assert_eq!(hands_off(&running), Some(true));
    for (window, hands) in [
        (Some(Window::Open), Some(false)),
        (Some(Window::Unknown), None),
        (None, None),
    ] {
        let runner = Runner {
            window,
            ..running.clone()
        };
        assert_eq!(hands_off(&runner), hands, "{window:?}");
    }
}

#[test]
fn atlas_in_a_vm_is_mid_update_only_while_a_ready_input_service_runs_another_build() {
    let status = |change: Change| {
        let mut said = answer("c1");
        change(&mut said);
        Observed::decode(&said, "c1").unwrap().status
    };

    assert!(mid_update(&status(|it| {
        it["status"]["inputService"]["build"] = json!("b2");
    })));
    let not_mid_update: [(&str, Change); 5] = [
        ("the same build", |_| {}),
        ("not ready", |it| {
            it["status"]["inputService"]["build"] = json!("b2");
            it["status"]["inputService"]["ready"] = json!(false);
        }),
        ("no build of its own", |it| {
            it["status"]["inputService"]["build"] = json!(null)
        }),
        ("no build of the daemon's", |it| {
            it["status"]["hello"]["daemon"]["build"] = json!(null)
        }),
        ("no input hello", |it| {
            it["status"]["inputService"] = json!({"available": false, "error": "gone"})
        }),
    ];
    for (what, change) in not_mid_update {
        assert!(!mid_update(&status(change)), "{what}");
    }
}

#[test]
fn a_vm_under_maintenance_is_hands_on_even_with_its_window_closed() {
    let observed = Observed::decode(&answer("c1"), "c1").unwrap();
    let held = crate::vm::maintenance::on("work");

    let refused = observed
        .prove(&vm(), &paired(), Some(crate::vm::maintenance::on("work")))
        .unwrap_err();

    assert_eq!(observed.runner.window, Some(Window::Closed));
    assert_eq!(refused.code, Code::Busy, "{refused:?}");
    assert_eq!(refused.error, held.error);
    assert!(!refused.error.contains("helper"), "{refused:?}");
    assert!(observed.prove(&vm(), &paired(), None).is_ok());
}

#[test]
fn an_unreadable_lifecycle_record_counts_as_maintenance() {
    use crate::vm::{lifecycle::Lifecycle, maintenance::Kept};
    let home = tempfile::tempdir().unwrap();
    let lifecycle = Lifecycle::under(home.path());
    let id = atlas_core::desktop::DesktopId::parse(crate::test_support::VM_ID).unwrap();
    let record = lifecycle.record_at(&id);
    std::fs::create_dir_all(record.parent().unwrap()).unwrap();
    std::fs::write(&record, b"{not a record").unwrap();
    let observed = Observed::decode(&answer("c1"), "c1").unwrap();

    let (kept, _) = Kept::read(Some(&lifecycle), &id);
    let held = kept.held("work").expect("held");
    let refused = observed.prove(&vm(), &paired(), Some(held)).unwrap_err();

    assert_eq!(kept, Kept::Unreadable);
    assert_eq!(refused.code, Code::State, "{refused:?}");
    assert!(refused.error.contains("cannot be read"), "{refused:?}");
    assert_eq!(
        Kept::read(None, &id).0,
        Kept::Unreadable,
        "no lifecycle at all"
    );
}

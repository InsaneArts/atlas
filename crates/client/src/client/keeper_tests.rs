use super::*;

fn told(lifted: Value) -> Value {
    json!({"ok": true, "said": true, "seq": 4, "outcome": "resumed", "lifted": lifted})
}

#[test]
fn only_a_stop_lifted_or_already_gone_lets_agents_go_on() {
    for lifted in ["cleared", "already-clear"] {
        let reply = resumption(&told(json!(lifted)));

        assert_eq!(reply["held"], false, "{lifted}");
        assert!(reply.get("warnings").is_none(), "{lifted}");
    }
}

#[test]
fn a_newer_stop_a_failed_lift_or_an_answer_not_understood_holds_agents_and_says_so() {
    for lifted in [
        json!("stop-held"),
        json!({"ok": false, "code": "busy", "error": "Another Stop or Resume is still being kept"}),
        json!("something new"),
        Value::Null,
    ] {
        let reply = resumption(&told(lifted.clone()));

        assert_eq!(reply["held"], true, "{lifted}");
        assert!(reply["warnings"][0].is_string(), "{lifted}");
    }
    let unconfirmed = resumption(&json!({"ok": true, "said": true, "seq": 4,
        "outcome": "unconfirmed", "lifted": "cleared"}));
    assert_eq!(unconfirmed["held"], true);
}

#[test]
fn a_vms_status_that_fails_still_says_an_unfinished_handover_and_keeps_the_keepers_own_failure() {
    let failed = Error::new(Code::Transport, "work's session keeper did not answer.")
        .with("why", json!("work's session keeper is not running"));
    let why = "An Atlas update is still handing work's session to the new build; run atlas \
               update again.";

    let said = json!(handing_over(failed, Some(why.to_owned())));

    assert_eq!(said["code"], "transport");
    assert_eq!(said["error"], "work's session keeper did not answer.");
    assert_eq!(said["why"], "work's session keeper is not running");
    assert_eq!(said["station"]["handover"], why);
    let plain = json!(handing_over(Error::new(Code::Transport, "x"), None));
    assert!(plain.get("station").is_none());
}

/// What a guest serving all this build asks of a VM declares.
fn everything() -> Declared {
    Declared::ours(MACHINE.iter().map(|name| (*name).to_owned()).collect())
}

/// The status of the VM `work`, ready for agents: live, proven, allowed,
/// clear, closed to all but Atlas, no handover, its keeper and Atlas VM of
/// this build, nothing unresolved, and work its guest started still
/// running.
fn ready() -> Value {
    let mut guest = crate::test_support::machine_hello("clear");
    guest["inputService"] = json!({"ok": true, "ready": true, "build": "b1"});
    json!({"ok": true, "proven": true,
        "proof": {"capture": true, "input": true, "generation": 1},
        "declared": everything(),
        "desktop": {"kind": "vm", "name": "work"},
        "keeper": {"live": true, "generation": 1, "guest": everything(),
            "keeper": {"capabilities": crate::vm::keeper::CAPABILITIES}},
        "runner": {"ok": true, "power": "running", "window": "closed",
            "incarnation": "6f1c0de2-run", "fence": 3},
        "guest": guest,
        "stop": {"state": "clear"},
        "leftovers": "running",
        "station": {"stop": {"state": "clear"}, "handover": null, "pending": [],
            "unresolved": []}})
}

/// One way a rule fails: what it changes of a ready status, and the words
/// it is then said in.
type Failing = (fn(&mut Value), &'static str);

/// How each rule fails on its own, in the order they are judged.
fn each_rule_failing() -> [Failing; 19] {
    [
        (
            |status| {
                status["keeper"]["live"] = json!(false);
                status["proven"] = json!(false);
                status["proof"] = json!({"code": "not-ready",
                    "error": "work is stopped. A command that needs it, such as run, starts it."});
            },
            "work is stopped. A command that needs it, such as run, starts it.",
        ),
        (
            |status| {
                status["proven"] = json!(false);
                status["proof"] = json!({"code": "on-console",
                    "error": "work's window is open; close it so agents can use it."});
            },
            "work's window is open; close it so agents can use it.",
        ),
        (
            |status| {
                status["proof"]["capture"] = json!(false);
                status["guest"]["capture"]["held"] = json!(
                    "Allow Atlas in Privacy & Security, Screen & System Audio Recording, in the \
                     VM's window: agents' pictures wait for it."
                );
            },
            "Allow Atlas in Privacy & Security, Screen & System Audio Recording, in the VM's \
             window: agents' pictures wait for it.",
        ),
        (
            |status| status["guest"]["inputService"]["build"] = json!("b2"),
            crate::vm::proof::MID_UPDATE,
        ),
        (
            |status| {
                status["proof"]["input"] = json!(false);
                status["guest"]["input"]["held"] = json!(
                    "Allow Atlas in Privacy & Security, Accessibility, in the VM's window: agents \
                     wait for it."
                );
            },
            "Allow Atlas in Privacy & Security, Accessibility, in the VM's window: agents wait \
             for it.",
        ),
        (
            |status| status["station"]["stop"] = json!({"state": "stopped", "why": "person"}),
            "The person stopped agents on work. Ask them before doing anything more there.",
        ),
        (
            |status| status["station"]["stop"] = json!({"state": "stopped", "why": "update"}),
            "An update of Atlas stopped agents on work; they wait until the person resumes them \
             from the Atlas menu bar.",
        ),
        (
            |status| status["station"]["stop"] = json!({"state": "unknown"}),
            "The Stop this Mac keeps for work cannot be read, so agents wait until the person \
             resumes them from the Atlas menu bar.",
        ),
        (
            |status| status["guest"]["stop"] = json!({"state": "stopped", "why": "recording"}),
            "work's Atlas restarted to take Screen Recording, so it holds agents until the \
             person resumes them from the Atlas menu bar.",
        ),
        (
            |status| status["guest"]["stop"] = json!({"state": "unknown"}),
            "work's own Stop cannot be read, so agents wait until the person resumes them from \
             the Atlas menu bar.",
        ),
        (
            |status| status["guest"]["agents"]["stopped"] = json!(true),
            "work is stopping its agents, so it holds them until the person resumes them from \
             the Atlas menu bar.",
        ),
        (
            |status| {
                status["guest"]["agents"]["admitted"] = json!(false);
                status["guest"]["agents"]["held"] =
                    json!("Agents wait until Remote Login is turned off in the VM.");
            },
            "Agents wait until Remote Login is turned off in the VM.",
        ),
        (
            |status| {
                status["station"]["handover"] =
                    json!("An Atlas update is still handing work's session to the new build.");
            },
            "An Atlas update is still handing work's session to the new build.",
        ),
        (
            |status| {
                let older: Vec<&str> = crate::vm::keeper::CAPABILITIES
                    .into_iter()
                    .filter(|capability| *capability != "picture-v1")
                    .collect();
                status["keeper"]["keeper"]["capabilities"] = json!(older);
            },
            "work's session keeper is from an older build; run atlas update.",
        ),
        (
            |status| status["declared"] = Value::Null,
            "work's session keeper is from an older build; run atlas update.",
        ),
        (
            |status| status["runner"]["incarnation"] = Value::Null,
            "work's Atlas VM is from an older build and keeps no picture fence; run atlas update.",
        ),
        (
            |status| status["runner"]["fence"] = Value::Null,
            "work's Atlas VM is from an older build and keeps no picture fence; run atlas update.",
        ),
        (
            |status| {
                let mut older = everything();
                older
                    .capabilities
                    .retain(|name| name != atlas_session::compatibility::PICTURES);
                status["declared"] = json!(older);
            },
            "work's Atlas is from another build that does not serve all this one needs \
             (pictures-v1); update Atlas so both are one build.",
        ),
        (
            |status| {
                status["station"]["unresolved"] =
                    json!([{"generation": "ab12", "executor": "guest", "holder": "gone"}]);
            },
            "An action on work may not have finished; Atlas settles it on its own once work \
             answers. If it stays, run atlas doctor --desktop work.",
        ),
    ]
}

/// [`readiness`] of `status`, as it says what its guest declared and its
/// station what it holds.
fn judged(status: &Value) -> std::result::Result<(), String> {
    readiness(
        status,
        &declared_of(status),
        &status["station"]["unresolved"],
    )
}

#[test]
fn a_vm_with_every_rule_holding_is_ready_though_work_its_guest_started_still_runs() {
    let status = ready();

    assert_eq!(judged(&status), Ok(()));
    assert_eq!(readiness_of(&status), Ok(()));
    let checks = readiness_checks(&status);
    assert_eq!(checks.len(), Rule::ALL.len());
    assert!(checks.iter().all(|check| check["ok"] == true), "{checks:?}");
}

#[test]
fn each_rule_failing_on_its_own_says_its_own_words_and_one_judged_before_comes_first() {
    let failing = each_rule_failing();
    for (at, (fail, words)) in failing.iter().enumerate() {
        let mut status = ready();
        fail(&mut status);

        assert_eq!(judged(&status), Err((*words).to_owned()), "{status}");
        let checks = readiness_checks(&status);
        let first = checks.iter().find(|check| check["ok"] != true).unwrap();
        assert_eq!(first["ok"], false, "{checks:?}");
        assert_eq!(first["nextStep"], *words);

        // Every rule judged after it fails too: its words still come first.
        for (later, _) in &failing[at + 1..] {
            let mut both = ready();
            later(&mut both);
            fail(&mut both);
            assert_eq!(judged(&both), Err((*words).to_owned()), "{both}");
        }
    }
}

#[test]
fn a_rule_that_comes_after_one_that_fails_is_not_known_in_doctors_checks() {
    let mut status = ready();
    status["proven"] = json!(false);
    status["proof"] = json!({"code": "identity", "error": "work's keys do not match."});

    let checks = readiness_checks(&status);

    let ok = |name: &str| {
        checks
            .iter()
            .find(|check| check["name"] == name)
            .map(|check| check["ok"].clone())
            .unwrap()
    };
    assert_eq!(ok("VM proved"), false);
    assert_eq!(
        ok("Screen Recording and Accessibility allowed"),
        Value::Null
    );
    assert_eq!(ok("Open to no one but Atlas"), Value::Null);
    assert_eq!(
        ok("Agents not stopped"),
        true,
        "judged once Atlas reaches it"
    );
    assert_eq!(ok("No action of unknown outcome"), true);
}

#[test]
fn a_change_atlas_vm_left_with_no_answer_keeps_a_vm_from_being_ready_in_words_naming_it() {
    let mut status = ready();
    status["station"]["unresolved"] =
        json!([{"generation": "cd34", "executor": "vm", "holder": "gone"}]);

    assert_eq!(
        judged(&status),
        Err(
            "Atlas VM has not said yet what a change it was making to work came to, so agents \
             wait until it does."
                .to_owned()
        )
    );
    for (unresolved, words) in [
        (
            json!({"generation": "a", "executor": "station", "holder": "gone"}),
            "What a change Atlas made on this Mac for work came to is not known, so agents wait.",
        ),
        (
            json!({"generation": "a", "executor": null, "holder": "gone"}),
            "What a change on work came to is not known, so agents wait.",
        ),
    ] {
        status["station"]["unresolved"] = json!([unresolved]);
        assert_eq!(judged(&status), Err(words.to_owned()));
    }
}

#[test]
fn a_change_a_live_process_is_making_leaves_a_vm_ready_and_one_left_beside_it_does_not() {
    let mut status = ready();
    status["station"]["unresolved"] =
        json!([{"generation": "a", "executor": "guest", "holder": "live"}]);

    assert_eq!(judged(&status), Ok(()), "work in progress");
    assert_eq!(
        under_way("work", &status["station"]["unresolved"]).as_deref(),
        Some("A change to work is still being made.")
    );

    status["station"]["unresolved"] = json!([
        {"generation": "a", "executor": "guest", "holder": "live"},
        {"generation": "b", "executor": "vm", "holder": "gone"},
    ]);
    assert_eq!(
        judged(&status),
        Err(
            "Atlas VM has not said yet what a change it was making to work came to, so agents \
             wait until it does."
                .to_owned()
        )
    );
    status["station"]["unresolved"] = json!([]);
    assert_eq!(under_way("work", &status["station"]["unresolved"]), None);
}

#[test]
fn pending_changes_that_cannot_be_read_keep_a_vm_from_being_ready() {
    let mut status = ready();
    for unreadable in [json!("unknown"), Value::Null] {
        status["station"]["unresolved"] = unreadable;

        assert_eq!(
            judged(&status),
            Err(
                "Which of Atlas's changes on work are still unsettled cannot be read, so agents \
                 wait."
                    .to_owned()
            )
        );
    }
}

#[test]
fn a_vm_whose_atlas_speaks_another_protocol_or_declares_nothing_is_not_ready() {
    let mut status = ready();
    status["declared"]["protocol"] = json!(PROTOCOL + 1);
    assert!(
        judged(&status)
            .unwrap_err()
            .starts_with(&format!("work's Atlas speaks protocol {}", PROTOCOL + 1))
    );

    status["declared"] = json!({"protocol": PROTOCOL});
    assert!(
        readiness_of(&status)
            .unwrap_err()
            .starts_with("work's Atlas is from another build"),
        "a guest that declared nothing serves nothing"
    );
}

#[test]
fn no_words_readiness_gives_a_vm_say_helper() {
    let mut said: Vec<String> = each_rule_failing()
        .into_iter()
        .map(|(fail, _)| {
            let mut status = ready();
            fail(&mut status);
            judged(&status).unwrap_err()
        })
        .collect();
    said.extend(
        readiness_checks(&json!({"ok": true, "desktop": {"name": "work"}}))
            .iter()
            .filter_map(|check| check["nextStep"].as_str().map(str::to_owned)),
    );
    said.extend(Rule::ALL.iter().map(|rule| rule.name().to_owned()));

    for words in said {
        assert!(!words.to_lowercase().contains("helper"), "{words}");
    }
}

#[test]
fn a_change_whose_holder_cannot_be_told_keeps_a_vm_from_being_ready_in_words_saying_so() {
    let mut status = ready();
    for holder in [json!("unknown"), json!("something new"), Value::Null] {
        status["station"]["unresolved"] =
            json!([{"generation": "a", "executor": "guest", "holder": holder}]);

        assert_eq!(
            judged(&status),
            Err(
                "Whether a change to work is still being made cannot be told, so agents wait."
                    .to_owned()
            ),
            "{holder}"
        );
        assert_eq!(
            under_way("work", &status["station"]["unresolved"]),
            None,
            "never said as under way"
        );
    }
}

#[test]
fn a_vms_capabilities_are_judged_in_the_generation_it_was_observed_in_never_from_a_hello() {
    let mut status = ready();
    let mut older = everything();
    older
        .capabilities
        .retain(|name| name != atlas_session::compatibility::PICTURES);
    // The keeper's hello read before a reconnect said everything; the proof
    // was taken in the next generation, of an older build.
    status["declared"] = json!(older);

    assert!(
        readiness_of(&status).unwrap_err().contains("(pictures-v1)"),
        "the proof's generation decides"
    );

    // No observation said it: the hello's, which says everything, never
    // stands in, and doctor does not know.
    status["declared"] = Value::Null;
    assert_eq!(
        readiness_of(&status),
        Err("work's session keeper is from an older build; run atlas update.".to_owned())
    );
    let served = |status: &Value| {
        readiness_checks(status)
            .into_iter()
            .find(|check| check["name"] == "Atlas in it serves this build")
            .unwrap()
    };
    assert_eq!(served(&status)["ok"], Value::Null, "not known");

    // A proof refused in the observed generation: what that observation
    // declared is judged, never the hello's.
    status["proven"] = json!(false);
    status["proof"] = json!({"code": "on-console", "error": "work's window is open."});
    assert_eq!(served(&status)["ok"], Value::Null, "{status}");
    status["declared"] = json!(older);
    assert_eq!(served(&status)["ok"], false);
    assert!(
        served(&status)["nextStep"]
            .as_str()
            .unwrap()
            .contains("(pictures-v1)")
    );
}

#[test]
fn an_unsettled_change_on_a_vm_never_points_at_the_menu_bar_or_atlas_restart() {
    for executor in [json!("guest"), json!("vm"), json!("station"), Value::Null] {
        for holder in ["gone", "unknown"] {
            let mut status = ready();
            status["station"]["unresolved"] =
                json!([{"generation": "a", "executor": executor, "holder": holder}]);

            let said = judged(&status).unwrap_err();

            assert!(!said.contains("menu bar"), "{said}");
            assert!(!said.contains("atlas restart"), "{said}");
        }
    }
}

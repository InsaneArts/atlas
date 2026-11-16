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

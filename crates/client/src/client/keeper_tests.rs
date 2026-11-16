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

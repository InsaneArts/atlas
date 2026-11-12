use super::*;
use serde_json::json;

fn hidden(uid: u32) -> Desktop {
    Desktop::from(Hidden { uid })
}

#[test]
fn a_hidden_login_keeps_the_record_name_installs_already_use() {
    assert_eq!(hidden(502).id().as_str(), "c6a7efd7472129144451d113");
    assert_eq!(hidden(503).id().as_str(), "ae2892390aeb6c07f55a0eea");
}

#[test]
fn a_desktop_is_named_by_its_account_alone() {
    assert_eq!(hidden(502).id(), hidden(502).id());
    assert_ne!(hidden(502).id(), hidden(503).id());
}

#[test]
fn a_hidden_login_is_proved_by_the_hidden_login_check() {
    for value in [
        json!({"ok": false}),
        json!({"ok": true, "user": {"uid": 503}}),
        json!({"ok": true, "user": {"uid": 502}, "session": {"onConsole": true}}),
    ] {
        let observation = Observation::decode(&value);
        let refused = hidden(502).prove(&observation).unwrap_err();
        let expected = observation.require_hidden(502).unwrap_err();
        assert_eq!(refused.code, expected.code, "{value}");
        assert_eq!(refused.error, expected.error, "{value}");
        assert!(!hidden(502).input_current(&observation, 1), "{value}");
    }
}

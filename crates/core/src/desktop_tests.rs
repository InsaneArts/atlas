use super::*;
use serde_json::json;

fn login(uid: u32) -> Desktop {
    Desktop::from(SecondLogin { uid })
}

#[test]
fn a_second_login_keeps_the_record_name_installs_already_use() {
    assert_eq!(login(502).id().as_str(), "c6a7efd7472129144451d113");
    assert_eq!(login(503).id().as_str(), "ae2892390aeb6c07f55a0eea");
}

#[test]
fn a_desktop_is_named_by_its_account_alone() {
    assert_eq!(login(502).id(), login(502).id());
    assert_ne!(login(502).id(), login(503).id());
}

#[test]
fn a_second_login_is_proved_by_the_second_login_check() {
    for value in [
        json!({"ok": false}),
        json!({"ok": true, "user": {"uid": 503}}),
        json!({"ok": true, "user": {"uid": 502}, "session": {"onConsole": true}}),
    ] {
        let observation = Observation::decode(&value);
        let refused = login(502).prove(&observation).unwrap_err();
        let expected = observation.require_hidden(502).unwrap_err();
        assert_eq!(refused.code, expected.code, "{value}");
        assert_eq!(refused.error, expected.error, "{value}");
        assert!(!login(502).input_current(&observation, 1), "{value}");
    }
}

use super::*;
use serde_json::json;

#[test]
fn the_status_names_each_verdict_in_one_word() {
    assert_eq!(
        serde_json::to_value([
            Restarting::Quiet,
            Restarting::Restarting,
            Restarting::Unknown
        ])
        .unwrap(),
        json!(["none", "restarting", "unknown"])
    );
}

#[test]
fn only_the_helper_s_own_answer_counts_as_answered() {
    assert!(answered(&json!({"ok": true, "daemon": {}})));
    for reply in [
        json!({"ok": false, "code": "blocked"}),
        json!({"ok": false, "code": "identity"}),
        json!({"ok": false, "code": "busy"}),
        json!({"ok": false, "code": "transport"}),
        json!(null),
    ] {
        assert!(!answered(&reply), "{reply}");
    }
}

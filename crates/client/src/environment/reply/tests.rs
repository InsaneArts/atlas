use super::*;
use serde_json::json;

/// `text` read as `T` and written again, as JSON with sorted keys.
fn again<T: Serialize + serde::de::DeserializeOwned>(text: &str) -> String {
    let read: T = serde_json::from_str(text).unwrap();
    serde_json::to_string(&serde_json::to_value(read).unwrap()).unwrap()
}

#[test]
fn each_reply_reads_back_as_it_was_sent() {
    let installs = [
        r#"{"app":"Probe","bundleId":"test.atlas.install-probe","installId":"ID","path":"/h/Applications/Probe.app","phase":"installed","present":true,"processes":[{"executable":"/h/Applications/Probe.app/Contents/MacOS/Probe","pid":812}],"root":{"device":1,"inode":2},"unchanged":null,"unproven":[],"verified":true}"#,
        r#"{"app":"Probe","bundleId":null,"installId":"ID","phase":"held","verified":false}"#,
        r#"{"installId":"ID","phase":"unreadable","verified":false}"#,
        r#"{"installId":"ID","phase":"missing"}"#,
    ];
    for text in installs {
        assert_eq!(again::<InstallState>(text), text);
    }
    let removals = [
        r#"{"app":"Probe","heldPath":"/h/held/ID/Probe.app","installId":"ID","ok":true,"outcome":"held","sideEffects":{"kept":[{"existedBefore":false,"path":"Library/Caches/x"}]}}"#,
        r#"{"app":"Probe","installId":"ID","ok":true,"outcome":"kept","reason":"other-volume","sideEffects":{"kept":[]}}"#,
        r#"{"installId":"ID","ok":true,"outcome":"kept","reason":"unreadable"}"#,
        r#"{"installId":"ID","ok":true,"outcome":"alreadyGone"}"#,
        r#"{"app":"Probe","installId":"ID","ok":true,"outcome":"inUse"}"#,
    ];
    for text in removals {
        assert_eq!(again::<Removal>(text), text);
    }
    let listed = r#"{"hidden":1,"more":0,"ok":true,"transactions":[{"active":true,"files":[{"legacyPrevious":false,"operation":"create","target":"a"}],"phase":"applied","planId":"P","retainedDirectories":0,"updatedAt":5}],"unreadable":0}"#;
    assert_eq!(again::<Transactions>(listed), listed);
}

#[test]
fn a_phase_or_reason_this_build_does_not_know_is_never_taken_for_one_it_does() {
    let state: InstallState =
        serde_json::from_value(json!({"installId": "ID", "phase": "sideways"})).unwrap();
    assert_eq!(state.phase, Lenient::Later(json!("sideways")));
    assert!(!state.verified());

    let removal: Removal =
        serde_json::from_value(json!({"ok": true, "outcome": "kept", "reason": "new"})).unwrap();
    assert_eq!(
        removal.outcome,
        Outcome::Kept {
            reason: KeptReason::Changed
        }
    );

    let listed: Transactions = serde_json::from_value(json!({"ok": true, "transactions": [
        {"planId": "P", "phase": "sideways"},
        {"phase": "applied"},
    ]}))
    .unwrap();
    assert_eq!(listed.known().count(), 1);
    assert!(matches!(
        listed.transactions[1],
        Lenient::Later(Value::Object(_))
    ));
}

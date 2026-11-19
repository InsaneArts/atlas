use super::*;

#[test]
fn only_a_known_sound_is_an_answer() {
    let said = answer(
        &json!({"ok": true, "op": "sound", "helperSound": "unavailable",
        "reason": "Allow Atlas.", "wanted": true}),
    )
    .unwrap();
    assert_eq!(
        said,
        json!({"ok": true, "helperSound": "unavailable", "reason": "Allow Atlas.",
            "wanted": true})
    );

    assert!(answer(&json!({"ok": true, "helperSound": "loud", "wanted": true})).is_err());
    assert!(answer(&json!({"ok": true, "helperSound": "on"})).is_err());
    assert!(answer(&json!({"ok": true})).is_err());
}

#[tokio::test]
async fn a_vm_is_said_to_have_no_sound_and_nothing_is_asked() {
    let dir = tempfile::tempdir().unwrap();
    let client = crate::test_support::vm_client(dir.path(), Some("main"));
    for refused in [
        show(&client).await.unwrap_err(),
        set(&client, false).await.unwrap_err(),
    ] {
        assert_eq!(refused.code, Code::Platform);
        assert_eq!(
            refused.error,
            "work is a VM, which has no sound: its apps are never heard."
        );
        assert!(!refused.error.contains("helper"));
        assert_eq!(refused.details["next"], "Nothing was sent to it.");
    }
}

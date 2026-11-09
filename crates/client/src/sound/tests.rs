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

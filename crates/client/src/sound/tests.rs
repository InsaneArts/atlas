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

#[test]
fn a_change_the_helper_could_not_confirm_stays_uncertain() {
    let refused = atlas_core::transport::refusal(&json!({"ok": false, "code": "daemon",
        "error": "The helper did not confirm its sound changed; run atlas sound",
        "uncertain": true}));

    assert!(refused.uncertain);
}

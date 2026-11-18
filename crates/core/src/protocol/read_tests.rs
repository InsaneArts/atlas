use super::*;
use serde_json::json;

#[test]
fn each_reason_reads_back_as_the_word_replies_give() {
    let words = [
        "outside",
        "protected",
        "symlink",
        "not-regular",
        "dataless",
        "owner",
        "too-large",
        "changing",
        "missing",
        "unreadable",
        "name",
        "exists",
        "space",
        "session-changed",
        "on-console",
        "gone",
    ];
    assert_eq!(ReadRefusal::ALL.len(), words.len());
    for (reason, word) in ReadRefusal::ALL.into_iter().zip(words) {
        assert_eq!(json!(reason), json!(word));
        assert_eq!(ReadRefusal::of(&json!({"reason": word})), Some(reason));
    }
    assert_eq!(ReadRefusal::of(&json!({"reason": "later"})), None);
}

#[test]
fn every_reason_said_of_a_vm_names_the_vm_and_never_the_helper() {
    for reason in ReadRefusal::ALL {
        let said = reason.said_on_vm();
        assert!(!said.contains("helper"), "{reason:?}: {said}");
        if reason.said().contains("helper") {
            assert!(said.contains("VM"), "{reason:?}: {said}");
        }
    }
}

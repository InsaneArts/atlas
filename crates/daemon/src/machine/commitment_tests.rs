use super::*;
use crate::machine::vault::Memory;
use std::sync::Arc;

fn revision(number: u64) -> Revision {
    Revision {
        number,
        digest: format!("{number:064x}"),
    }
}

/// The commitment of a store created as pairing creates it.
fn commitment() -> (Arc<Memory>, Commitment<Arc<Memory>>) {
    let vault = Arc::new(Memory::default());
    create(&vault, "outcomes").unwrap();
    (vault.clone(), Commitment::new(vault, "outcomes"))
}

#[test]
fn a_store_just_created_is_trusted_only_while_it_is_empty() {
    let (_vault, commitment) = commitment();

    assert_eq!(commitment.judge(None).unwrap(), Judged::Current);
    assert_eq!(
        commitment.judge(Some(&revision(1))).unwrap(),
        Judged::Unknown
    );
}

#[test]
fn creating_a_store_again_keeps_what_it_committed() {
    let (vault, commitment) = commitment();
    commitment.begin(revision(1)).unwrap();
    commitment.settle().unwrap();

    create(&vault, "outcomes").unwrap();

    assert_eq!(
        commitment.judge(Some(&revision(1))).unwrap(),
        Judged::Current
    );
}

#[test]
fn a_write_is_committed_before_it_is_made_and_current_once_settled() {
    let (_vault, commitment) = commitment();

    commitment.begin(revision(1)).unwrap();
    // Crashed between: either the old state or the new one is accepted.
    assert_eq!(commitment.judge(None).unwrap(), Judged::Current);
    assert_eq!(commitment.judge(Some(&revision(1))).unwrap(), Judged::Next);

    commitment.settle().unwrap();
    assert_eq!(
        commitment.judge(Some(&revision(1))).unwrap(),
        Judged::Current
    );
    assert_eq!(commitment.judge(None).unwrap(), Judged::Unknown, "deleted");
}

#[test]
fn a_store_whose_commitment_was_lost_is_unknown_and_never_started_again() {
    let (vault, commitment) = commitment();
    commitment.begin(revision(1)).unwrap();
    commitment.settle().unwrap();

    // Its manifest and its commitment both gone.
    vault.lose("store outcomes");

    assert_eq!(commitment.judge(None).unwrap(), Judged::Unknown);
    assert!(commitment.begin(revision(1)).is_err());
    let never_created = Commitment::new(Arc::new(Memory::default()), "stop");
    assert_eq!(never_created.judge(None).unwrap(), Judged::Unknown);
}

#[test]
fn a_store_rolled_back_or_forged_is_unknown() {
    let (_vault, commitment) = commitment();
    for number in 1..=2 {
        commitment.begin(revision(number)).unwrap();
        commitment.settle().unwrap();
    }

    assert_eq!(
        commitment.judge(Some(&revision(1))).unwrap(),
        Judged::Unknown
    );
    let forged = Revision {
        number: 2,
        digest: "f".repeat(64),
    };
    assert_eq!(commitment.judge(Some(&forged)).unwrap(), Judged::Unknown);
}

#[test]
fn revisions_come_in_turn_and_one_pending_at_a_time() {
    let (_vault, commitment) = commitment();

    assert!(commitment.begin(revision(2)).is_err());
    commitment.begin(revision(1)).unwrap();
    commitment.begin(revision(1)).unwrap();
    assert!(commitment.begin(revision(2)).is_err());

    commitment.abandon().unwrap();
    assert_eq!(
        commitment.judge(Some(&revision(1))).unwrap(),
        Judged::Unknown
    );
    assert!(commitment.settle().is_err());
}

#[test]
fn a_damaged_or_inconsistent_commitment_is_refused_never_started_again() {
    let (vault, commitment) = commitment();
    let digest = "a".repeat(64);
    for damage in [
        "[]".to_owned(),
        // Its next going backwards.
        format!(
            r#"{{"version":1,"current":{{"number":5,"digest":"{digest}"}},"next":{{"number":2,"digest":"{digest}"}}}}"#
        ),
        // The empty store with a digest.
        format!(r#"{{"version":1,"current":{{"number":0,"digest":"{digest}"}},"next":null}}"#),
        // Its pending revision lost.
        format!(r#"{{"version":1,"current":{{"number":1,"digest":"{digest}"}}}}"#),
        // A digest that is not one.
        r#"{"version":1,"current":{"number":1,"digest":"XYZ"},"next":null}"#.to_owned(),
    ] {
        vault.damage("store outcomes", damage.as_bytes());

        assert!(commitment.judge(None).is_err(), "{damage}");
        assert!(commitment.begin(revision(1)).is_err(), "{damage}");
        assert!(commitment.settle().is_err(), "{damage}");
    }
}

#[test]
fn the_last_revision_is_refused_rather_than_turned_over() {
    let (vault, commitment) = commitment();
    let last = format!(
        r#"{{"version":1,"current":{{"number":{},"digest":"{}"}},"next":null}}"#,
        u64::MAX,
        "a".repeat(64)
    );
    vault.damage("store outcomes", last.as_bytes());

    assert!(commitment.begin(revision(0)).is_err());
}

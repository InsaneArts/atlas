use super::*;
use crate::machine::generations::Kept;
use crate::machine::vault::Memory;
use atlas_session::generation::{Generation, Generations};
use std::sync::Arc;

fn key(byte: u8) -> PeerKey {
    PeerKey::from_bytes([byte; 32])
}

fn pairing(folder: &std::path::Path) -> (Arc<Memory>, Pairing<Arc<Memory>>) {
    let vault = Arc::new(Memory::default());
    let pairing = Pairing::new(vault.clone(), folder.join("pairing-secret"));
    (vault, pairing)
}

#[test]
fn a_guest_commits_to_one_station_once_and_its_count_begins() {
    let folder = tempfile::tempdir().unwrap();
    let (vault, pairing) = pairing(folder.path());
    let change = Token::new();

    assert_eq!(pairing.committed().unwrap(), None);
    pairing.commit(key(1), &change).unwrap();
    pairing.commit(key(1), &change).unwrap();

    assert_eq!(pairing.committed().unwrap(), Some((key(1), change.clone())));
    assert!(pairing.commit(key(2), &change).is_err());
    assert!(pairing.commit(key(1), &Token::new()).is_err());
    assert_eq!(Kept::new(vault).next().unwrap(), Generation(1));
}

#[test]
fn a_damaged_commitment_is_refused_never_replaced() {
    let folder = tempfile::tempdir().unwrap();
    let (vault, pairing) = pairing(folder.path());
    vault.damage(NAME, b"{\"version\":2}");

    assert!(pairing.committed().is_err());
    assert!(pairing.commit(key(1), &Token::new()).is_err());
    assert_eq!(vault.read(NAME).unwrap().unwrap(), b"{\"version\":2}");
}

#[test]
fn the_secret_is_read_from_its_file_and_forgotten_for_good() {
    let folder = tempfile::tempdir().unwrap();
    let (_vault, pairing) = pairing(folder.path());
    assert!(pairing.secret().unwrap().is_none());

    let secret = [9_u8; 32];
    let hex: String = secret.iter().map(|byte| format!("{byte:02x}")).collect();
    std::fs::write(folder.path().join("pairing-secret"), format!("{hex}\n")).unwrap();
    assert_eq!(pairing.secret().unwrap().unwrap().as_bytes(), &secret);

    pairing.forget_secret().unwrap();
    pairing.forget_secret().unwrap();
    assert!(pairing.secret().unwrap().is_none());
}

#[test]
fn a_damaged_secret_is_refused() {
    let folder = tempfile::tempdir().unwrap();
    let (_vault, pairing) = pairing(folder.path());
    std::fs::write(folder.path().join("pairing-secret"), "not a secret").unwrap();

    assert_eq!(pairing.secret().unwrap_err().code, Code::Identity);
}

thread_local! {
    /// Whether this test's folder syncs fail.
    pub(super) static SYNC_FAILS: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[test]
fn a_secret_whose_removal_was_not_kept_is_kept_again_when_forgotten_again() {
    let folder = tempfile::tempdir().unwrap();
    let (_vault, pairing) = pairing(folder.path());
    std::fs::write(folder.path().join("pairing-secret"), "0".repeat(64)).unwrap();

    SYNC_FAILS.set(true);
    assert!(pairing.forget_secret().is_err(), "removed, but not kept");
    assert!(
        pairing.forget_secret().is_err(),
        "still not kept, though gone"
    );
    SYNC_FAILS.set(false);
    pairing.forget_secret().unwrap();
}

#[test]
fn pairing_creates_every_recovery_stores_commitment() {
    let folder = tempfile::tempdir().unwrap();
    let (vault, pairing) = pairing(folder.path());

    pairing.commit(key(1), &Token::new()).unwrap();

    for store in crate::machine::commitment::STORES {
        let commitment = crate::machine::commitment::Commitment::new(vault.clone(), store);
        assert_eq!(
            commitment.judge(None).unwrap(),
            crate::machine::commitment::Judged::Current,
            "{store}"
        );
    }
}

#[test]
fn of_two_adapters_committing_at_once_at_most_one_commits() {
    let folder = tempfile::tempdir().unwrap();
    let vault = Arc::new(Memory::default());
    let barrier = Arc::new(std::sync::Barrier::new(2));
    let committed: Vec<bool> = [1_u8, 2]
        .map(|byte| {
            let (vault, barrier) = (vault.clone(), barrier.clone());
            let secret = folder.path().join("pairing-secret");
            std::thread::spawn(move || {
                let pairing = Pairing::new(vault, secret);
                barrier.wait();
                pairing.commit(key(byte), &Token::new()).is_ok()
            })
        })
        .into_iter()
        .map(|thread| thread.join().unwrap())
        .collect();

    assert_eq!(committed.iter().filter(|ok| **ok).count(), 1);
}

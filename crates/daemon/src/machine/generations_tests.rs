use super::*;
use crate::machine::vault::Memory;
use std::sync::Arc;

#[test]
fn a_guest_that_never_paired_gives_no_generation() {
    let kept = Kept::new(Arc::new(Memory::default()));

    assert_eq!(kept.next().unwrap_err().code, Code::Identity);
}

#[test]
fn generations_go_up_from_the_pairing_and_are_never_given_twice() {
    let vault = Arc::new(Memory::default());
    begin(&vault).unwrap();
    let kept = Kept::new(vault.clone());

    assert_eq!(kept.next().unwrap(), Generation(1));
    assert_eq!(kept.next().unwrap(), Generation(2));
    // A second start keeps the count it finds.
    begin(&vault).unwrap();
    assert_eq!(Kept::new(vault).next().unwrap(), Generation(3));
}

#[test]
fn a_damaged_or_lost_count_refuses_and_never_starts_again() {
    let vault = Arc::new(Memory::default());
    begin(&vault).unwrap();
    vault.damage(NAME, b"{");
    let kept = Kept::new(vault.clone());
    assert!(kept.next().is_err());

    vault.lose(NAME);
    assert!(kept.next().is_err());
}

#[test]
fn two_adapters_giving_at_once_never_give_the_same_generation() {
    let vault = Arc::new(Memory::default());
    begin(&vault).unwrap();
    let barrier = Arc::new(std::sync::Barrier::new(8));
    let mut given: Vec<Generation> = (0..8)
        .map(|_| {
            let (vault, barrier) = (vault.clone(), barrier.clone());
            std::thread::spawn(move || {
                let kept = Kept::new(vault);
                barrier.wait();
                kept.next().unwrap()
            })
        })
        .collect::<Vec<_>>()
        .into_iter()
        .map(|thread| thread.join().unwrap())
        .collect();
    given.sort();
    given.dedup();

    assert_eq!(given.len(), 8);
}

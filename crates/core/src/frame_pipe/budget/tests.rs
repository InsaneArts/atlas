use super::*;

#[test]
fn a_reservation_is_all_or_nothing_and_returns_when_its_last_owner_drops() {
    let budget = Budget::new(100);
    let first = budget.reserve(60).unwrap();
    assert!(budget.reserve(41).is_none(), "nothing is partly held");
    assert!(budget.reserve(u64::MAX).is_none(), "the sum would overflow");
    assert_eq!(budget.reserved(), 60);
    let shared = std::sync::Arc::new(budget.reserve(40).unwrap());
    let copy = shared.clone();
    assert_eq!(budget.reserved(), 100);
    drop(shared);
    assert_eq!(budget.reserved(), 100, "one owner still holds it");
    drop(copy);
    drop(first);
    assert_eq!(budget.reserved(), 0);
}

#[test]
fn the_broker_admits_observers_only_while_every_copy_fits() {
    let square = 4096 * 4096 * 3;
    assert!(admits(square, 2, false), "288 MiB");
    assert!(admits(square, 3, false), "384 MiB");
    assert!(!admits(square, 4, false), "a fourth is refused");
    assert!(!admits(square, 3, true), "a snapshot's pin counts too");
    let display = 3024 * 1964 * 3;
    assert!(admits(display, 4, true));
    assert!((2 + 2 * 4) * display < 180 * 1024 * 1024, "about 170 MiB");
    assert!(!admits(SOURCE_MAX + 1, 1, false));
    assert!(!admits(u64::MAX, u64::MAX, true));
}

#[test]
fn a_fifth_observer_is_refused_however_small_its_pictures() {
    assert!(admits(640 * 400 * 3, OBSERVERS_MAX, true));
    assert!(!admits(640 * 400 * 3, OBSERVERS_MAX + 1, false));
}

use super::*;

#[test]
fn work_handed_over_is_orphaned_once_it_is_no_longer_owned() {
    let keepers = Keepers::default();
    let supervisor = Instance::new(100_500, [1, 0]);
    keepers.set(supervisor, Keeping::Active);
    assert!(!keepers.unsettled());

    keepers.hand_over(&supervisor);

    assert!(!keepers.owns(&supervisor));
    assert!(keepers.uncertain());
    assert!(keepers.unsettled());
    assert!(!keepers.settled_then(|| panic!("reopened over orphaned work")));
}

#[test]
fn a_stop_leaves_no_keeper_settled() {
    let keepers = Keepers::default();
    keepers.set(Instance::new(100_500, [1, 0]), Keeping::Active);

    keepers.ending_all();

    assert!(keepers.unsettled());
    assert!(!keepers.uncertain());
}

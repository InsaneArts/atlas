use super::*;
use crate::vm::lifecycle::tests::lifecycle;

#[test]
fn adds_and_removals_pass_together_while_no_uninstall_began() {
    let (_home, lifecycle) = lifecycle();

    let one = lifecycle.admit().unwrap();
    let two = lifecycle.admit().unwrap();

    drop((one, two));
    assert_eq!(lifecycle.barrier().unwrap(), None);
}

#[test]
fn an_uninstall_waits_for_an_add_under_way_and_an_add_for_an_uninstall() {
    let (_home, lifecycle) = lifecycle();
    let change = Token::new();

    let admitted = lifecycle.admit().unwrap();
    let busy = lifecycle.uninstalling(&change).unwrap_err();
    drop(admitted);
    let uninstalling = lifecycle.uninstalling(&change).unwrap();
    let refused = lifecycle.admit().unwrap_err();

    assert_eq!(busy.code, Code::Busy);
    assert!(
        refused.error.contains("being uninstalled"),
        "{}",
        refused.error
    );
    assert_eq!(uninstalling.change(), &change);
}

#[test]
fn a_vm_phase_cut_short_refuses_every_add_until_the_same_uninstall_commits_it() {
    let (_home, lifecycle) = lifecycle();
    let change = Token::new();
    drop(lifecycle.uninstalling(&change).unwrap());

    let refused = lifecycle.admit().unwrap_err();
    let other = lifecycle.uninstalling(&Token::new()).unwrap_err();
    let resumed = lifecycle.uninstalling(&change).unwrap();
    resumed.commit().unwrap();

    assert!(
        refused.error.contains("being uninstalled"),
        "{}",
        refused.error
    );
    assert!(other.error.contains("Another uninstall"), "{}", other.error);
    assert_eq!(
        lifecycle.barrier().unwrap(),
        Some(Barrier {
            change,
            phase: Phase::Committed,
        })
    );
    lifecycle.admit().unwrap();
}

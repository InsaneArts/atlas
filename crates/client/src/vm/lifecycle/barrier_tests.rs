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
    let busy = lifecycle.uninstalling(&change, &[]).unwrap_err();
    drop(admitted);
    let uninstalling = lifecycle.uninstalling(&change, &[]).unwrap();
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
    drop(lifecycle.uninstalling(&change, &[]).unwrap());

    let refused = lifecycle.admit().unwrap_err();
    let other = lifecycle.uninstalling(&Token::new(), &[]).unwrap_err();
    let resumed = lifecycle.resuming(&change).unwrap();
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
            vms: Vec::new(),
        })
    );
    lifecycle.admit().unwrap();
}

#[test]
fn an_uninstall_keeps_the_vms_it_was_allowed_and_goes_on_as_them() {
    let (_home, lifecycle) = lifecycle();
    let change = Token::new();
    let work = DesktopId::parse(&"a".repeat(24)).unwrap();
    drop(
        lifecycle
            .uninstalling(&change, std::slice::from_ref(&work))
            .unwrap(),
    );

    let resumed = lifecycle.resuming(&change).unwrap();

    assert_eq!(resumed.vms(), std::slice::from_ref(&work));
    assert_eq!(lifecycle.barrier().unwrap().unwrap().vms, [work]);
}

#[test]
fn an_uninstall_called_off_lets_every_add_in_and_another_is_allowed_anew() {
    let (_home, lifecycle) = lifecycle();
    let change = Token::new();
    drop(lifecycle.uninstalling(&change, &[]).unwrap());

    assert!(!lifecycle.call_off(&Token::new()).unwrap(), "another's");
    assert!(lifecycle.call_off(&change).unwrap());
    assert!(!lifecycle.call_off(&change).unwrap(), "called off once");

    assert_eq!(
        lifecycle.barrier().unwrap().unwrap().phase,
        Phase::Cancelled
    );
    lifecycle.admit().unwrap();
    let other = Token::new();
    let next = lifecycle.uninstalling(&other, &[]).unwrap();
    assert_eq!(
        next.change(),
        &other,
        "a new uninstall, not the one called off"
    );
}

#[test]
fn an_uninstall_cannot_be_called_off_while_another_holds_the_barrier() {
    let (_home, lifecycle) = lifecycle();
    let change = Token::new();
    let held = lifecycle.uninstalling(&change, &[]).unwrap();

    let busy = lifecycle.call_off(&change).unwrap_err();

    assert_eq!(busy.code, Code::Busy);
    drop(held);
    assert_eq!(lifecycle.barrier().unwrap().unwrap().phase, Phase::Vms);
}

#[test]
fn a_resume_goes_on_only_with_the_same_uninstall_still_in_its_vm_phase_and_writes_nothing() {
    let (_home, lifecycle) = lifecycle();
    let change = Token::new();
    drop(lifecycle.uninstalling(&change, &[]).unwrap());

    let other = lifecycle.resuming(&Token::new()).unwrap_err();
    assert_eq!(other.code, Code::State);
    assert!(lifecycle.call_off(&change).unwrap());
    let cancelled = lifecycle.resuming(&change).unwrap_err();

    assert!(
        cancelled.error.contains("called off"),
        "{}",
        cancelled.error
    );
    assert_eq!(
        lifecycle.barrier().unwrap().unwrap().phase,
        Phase::Cancelled,
        "never turned back into a VM phase"
    );
    lifecycle.admit().unwrap();
}

#[test]
fn a_committed_uninstall_keeps_adds_out_until_it_ends_though_its_folders_go() {
    let (home, lifecycle) = lifecycle();
    let change = Token::new();
    let closed = lifecycle
        .uninstalling(&change, &[])
        .unwrap()
        .commit()
        .unwrap();
    assert_eq!(
        lifecycle.barrier().unwrap().unwrap().phase,
        Phase::Committed
    );

    let refused = lifecycle.admit().unwrap_err();
    // The shared cleanup deletes Atlas's own folder, the barrier's with it.
    std::fs::remove_dir_all(home.path().join("Library/Application Support/Atlas")).unwrap();
    let still = lifecycle.admit().unwrap_err();
    drop(closed);

    assert!(
        refused.error.contains("being uninstalled"),
        "{}",
        refused.error
    );
    assert!(still.error.contains("being uninstalled"), "{}", still.error);
    lifecycle.admit().unwrap();
}

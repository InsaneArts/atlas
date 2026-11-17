use super::*;
use crate::vm::lifecycle::{
    record::{Record, State},
    tests::lifecycle,
};

fn kept(lifecycle: &Lifecycle, reservation: &Reservation, state: State) {
    let mut record = Record::adding(
        reservation,
        "/Users/someone/.config/atlas.toml".into(),
        crate::vm::lifecycle::tests::provenance(),
    );
    record.state = state;
    lifecycle.keep_record(&record).unwrap();
}

#[test]
fn a_name_the_settings_already_list_is_never_reserved() {
    let (_home, lifecycle) = lifecycle();

    let refused = lifecycle
        .reserve(Some("second-login"), &["second-login"])
        .unwrap_err();

    assert!(refused.error.contains("already list"), "{}", refused.error);
    assert_eq!(lifecycle.reservations().unwrap(), []);
}

#[test]
fn a_name_reserved_by_another_vm_is_refused_once_that_vm_is_added() {
    let (_home, lifecycle) = lifecycle();
    let first = lifecycle.reserve(Some("work"), &[]).unwrap();
    kept(&lifecycle, &first, State::Created);

    let refused = lifecycle.reserve(Some("work"), &[]).unwrap_err();

    assert!(refused.error.contains("already a VM"), "{}", refused.error);
}

#[test]
fn a_named_add_asked_again_resumes_the_vm_it_reserved() {
    let (_home, lifecycle) = lifecycle();
    let first = lifecycle.reserve(Some("work"), &[]).unwrap();
    let cut_short = lifecycle.reserve(Some("work"), &[]).unwrap();
    kept(&lifecycle, &first, State::Adding);
    let again = lifecycle.reserve(Some("work"), &["work"]).unwrap();

    assert!(!first.resumed);
    assert!(first.named);
    for resumed in [&cut_short, &again] {
        assert!(resumed.resumed);
        assert_eq!(resumed.id, first.id);
    }
}

#[test]
fn unnamed_adds_take_vm_then_vm_2_skipping_names_held_anywhere() {
    let (_home, lifecycle) = lifecycle();
    let first = lifecycle.reserve(None, &[]).unwrap();
    kept(&lifecycle, &first, State::Created);
    lifecycle.reserve(Some("vm-2"), &[]).unwrap();

    let next = lifecycle.reserve(None, &["vm-3"]).unwrap();

    assert_eq!(first.name, "vm");
    assert!(!first.named);
    assert_eq!(next.name, "vm-4");
    assert!(!next.resumed);
}

#[test]
fn an_unnamed_add_asked_again_resumes_the_unnamed_add_still_under_way() {
    let (_home, lifecycle) = lifecycle();
    let named = lifecycle.reserve(Some("work"), &[]).unwrap();
    kept(&lifecycle, &named, State::Adding);
    let unnamed = lifecycle.reserve(None, &[]).unwrap();
    kept(&lifecycle, &unnamed, State::Adding);

    let again = lifecycle.reserve(None, &[]).unwrap();

    assert!(again.resumed);
    assert_eq!(again.id, unnamed.id);
    assert_eq!(again.name, "vm");
}

#[test]
fn a_name_is_given_up_only_by_the_vm_that_holds_it() {
    let (_home, lifecycle) = lifecycle();
    let held = lifecycle.reserve(Some("work"), &[]).unwrap();
    let other = DesktopId::parse("89abcdef0123456789abcdef").unwrap();

    lifecycle.release("work", &other).unwrap();
    assert_eq!(
        lifecycle.reserved_by(&held.id).unwrap().as_deref(),
        Some("work")
    );
    lifecycle.release("work", &held.id).unwrap();

    assert_eq!(lifecycle.reserved_by(&held.id).unwrap(), None);
}

#[test]
fn a_name_that_is_not_a_desktop_name_is_refused() {
    let (_home, lifecycle) = lifecycle();

    assert!(lifecycle.reserve(Some("Work Mac"), &[]).is_err());
    assert!(lifecycle.reserve(Some("../x"), &[]).is_err());
}

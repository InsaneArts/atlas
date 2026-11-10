use super::*;

#[test]
fn copies_stay_refused_until_the_person_allows_them() {
    let home = tempfile::tempdir().unwrap();
    assert!(!allowed_in(home.path()));

    let refusal = set_in(home.path(), true, false).unwrap_err();
    assert_eq!(refusal.code, Code::Stopped);
    assert!(!allowed_in(home.path()));

    assert!(set_in(home.path(), true, true).unwrap());
    assert!(allowed_in(home.path()));
}

#[test]
fn anyone_turns_copies_off_and_turning_them_off_twice_is_fine() {
    let home = tempfile::tempdir().unwrap();
    assert!(set_in(home.path(), true, true).unwrap());

    assert!(!set_in(home.path(), false, false).unwrap());
    assert!(!set_in(home.path(), false, false).unwrap());
    assert!(!allowed_in(home.path()));
}

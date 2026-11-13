use super::*;

#[test]
fn process_existence_ignores_ownership() {
    assert!(process_exists(std::process::id()));
    assert!(process_exists(1), "launchd exists though owned by root");
    assert!(!process_exists(u32::MAX));
}

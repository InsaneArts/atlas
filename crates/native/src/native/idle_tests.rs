use super::*;

#[test]
#[ignore = "needs a graphical login on this Mac"]
fn macos_reports_every_idle_time_in_a_login() {
    let idle = Idle::now();
    assert!(idle.shared >= 0.0);
    assert!(idle.login >= 0.0);
    assert!(idle.hardware.is_some_and(|seconds| seconds >= 0.0));
}

#[test]
#[ignore = "needs a graphical login on this Mac"]
fn resetting_secure_typing_leaves_it_as_it_was() {
    let before = SecureTyping::now();
    assert!(before.is_some());
    reset_secure_typing();
    assert_eq!(SecureTyping::now(), before);
}

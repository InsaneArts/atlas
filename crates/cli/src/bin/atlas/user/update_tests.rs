use super::*;

#[test]
fn the_helper_has_a_job_to_move_once_its_setup_is_recorded_or_its_daemon_answers() {
    assert!(has_helper_job(true, false), "a recorded setup");
    assert!(has_helper_job(false, true), "a daemon that answers");
    assert!(!has_helper_job(false, false));
}

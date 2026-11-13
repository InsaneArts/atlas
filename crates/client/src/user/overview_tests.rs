use super::*;

#[test]
fn why_the_helper_is_silent_is_looked_at_again_within_seconds() {
    assert!(ABSENCE.fresh_for() <= Duration::from_secs(5));
    assert!(ABSENCE.fresh_for() < LOGINS.fresh_for());
}

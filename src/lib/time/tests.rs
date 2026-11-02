use super::{local_clock, ms, same_local_day};
use std::time::Duration;

#[test]
fn milliseconds_are_whole_and_saturate() {
    assert_eq!(ms(Duration::from_micros(2_999)), 2);
    assert_eq!(ms(Duration::MAX), u64::MAX);
}

#[test]
fn local_days_follow_midnight_rather_than_daylight_saving_clock_changes() {
    if !crate::test_support::in_timezone(
        "time::tests::local_days_follow_midnight_rather_than_daylight_saving_clock_changes",
        "America/New_York",
    ) {
        return;
    }
    // The spring day has 23 hours. Both sides of the skipped hour are
    // still that day's input; only its two midnights separate days.
    assert!(!same_local_day(1_710_046_799_000, 1_710_046_800_000));
    assert!(same_local_day(1_710_053_999_000, 1_710_054_000_000));
    assert!(!same_local_day(1_710_129_599_000, 1_710_129_600_000));
    assert_eq!(local_clock(1_710_053_999), Some((1, 59, 59)));
    assert_eq!(local_clock(1_710_054_000), Some((3, 0, 0)));
    // Repeating an hour in autumn does not start a new day either.
    assert!(same_local_day(1_730_613_599_000, 1_730_613_600_000));
}

#[test]
fn local_days_also_separate_the_last_day_of_one_year_from_the_next() {
    if !crate::test_support::in_timezone(
        "time::tests::local_days_also_separate_the_last_day_of_one_year_from_the_next",
        "UTC",
    ) {
        return;
    }
    assert!(!same_local_day(1_735_689_599_000, 1_735_689_600_000));
    assert!(same_local_day(1_735_689_600_000, 1_735_689_601_000));
}

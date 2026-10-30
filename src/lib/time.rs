//! Times as Atlas records and sends them.
use std::time::Duration;

/// `duration` in whole milliseconds, or the most a `u64` holds for one
/// longer than that.
pub(crate) fn ms(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

/// Whether two timestamps belong to the same local calendar day, as the
/// viewer groups them. Unknown dates stay separate rather than losing a
/// day's attribution when input is gathered.
pub(crate) fn same_local_day(earlier_ms: u64, later_ms: u64) -> bool {
    let day = |ms| {
        let local = local_time(i64::try_from(ms / 1000).ok()?)?;
        Some((local.tm_year, local.tm_yday))
    };
    day(earlier_ms)
        .zip(day(later_ms))
        .is_some_and(|(earlier, later)| earlier == later)
}

/// The local hour, minute and second of `seconds` since 1970.
pub(crate) fn local_clock(seconds: i64) -> Option<(u8, u8, u8)> {
    let local = local_time(seconds)?;
    Some((
        u8::try_from(local.tm_hour).ok()?,
        u8::try_from(local.tm_min).ok()?,
        u8::try_from(local.tm_sec).ok()?,
    ))
}

#[allow(unsafe_code, reason = "FFI: localtime_r has no safe binding")]
fn local_time(seconds: i64) -> Option<libc::tm> {
    let time: libc::time_t = seconds;
    // SAFETY: tm holds only integers and a pointer, for which zeroes are valid.
    let mut local: libc::tm = unsafe { std::mem::zeroed() };
    // SAFETY: both pointers are live and aligned; localtime_r only writes local.
    let filled = unsafe { libc::localtime_r(&time, &mut local) };
    (!filled.is_null()).then_some(local)
}

#[cfg(test)]
mod tests {
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
}

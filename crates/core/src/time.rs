//! Times as Atlas records and sends them.
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// `duration` in whole milliseconds, or the most a `u64` holds for one
/// longer than that.
pub fn ms(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

/// Whether two timestamps belong to the same local calendar day, as the
/// viewer groups them. Unknown dates stay separate rather than losing a
/// day's attribution when input is gathered.
pub fn same_local_day(earlier_ms: u64, later_ms: u64) -> bool {
    let day = |ms| {
        let local = local_time(i64::try_from(ms / 1000).ok()?)?;
        Some((local.tm_year, local.tm_yday))
    };
    day(earlier_ms)
        .zip(day(later_ms))
        .is_some_and(|(earlier, later)| earlier == later)
}

/// The local hour, minute and second of `seconds` since 1970.
pub fn local_clock(seconds: i64) -> Option<(u8, u8, u8)> {
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

/// Milliseconds since 1970, as records keep times.
pub fn now_ms() -> u64 {
    u64::try_from(timestamp() / 1_000_000).unwrap_or(u64::MAX)
}

pub fn timestamp() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}

#[cfg(test)]
#[path = "time_tests.rs"]
mod tests;

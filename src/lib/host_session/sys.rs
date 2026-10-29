//! The one system call host sessions need that has no safe binding: the
//! local time of day.

/// The local hour, minute and second of `seconds` since 1970.
pub(super) fn local_clock(seconds: i64) -> Option<(u8, u8, u8)> {
    let time: libc::time_t = seconds;
    // SAFETY: tm holds only integers and a pointer, for which all zeroes (a
    // null pointer) is a valid value.
    let mut local: libc::tm = unsafe { std::mem::zeroed() };
    // SAFETY: both pointers are to valid, live, properly aligned values, and
    // localtime_r writes only `local`.
    let filled = unsafe { libc::localtime_r(&time, &mut local) };
    if filled.is_null() {
        return None;
    }

    Some((
        u8::try_from(local.tm_hour).ok()?,
        u8::try_from(local.tm_min).ok()?,
        u8::try_from(local.tm_sec).ok()?,
    ))
}

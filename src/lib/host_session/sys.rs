//! The two system calls host sessions need that have no safe binding: one
//! process's facts, and the local time of day.
use super::ancestry::{Facts, Lookup};

/// What macOS says of `pid`: its parent, command name, exact start and
/// whether it is stopped; that no such process runs; or nothing it would
/// say, as for another account's process.
pub(super) fn process(pid: i32) -> Lookup {
    let Ok(size) = libc::c_int::try_from(std::mem::size_of::<libc::proc_bsdinfo>()) else {
        return Lookup::Unknown;
    };
    // SAFETY: proc_bsdinfo holds only integers and byte arrays, for which
    // all zeroes is a valid value.
    let mut info: libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
    // SAFETY: `info` is a valid, writable proc_bsdinfo of the size passed,
    // and the kernel writes at most that many bytes.
    let written =
        unsafe { libc::proc_pidinfo(pid, libc::PROC_PIDTBSDINFO, 0, (&raw mut info).cast(), size) };
    if written != size {
        let gone =
            written <= 0 && std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH);
        return if gone { Lookup::Gone } else { Lookup::Unknown };
    }

    let command: Vec<u8> = info
        .pbi_comm
        .iter()
        .map(|character| character.to_ne_bytes()[0])
        .take_while(|byte| *byte != 0)
        .collect();
    let Ok(parent) = i32::try_from(info.pbi_ppid) else {
        return Lookup::Unknown;
    };

    Lookup::Found(Facts {
        parent,
        command: String::from_utf8_lossy(&command).into_owned(),
        start: [info.pbi_start_tvsec, info.pbi_start_tvusec],
        stopped: info.pbi_status == libc::SSTOP,
    })
}

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

//! Read a child's exit without releasing its PID or process-group reservation.
use std::io;

/// Whether this process's child `pid` has exited, without collecting its
/// status. The caller keeps the child owned and waits only after its last
/// group signal, so an exited leader still reserves the group's number.
///
/// # Errors
///
/// Returns invalid input for a nonpositive or out-of-range PID, or the
/// kernel's error when it cannot observe this child. No child is reaped.
#[allow(unsafe_code, reason = "nix does not expose waitid on macOS")]
pub fn exited(pid: u32) -> io::Result<bool> {
    let expected = i32::try_from(pid)
        .ok()
        .filter(|pid| *pid > 0)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "Invalid child process identity",
            )
        })?;
    loop {
        // SAFETY: an all-zero siginfo_t is valid initialized stack storage.
        let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
        // SAFETY: pid is positive and in range. info is initialized, aligned
        // writable storage. The kernel verifies child ownership; WNOWAIT
        // leaves its status and PID reserved. Both return and si_pid are checked.
        let result = unsafe {
            libc::waitid(
                libc::P_PID,
                pid,
                &mut info,
                libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
            )
        };
        if result == -1 {
            let error = io::Error::last_os_error();
            if error.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return Err(error);
        }
        // SAFETY: waitid succeeded and initialized siginfo_t storage.
        return match unsafe { info.si_pid() } {
            0 => Ok(false),
            found if found == expected => Ok(true),
            _ => Err(io::Error::other("Unexpected child process identity")),
        };
    }
}

#[cfg(test)]
#[path = "child_tests.rs"]
mod tests;

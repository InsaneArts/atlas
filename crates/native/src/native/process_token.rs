//! One process by the kernel's own name for it: its PID together with the
//! version the kernel gave this use of that PID. A PID alone can come to
//! name another process between a look and a signal; what is read or
//! signalled through a token reaches that very process or nothing, as the
//! kernel itself refuses a token whose process is gone.
//!
//! A token is had only for a process of this account.
#![allow(
    unsafe_code,
    reason = "FFI: Mach task info, libbsm and libproc's token calls have no safe binding"
)]
use nix::sys::signal::Signal;
use std::{
    ffi::{OsString, c_void},
    io,
    os::unix::ffi::OsStringExt,
    path::PathBuf,
    sync::OnceLock,
    time::Duration,
};

/// `audit_token_t` of `mach/message.h`.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct AuditToken {
    val: [u32; 8],
}

/// `mach_timebase_info_data_t` of `mach/mach_time.h`.
#[repr(C)]
#[derive(Clone, Copy)]
struct Timebase {
    numer: u32,
    denom: u32,
}

/// `TASK_AUDIT_TOKEN` of `mach/task_info.h`, and the words it fills.
const TASK_AUDIT_TOKEN: u32 = 15;
const TASK_AUDIT_TOKEN_COUNT: u32 = 8;

unsafe extern "C" {
    /// This task's own port, as `mach_task_self()` reads it.
    static mach_task_self_: u32;
    fn task_name_for_pid(task: u32, pid: libc::c_int, name: *mut u32) -> libc::c_int;
    fn task_info(task: u32, flavor: u32, info: *mut u32, count: *mut u32) -> libc::c_int;
    fn mach_port_deallocate(task: u32, name: u32) -> libc::c_int;
    fn mach_absolute_time() -> u64;
    fn mach_timebase_info(info: *mut Timebase) -> libc::c_int;
    fn proc_pidpath_audittoken(
        token: *mut AuditToken,
        buffer: *mut c_void,
        size: u32,
    ) -> libc::c_int;
    fn proc_signal_with_audittoken(token: *mut AuditToken, signal: libc::c_int) -> libc::c_int;
}

#[link(name = "bsm")]
unsafe extern "C" {
    fn audit_token_to_euid(token: AuditToken) -> libc::uid_t;
    fn audit_token_to_pid(token: AuditToken) -> libc::pid_t;
    fn audit_token_to_asid(token: AuditToken) -> libc::pid_t;
}

/// The kernel's name for one process: this use of its PID, and no later one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Token(AuditToken);

impl Token {
    /// The token of process `pid` as it is now.
    ///
    /// # Errors
    ///
    /// Returns an error when the kernel gives none: the process is gone, or
    /// is another account's.
    pub fn of(pid: i32) -> io::Result<Self> {
        let mut name = 0;
        // SAFETY: this task's own port is always valid, and `name` is
        // writable for the one port name the call returns.
        let found = unsafe { task_name_for_pid(mach_task_self_, pid, &mut name) };
        if found != 0 {
            return Err(io::Error::other(format!(
                "The kernel names no such process (task_name_for_pid: {found})"
            )));
        }

        let mut token = AuditToken { val: [0; 8] };
        let mut count = TASK_AUDIT_TOKEN_COUNT;
        // SAFETY: `name` is the task name just returned, and the token has
        // room for the eight words `count` declares.
        let read = unsafe { task_info(name, TASK_AUDIT_TOKEN, token.val.as_mut_ptr(), &mut count) };
        // SAFETY: the lookup returned exactly one right, released here on
        // every path. A failure to release it leaks one name and no more.
        let _ = unsafe { mach_port_deallocate(mach_task_self_, name) };
        if read != 0 {
            return Err(io::Error::other(format!(
                "The kernel gave no token for the process (task_info: {read})"
            )));
        }
        if count != TASK_AUDIT_TOKEN_COUNT {
            return Err(io::Error::other(format!(
                "The kernel's token has {count} words, not {TASK_AUDIT_TOKEN_COUNT}"
            )));
        }

        let token = Self(token);
        if token.pid() != pid {
            return Err(io::Error::other("The token is another process's"));
        }
        Ok(token)
    }

    /// Its PID.
    pub fn pid(&self) -> i32 {
        // SAFETY: libbsm only decodes the token it is given by value.
        unsafe { audit_token_to_pid(self.0) }
    }

    /// Its effective user ID.
    pub fn uid(&self) -> u32 {
        // SAFETY: libbsm only decodes the token it is given by value.
        unsafe { audit_token_to_euid(self.0) }
    }

    /// The login session it belongs to, as the kernel's audit records name
    /// it: the same for every process of one login.
    pub fn session(&self) -> i32 {
        // SAFETY: libbsm only decodes the token it is given by value.
        unsafe { audit_token_to_asid(self.0) }
    }

    /// The program that very process runs.
    ///
    /// # Errors
    ///
    /// Returns an error when the process is gone or the kernel does not say.
    pub fn program(&self) -> io::Result<PathBuf> {
        let mut token = self.0;
        let mut buffer = vec![0_u8; libc::PROC_PIDPATHINFO_MAXSIZE as usize];
        let size = u32::try_from(buffer.len()).map_err(io::Error::other)?;
        // SAFETY: the token is a live copy and the buffer is writable for
        // the length passed.
        let length =
            unsafe { proc_pidpath_audittoken(&mut token, buffer.as_mut_ptr().cast(), size) };
        let length = usize::try_from(length)
            .ok()
            .filter(|length| (1..=buffer.len()).contains(length))
            .ok_or_else(io::Error::last_os_error)?;
        buffer.truncate(length);
        Ok(PathBuf::from(OsString::from_vec(buffer)))
    }

    /// Send `signal` to that very process. The kernel checks the sender as
    /// for any signal, and refuses a token whose process is gone: no other
    /// process is ever reached.
    ///
    /// # Errors
    ///
    /// Returns the kernel's refusal.
    pub fn signal(&self, signal: Signal) -> io::Result<()> {
        let mut token = self.0;
        // SAFETY: the token is a live copy; the call only reads it.
        match unsafe { proc_signal_with_audittoken(&mut token, signal as libc::c_int) } {
            0 => Ok(()),
            error => Err(io::Error::from_raw_os_error(error)),
        }
    }

    /// How long ago that very process started, by the kernel's own count
    /// of time since boot: no setting of the clock moves it.
    ///
    /// # Errors
    ///
    /// Returns an error when the kernel does not say, or the process has
    /// gone while it was asked.
    pub fn age(&self) -> io::Result<Duration> {
        let pid = self.pid();
        let age = age(pid)?;
        // The age is read by PID: the same token after it shows that the
        // PID named this process all along.
        if Self::of(pid)? != *self {
            return Err(io::Error::other(
                "The process was gone while its age was read",
            ));
        }
        Ok(age)
    }

    /// A token as the kernel would give it for `pid` in its `version`-th
    /// use, as the account `uid` in the login `session`: for fakes.
    #[cfg(any(test, feature = "test-support"))]
    pub fn made_up(pid: i32, version: u32, uid: u32, session: i32) -> Self {
        Self(AuditToken {
            val: [
                uid,
                uid,
                0,
                uid,
                0,
                pid.cast_unsigned(),
                session.cast_unsigned(),
                version,
            ],
        })
    }
}

/// How long ago process `pid` started, by the kernel's own count of time
/// since boot: no setting of the clock moves it. Read by PID, so
/// [`Token::age`] reads the process's token again after it.
fn age(pid: i32) -> io::Result<Duration> {
    let mut usage = std::mem::MaybeUninit::<libc::rusage_info_v4>::zeroed();
    // SAFETY: for RUSAGE_INFO_V4 the kernel writes at most one
    // rusage_info_v4 into the buffer, which is exactly that size and aligned
    // for it; the buffer is read only after the call reports success.
    let status =
        unsafe { libc::proc_pid_rusage(pid, libc::RUSAGE_INFO_V4, usage.as_mut_ptr().cast()) };
    if status != 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: the call succeeded, so the structure is initialized; it was
    // zeroed before, so no field is left undefined either way.
    let started = unsafe { usage.assume_init() }.ri_proc_start_abstime;
    // SAFETY: no arguments; reads the kernel's count.
    let now = unsafe { mach_absolute_time() };

    let timebase = timebase()?;
    let ticks = u128::from(now.saturating_sub(started));
    let nanos = ticks * u128::from(timebase.numer) / u128::from(timebase.denom);
    Ok(Duration::from_nanos(
        u64::try_from(nanos).unwrap_or(u64::MAX),
    ))
}

/// How the kernel's count converts to nanoseconds, read once.
fn timebase() -> io::Result<Timebase> {
    /// The rate, or the status the kernel refused it with.
    static TIMEBASE: OnceLock<Result<Timebase, libc::c_int>> = OnceLock::new();
    let timebase = TIMEBASE
        .get_or_init(|| {
            let mut timebase = Timebase { numer: 0, denom: 0 };
            // SAFETY: the structure is writable for the two words the call
            // fills.
            match unsafe { mach_timebase_info(&mut timebase) } {
                0 => Ok(timebase),
                status => Err(status),
            }
        })
        .map_err(|status| {
            io::Error::other(format!(
                "The kernel's clock has no known rate (mach_timebase_info: {status})"
            ))
        })?;
    if timebase.denom == 0 {
        return Err(io::Error::other("The kernel's clock rate divides by zero"));
    }
    Ok(timebase)
}

#[cfg(test)]
#[path = "process_token_tests.rs"]
mod tests;

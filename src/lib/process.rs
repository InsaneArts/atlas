//! What macOS says about a process: when it started, whose it is, its group
//! and parent. A process ID is never trusted on its own: the same ID with
//! another start time is another process.
//!
//! Everything is read through the `KERN_PROC` sysctls, which report any
//! account's process and a process that has exited but is not yet reaped,
//! with its exact start time. `proc_pidinfo` refuses both; [`own`] keeps
//! its rule, for what describes only this account's live processes.
use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use std::ffi::CStr;

/// When a process started: seconds and microseconds since 1970, as the
/// kernel keeps them.
pub type Start = [u64; 2];

/// One process instance: the same PID with another start time is another
/// process.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Instance {
    pub pid: i32,
    pub start: Start,
}

impl Instance {
    pub fn new(pid: i32, start: Start) -> Self {
        Self { pid, start }
    }

    /// This process, when macOS describes it.
    pub fn current() -> Option<Self> {
        let pid = i32::try_from(std::process::id()).ok()?;
        match own(pid) {
            Own::Found(info) => Some(Self::new(pid, info.instance.start)),
            Own::Gone | Own::Unknown => None,
        }
    }

    /// How this very process is now, as this account may know it.
    pub fn state(&self) -> State {
        match own(self.pid) {
            Own::Found(info) if info.instance.start != self.start => State::Gone,
            Own::Found(info) if info.stopped => State::Stopped,
            Own::Found(_) => State::Running,
            Own::Gone => State::Gone,
            Own::Unknown => State::Unknown,
        }
    }

    /// Whether this very process still runs, stopped or not: its PID lives
    /// with the same start time.
    pub fn running(&self) -> bool {
        matches!(self.state(), State::Running | State::Stopped)
    }

    /// When it started, in milliseconds since 1970.
    pub fn started_ms(&self) -> u64 {
        self.start[0]
            .saturating_mul(1000)
            .saturating_add(self.start[1] / 1000)
    }

    /// Whether this is the process asking.
    pub fn is_this_one(&self) -> bool {
        u32::try_from(self.pid).is_ok_and(|pid| pid == std::process::id())
    }
}

/// How an [`Instance`] is now, as this account may know it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Running,
    /// Suspended, as by SIGSTOP: it runs again only once continued.
    Stopped,
    /// No such process runs: its PID is free, or another process's, or it
    /// has exited and waits to be reaped.
    Gone,
    /// macOS would not say, as for another account's process.
    Unknown,
}

/// What looking up a PID as this account found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Own {
    /// A live process this account may describe.
    Found(Info),
    /// No live process has that PID.
    Gone,
    /// It is not this account's to describe, or macOS would not say.
    Unknown,
}

/// `pid` as this account may describe it: a process that has exited,
/// reaped or not, is gone, and another account's process (the kernel's own
/// among them) is not described unless this runs as root, by the rule
/// `proc_pidinfo` keeps.
pub(crate) fn own(pid: i32) -> Own {
    if pid == 0 {
        return Own::Unknown;
    }
    let euid = nix::unistd::geteuid().as_raw();
    match info(pid) {
        Ok(Some(info)) if info.zombie => Own::Gone,
        Ok(Some(info)) if euid != 0 && info.uid != euid => Own::Unknown,
        Ok(Some(info)) => Own::Found(info),
        Ok(None) => Own::Gone,
        Err(_) => Own::Unknown,
    }
}

/// What the kernel says of one process now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Info {
    pub instance: Instance,
    pub ppid: i32,
    pub pgid: i32,
    /// Its effective user ID.
    pub uid: u32,
    /// Its real user ID.
    pub ruid: u32,
    /// Its saved user ID.
    pub svuid: u32,
    /// It has exited and waits for its parent to collect its status.
    pub zombie: bool,
    /// It is suspended, as by SIGSTOP.
    pub stopped: bool,
    /// Its command name, as the kernel keeps it (at most 16 bytes).
    pub name: String,
}

impl Info {
    /// Whether the account `uid` may signal it, by the kernel's own rule
    /// for a sender that is not root: its real or saved user ID is `uid`.
    /// A set-user-ID program `uid` started is still its own; a process that
    /// has exited belongs to no account any more.
    pub fn owned_by(&self, uid: u32) -> bool {
        self.zombie || self.ruid == uid || self.svuid == uid
    }
}

/// `struct kinfo_proc` of `sys/sysctl.h`, laid out as the 64-bit macOS SDK
/// declares it on arm64 and x86_64 alike, so the fields read below are found
/// by `offset_of!` rather than by hand. Only its layout is used: records
/// are read from the kernel's bytes, never through a reference to one.
#[repr(C)]
#[allow(dead_code, reason = "a layout mirror: only offsets and size are read")]
struct KinfoProc {
    kp_proc: ExternProc,
    kp_eproc: Eproc,
}

/// `struct extern_proc` of `sys/proc.h`.
#[repr(C)]
#[allow(dead_code, reason = "a layout mirror: only offsets and size are read")]
struct ExternProc {
    /// A union of two pointers and `p_starttime`, a timeval.
    p_starttime: libc::timeval,
    p_vmspace: *mut libc::c_void,
    p_sigacts: *mut libc::c_void,
    p_flag: libc::c_int,
    p_stat: libc::c_char,
    p_pid: libc::pid_t,
    p_oppid: libc::pid_t,
    p_dupfd: libc::c_int,
    user_stack: *mut libc::c_char,
    exit_thread: *mut libc::c_void,
    p_debugger: libc::c_int,
    sigwait: libc::c_int,
    p_estcpu: libc::c_uint,
    p_cpticks: libc::c_int,
    p_pctcpu: u32,
    p_wchan: *mut libc::c_void,
    p_wmesg: *mut libc::c_char,
    p_swtime: libc::c_uint,
    p_slptime: libc::c_uint,
    p_realtimer: [libc::timeval; 2],
    p_rtime: libc::timeval,
    p_uticks: u64,
    p_sticks: u64,
    p_iticks: u64,
    p_traceflag: libc::c_int,
    p_tracep: *mut libc::c_void,
    p_siglist: libc::c_int,
    p_textvp: *mut libc::c_void,
    p_holdcnt: libc::c_int,
    p_sigmask: u32,
    p_sigignore: u32,
    p_sigcatch: u32,
    p_priority: u8,
    p_usrpri: u8,
    p_nice: libc::c_char,
    p_comm: [libc::c_char; COMM_LEN],
    p_pgrp: *mut libc::c_void,
    p_addr: *mut libc::c_void,
    p_xstat: u16,
    p_acflag: u16,
    p_ru: *mut libc::c_void,
}

/// `struct eproc` of `sys/sysctl.h`.
#[repr(C)]
#[allow(dead_code, reason = "a layout mirror: only offsets and size are read")]
struct Eproc {
    e_paddr: *mut libc::c_void,
    e_sess: *mut libc::c_void,
    e_pcred: Pcred,
    e_ucred: Ucred,
    e_vm: Vmspace,
    e_ppid: libc::pid_t,
    e_pgid: libc::pid_t,
    e_jobc: i16,
    e_tdev: i32,
    e_tpgid: libc::pid_t,
    e_tsess: *mut libc::c_void,
    e_wmesg: [libc::c_char; 8],
    e_xsize: i32,
    e_xrssize: i16,
    e_xccount: i16,
    e_xswrss: i16,
    e_flag: i32,
    e_login: [libc::c_char; 12],
    e_spare: [i32; 4],
}

/// `struct _pcred` of `sys/sysctl.h`.
#[repr(C)]
#[allow(dead_code, reason = "a layout mirror: only offsets and size are read")]
struct Pcred {
    pc_lock: [libc::c_char; 72],
    pc_ucred: *mut libc::c_void,
    p_ruid: libc::uid_t,
    p_svuid: libc::uid_t,
    p_rgid: libc::gid_t,
    p_svgid: libc::gid_t,
    p_refcnt: libc::c_int,
}

/// `struct _ucred` of `sys/sysctl.h`.
#[repr(C)]
#[allow(dead_code, reason = "a layout mirror: only offsets and size are read")]
struct Ucred {
    cr_ref: i32,
    cr_uid: libc::uid_t,
    cr_ngroups: i16,
    cr_groups: [libc::gid_t; 16],
}

/// `struct vmspace` of `sys/sysctl.h`, the four dummy fields.
#[repr(C)]
#[allow(dead_code, reason = "a layout mirror: only offsets and size are read")]
struct Vmspace {
    dummy: i32,
    dummy2: *mut libc::c_char,
    dummy3: [i32; 5],
    dummy4: [*mut libc::c_char; 3],
}

/// The length of `p_comm`: MAXCOMLEN and its NUL.
const COMM_LEN: usize = 17;
/// One `struct kinfo_proc`, and where each field read lies in it.
const KINFO_PROC: usize = std::mem::size_of::<KinfoProc>();
const START: usize = std::mem::offset_of!(KinfoProc, kp_proc.p_starttime);
const STAT: usize = std::mem::offset_of!(KinfoProc, kp_proc.p_stat);
const PID: usize = std::mem::offset_of!(KinfoProc, kp_proc.p_pid);
const COMM: usize = std::mem::offset_of!(KinfoProc, kp_proc.p_comm);
const RUID: usize = std::mem::offset_of!(KinfoProc, kp_eproc.e_pcred.p_ruid);
const SVUID: usize = std::mem::offset_of!(KinfoProc, kp_eproc.e_pcred.p_svuid);
const UID: usize = std::mem::offset_of!(KinfoProc, kp_eproc.e_ucred.cr_uid);
const PPID: usize = std::mem::offset_of!(KinfoProc, kp_eproc.e_ppid);
const PGID: usize = std::mem::offset_of!(KinfoProc, kp_eproc.e_pgid);
/// The SDK's own `sizeof` and `offsetof` on both 64-bit Mac architectures,
/// measured once against `<sys/sysctl.h>`: the mirror must agree.
const _: () = {
    assert!(KINFO_PROC == 648);
    assert!(START == 0);
    assert!(STAT == 36);
    assert!(PID == 40);
    assert!(COMM == 243);
    assert!(RUID == 392);
    assert!(SVUID == 396);
    assert!(UID == 420);
    assert!(PPID == 560);
    assert!(PGID == 564);
    assert!(std::mem::size_of::<libc::timeval>() == 16);
};
/// `p_stat` of a process that is suspended.
const SSTOP: u8 = 4;
/// `p_stat` of a process that has exited and is not yet reaped.
const SZOMB: u8 = 5;

/// This boot's identity, `kern.bootsessionuuid`: unaffected by changes to
/// the clock.
pub fn boot() -> Result<String> {
    boot_named(c"kern.bootsessionuuid")
}

#[allow(unsafe_code, reason = "FFI: kern.bootsessionuuid has no safe binding")]
fn boot_named(name: &CStr) -> Result<String> {
    let mut boot = [0_u8; 37];
    let mut size = std::mem::size_of_val(&boot);
    // SAFETY: NUL-terminated name and writable buffer of the declared size; no input buffer.
    let status = unsafe {
        libc::sysctlbyname(
            name.as_ptr(),
            boot.as_mut_ptr().cast(),
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    };
    if status != 0 || size != std::mem::size_of_val(&boot) {
        return Err(std::io::Error::last_os_error().into());
    }
    CStr::from_bytes_with_nul(&boot)
        .ok()
        .and_then(|boot| boot.to_str().ok())
        .map(str::to_owned)
        .ok_or_else(|| Error::new("state", "Invalid boot session UUID"))
}

/// The exact start time of any account's process, an unreaped one
/// included. A process that does not exist is an error.
pub fn start(pid: i32) -> Result<Start> {
    info(pid)?
        .map(|info| info.instance.start)
        .ok_or_else(|| Error::new("state", "The process's start time is unavailable"))
}

/// `pid` now, whoever's it is; None when no such process exists.
pub fn info(pid: i32) -> Result<Option<Info>> {
    if pid <= 0 {
        return Ok(None);
    }
    let mut found = listed(&mut [libc::CTL_KERN, libc::KERN_PROC, libc::KERN_PROC_PID, pid])?;
    Ok(found.pop().filter(|info| info.instance.pid == pid))
}

/// Every process in group `pgid`, unreaped ones included.
pub fn group(pgid: i32) -> Result<Vec<Info>> {
    if pgid <= 0 {
        return Ok(Vec::new());
    }
    Ok(
        listed(&mut [libc::CTL_KERN, libc::KERN_PROC, libc::KERN_PROC_PGRP, pgid])?
            .into_iter()
            .filter(|info| info.pgid == pgid)
            .collect(),
    )
}

/// Every process whose effective user ID is `uid`.
pub fn of_user(uid: u32) -> Result<Vec<Info>> {
    let id = libc::c_int::try_from(uid)
        .map_err(|_| Error::new("state", "The account's user ID is out of range"))?;
    Ok(
        listed(&mut [libc::CTL_KERN, libc::KERN_PROC, libc::KERN_PROC_UID, id])?
            .into_iter()
            .filter(|info| info.uid == uid)
            .collect(),
    )
}

/// The session `pid` belongs to; None when it cannot be read, as for a
/// process that has exited.
pub fn session(pid: i32) -> Option<i32> {
    nix::unistd::getsid(Some(nix::unistd::Pid::from_raw(pid)))
        .ok()
        .map(nix::unistd::Pid::as_raw)
}

/// The processes the `KERN_PROC` query `name` lists.
#[allow(
    unsafe_code,
    reason = "FFI: the KERN_PROC sysctls have no safe binding"
)]
fn listed(name: &mut [libc::c_int]) -> Result<Vec<Info>> {
    let length = libc::c_uint::try_from(name.len())
        .map_err(|_| Error::new("state", "A process query is malformed"))?;
    // The list can grow between asking its size and reading it: room for a
    // few more is asked for, and a list that still outgrew it is read again.
    for _ in 0..4 {
        let mut size = 0;
        // SAFETY: the MIB is valid for its length; a null output asks only its
        // size, which the kernel writes back through `size` (in and out).
        let status = unsafe {
            libc::sysctl(
                name.as_mut_ptr(),
                length,
                std::ptr::null_mut(),
                &mut size,
                std::ptr::null_mut(),
                0,
            )
        };
        if status != 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        if size == 0 {
            return Ok(Vec::new());
        }
        let mut buffer = vec![0_u8; size + 16 * KINFO_PROC];
        let mut size = buffer.len();
        // SAFETY: the MIB is valid and the buffer has the writable capacity
        // `size` declares on the way in; the kernel writes back how much it
        // filled through the same `size`.
        let status = unsafe {
            libc::sysctl(
                name.as_mut_ptr(),
                length,
                buffer.as_mut_ptr().cast(),
                &mut size,
                std::ptr::null_mut(),
                0,
            )
        };
        if status != 0 {
            let error = std::io::Error::last_os_error();
            if error.raw_os_error() == Some(libc::ENOMEM) {
                continue;
            }
            return Err(error.into());
        }
        if size > buffer.len() || !size.is_multiple_of(KINFO_PROC) {
            return Err(Error::new("state", "The process list is malformed"));
        }
        // A record that does not decode is an error, never a process left
        // out: a missing process could read as one that ended.
        return buffer[..size]
            .chunks_exact(KINFO_PROC)
            .map(|record| {
                parse(record)
                    .ok_or_else(|| Error::new("state", "A process record could not be decoded"))
            })
            .collect();
    }
    Err(Error::new(
        "state",
        "The process list kept changing while it was read",
    ))
}

/// One `struct kinfo_proc`, when its start time is a real one.
fn parse(record: &[u8]) -> Option<Info> {
    let int = |at: usize| {
        record
            .get(at..at + 4)
            .and_then(|bytes| bytes.try_into().ok())
            .map(i32::from_ne_bytes)
    };
    let unsigned = |at: usize| int(at).map(|value| value as u32);
    let seconds = record
        .get(START..START + 8)
        .and_then(|bytes| bytes.try_into().ok())
        .map(i64::from_ne_bytes)?;
    let micros = int(START + 8)?;
    if seconds <= 0 || !(0..1_000_000).contains(&micros) {
        return None;
    }

    let comm = record.get(COMM..COMM + COMM_LEN)?;
    let end = comm
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(comm.len());
    Some(Info {
        instance: Instance {
            pid: int(PID)?,
            start: [seconds as u64, micros as u64],
        },
        ppid: int(PPID)?,
        pgid: int(PGID)?,
        uid: unsigned(UID)?,
        ruid: unsigned(RUID)?,
        svuid: unsigned(SVUID)?,
        zombie: *record.get(STAT)? == SZOMB,
        stopped: *record.get(STAT)? == SSTOP,
        name: String::from_utf8_lossy(&comm[..end]).into_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::process::CommandExt;

    #[test]
    fn this_process_reads_as_the_kernel_reports_it() {
        let me = info(std::process::id() as i32).unwrap().unwrap();
        assert_eq!(me.instance.pid, std::process::id() as i32);
        assert_eq!(me.ppid, nix::unistd::getppid().as_raw());
        assert_eq!(me.pgid, nix::unistd::getpgrp().as_raw());
        assert_eq!(me.uid, nix::unistd::geteuid().as_raw());
        assert_eq!(me.ruid, nix::unistd::getuid().as_raw());
        assert_eq!(me.svuid, nix::unistd::geteuid().as_raw());
        assert!(me.owned_by(me.ruid));
        assert!(!me.zombie);
        let executable = std::env::current_exe().unwrap();
        let file = executable.file_name().unwrap().to_string_lossy();
        assert!(file.starts_with(&me.name), "{file} {}", me.name);
    }

    #[test]
    fn a_process_is_known_by_its_start_time_not_its_pid_alone() {
        let mut child = std::process::Command::new("/bin/sleep")
            .arg("5")
            .spawn()
            .unwrap();
        let pid = child.id() as i32;
        let known = info(pid).unwrap().unwrap();
        assert_eq!(known.instance.start, start(pid).unwrap());
        let other = Instance {
            pid,
            start: [known.instance.start[0], known.instance.start[1] ^ 1],
        };
        assert_ne!(known.instance, other);
        child.kill().unwrap();
        child.wait().unwrap();
    }

    #[test]
    fn an_unreaped_child_is_read_as_a_zombie_with_its_start_time() {
        let mut child = std::process::Command::new("/usr/bin/true").spawn().unwrap();
        let pid = child.id() as i32;
        let started = start(pid).unwrap();
        let mut zombie = None;
        for _ in 0..200 {
            zombie = info(pid).unwrap().filter(|info| info.zombie);
            if zombie.is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let zombie = zombie.expect("the child exited");
        assert_eq!(zombie.instance.start, started);
        child.wait().unwrap();
        assert_eq!(info(pid).unwrap(), None);
    }

    #[test]
    fn another_accounts_process_is_read_and_a_reaped_one_is_absent() {
        let launchd = info(1).unwrap().unwrap();
        assert_eq!((launchd.uid, launchd.ruid), (0, 0));
        assert!(!launchd.owned_by(nix::unistd::getuid().as_raw()));

        let mut child = std::process::Command::new("/usr/bin/true").spawn().unwrap();
        let pid = child.id() as i32;
        child.wait().unwrap();
        assert_eq!(info(pid).unwrap(), None);
        assert!(start(pid).is_err());
    }

    #[test]
    fn a_process_group_lists_only_its_members() {
        let mut leader = std::process::Command::new("/bin/sh")
            .args(["-c", "sleep 5 & sleep 5 & wait"])
            .process_group(0)
            .spawn()
            .unwrap();
        let pgid = leader.id() as i32;
        let mut members = Vec::new();
        for _ in 0..200 {
            members = group(pgid).unwrap();
            if members.len() == 3 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(members.len(), 3, "{members:?}");
        assert!(members.iter().all(|member| member.pgid == pgid));
        assert!(
            of_user(nix::unistd::geteuid().as_raw())
                .unwrap()
                .iter()
                .filter(|info| info.pgid == pgid)
                .count()
                == 3
        );
        nix::sys::signal::killpg(
            nix::unistd::Pid::from_raw(pgid),
            nix::sys::signal::Signal::SIGKILL,
        )
        .unwrap();
        leader.wait().unwrap();
    }

    #[test]
    fn a_session_is_read_for_a_live_process() {
        assert_eq!(
            session(std::process::id() as i32),
            nix::unistd::getsid(None).ok().map(nix::unistd::Pid::as_raw)
        );
    }

    #[test]
    fn the_boot_is_a_uuid() {
        let boot = boot().unwrap();
        assert_eq!(boot.len(), 36);
        assert_eq!(boot.matches('-').count(), 4);
    }

    /// Records written before every read went through the sysctls hold the
    /// start time `proc_pidinfo` gave: the two must agree to the
    /// microsecond, or every recorded process would read as another.
    #[test]
    #[allow(
        unsafe_code,
        reason = "FFI: proc_pidinfo, which older records were read with, has no safe binding"
    )]
    fn the_start_time_is_the_one_proc_pidinfo_gives() {
        let pid = nix::unistd::getpid().as_raw();
        let mut info = std::mem::MaybeUninit::<libc::proc_bsdinfo>::uninit();
        let size = std::mem::size_of_val(&info) as i32;
        // SAFETY: proc_pidinfo receives a writable buffer of the declared size.
        let read = unsafe {
            libc::proc_pidinfo(
                pid,
                libc::PROC_PIDTBSDINFO,
                0,
                info.as_mut_ptr().cast(),
                size,
            )
        };
        assert_eq!(read, size);
        // SAFETY: proc_pidinfo initialized the complete proc_bsdinfo above.
        let info = unsafe { info.assume_init() };
        assert_eq!(
            start(pid).unwrap(),
            [info.pbi_start_tvsec, info.pbi_start_tvusec]
        );
    }

    #[test]
    fn only_this_accounts_live_processes_are_described() {
        let this = Instance::current().unwrap();
        assert!(this.is_this_one());
        assert_eq!(this.state(), State::Running);
        assert_eq!(
            Instance::new(this.pid, [this.start[0], this.start[1] ^ 1]).state(),
            State::Gone,
            "another start is another process"
        );

        if nix::unistd::geteuid().is_root() {
            return;
        }
        let launchd = info(1).unwrap().unwrap();
        assert_eq!(
            launchd.instance.state(),
            State::Unknown,
            "another account's"
        );
        assert_eq!(
            Instance::new(0, [1, 0]).state(),
            State::Unknown,
            "the kernel's"
        );

        let mut child = std::process::Command::new("/usr/bin/true").spawn().unwrap();
        let pid = child.id() as i32;
        let exited = Instance::new(pid, start(pid).unwrap());
        let mut zombie = false;
        for _ in 0..200 {
            zombie = info(pid).unwrap().is_some_and(|info| info.zombie);
            if zombie {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(zombie, "the child exited");
        assert_eq!(
            exited.state(),
            State::Gone,
            "an unreaped process runs no more"
        );
        child.wait().unwrap();
        assert_eq!(exited.state(), State::Gone);
    }

    #[test]
    fn a_suspended_process_reads_as_stopped() {
        use nix::sys::signal::{Signal, kill};

        let mut child = std::process::Command::new("/bin/sleep")
            .arg("30")
            .spawn()
            .unwrap();
        let pid = child.id() as i32;
        kill(nix::unistd::Pid::from_raw(pid), Signal::SIGSTOP).unwrap();
        let started = std::time::Instant::now();
        while !info(pid).unwrap().unwrap().stopped {
            assert!(started.elapsed() < std::time::Duration::from_secs(10));
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        child.kill().unwrap();
        child.wait().unwrap();
    }
}

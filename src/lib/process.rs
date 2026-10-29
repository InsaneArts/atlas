//! What macOS says about a process: when it started, whose it is, its group
//! and parent. A process ID is never trusted on its own: the same ID with
//! another start time is another process.
//!
//! Everything is read through the `KERN_PROC` sysctls, which report any
//! account's process and a process that has exited but is not yet reaped,
//! with its exact start time. `proc_pidinfo` refuses both.
use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use std::ffi::CStr;

/// One process instance: the same PID with another start time is another
/// process.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Instance {
    pub pid: i32,
    /// When it started: seconds and microseconds since 1970, as the kernel
    /// keeps them.
    pub start: [u64; 2],
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
    /// It has exited and waits for its parent to collect its status.
    pub zombie: bool,
    /// Its command name, as the kernel keeps it (at most 16 bytes).
    pub name: String,
}

impl Info {
    /// Whether it runs as `uid`, effective and real user ID alike.
    pub fn runs_as(&self, uid: u32) -> bool {
        self.uid == uid && self.ruid == uid
    }
}

/// The size of one `struct kinfo_proc` on 64-bit macOS.
const KINFO_PROC: usize = 648;
/// Where each field this module reads lies in a `struct kinfo_proc`
/// (`sys/sysctl.h`, `sys/proc.h`): `kp_proc.p_starttime` (a timeval),
/// `p_stat`, `p_pid`, `p_comm`, `kp_eproc.e_pcred.p_ruid`,
/// `e_ucred.cr_uid`, `e_ppid` and `e_pgid`.
const START: usize = 0;
const STAT: usize = 36;
const PID: usize = 40;
const COMM: usize = 243;
const COMM_LEN: usize = 17;
const RUID: usize = 392;
const UID: usize = 420;
const PPID: usize = 560;
const PGID: usize = 564;
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
pub fn start(pid: i32) -> Result<[u64; 2]> {
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
        // SAFETY: the MIB is valid for its length; a null output asks only its size.
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
        // SAFETY: the MIB is valid and the buffer has the declared writable capacity.
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
        return Ok(buffer[..size]
            .chunks_exact(KINFO_PROC)
            .filter_map(parse)
            .collect());
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
        zombie: *record.get(STAT)? == SZOMB,
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
        assert!(!launchd.runs_as(nix::unistd::getuid().as_raw()));

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
}

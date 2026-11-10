//! What a process uses, as the kernel accounts it: physical footprint, its
//! lifetime peak, and resident size. Only this account's processes are read;
//! another account's accounting would need its owner's rights.
use atlas_core::{
    app_bundle::signature::executable_of,
    process::{self, Instance},
};
use serde::Serialize;
use std::{
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
};

/// A process's memory, in bytes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Usage {
    pub footprint: u64,
    pub peak_footprint: u64,
    pub resident: u64,
}

/// This process's memory now, or none when the kernel refuses to say.
pub fn own() -> Option<Usage> {
    of(nix::unistd::getpid().as_raw())
}

/// One process as it was found: which instance, whose, and what it runs.
/// Two readings describe the same process only when all of these agree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Process {
    pub instance: Instance,
    /// Its effective user ID.
    pub uid: u32,
    /// Its real user ID.
    pub real_uid: u32,
    /// The executable it runs.
    pub path: PathBuf,
}

/// Process `pid` as it is now, or none when it has exited or macOS would
/// not say. Its start time is read before and after its executable, so a
/// PID reused in between is not described as one process.
pub fn describe(pid: i32) -> Option<Process> {
    let live = || {
        process::info(pid)
            .ok()
            .flatten()
            .filter(|info| !info.zombie)
    };
    let before = live()?;
    let path = executable_of(u32::try_from(pid).ok()?)?;
    let after = live()?;
    (after == before).then_some(Process {
        instance: before.instance,
        uid: before.uid,
        real_uid: before.ruid,
        path,
    })
}

/// This account's process running the executable at `path`, and its
/// memory; none when it has none or the kernel refuses to say. Only the
/// account's processes with the executable's name are described, and
/// nothing is started to find out.
pub fn running(path: &Path) -> Option<(Process, Usage)> {
    let uid = nix::unistd::getuid().as_raw();
    // The kernel keeps at most the first 16 bytes of a command name.
    let name = path.file_name()?.as_bytes();
    let name = &name[..name.len().min(16)];
    let found = process::of_user(uid)
        .ok()?
        .into_iter()
        .filter(|info| info.ruid == uid && !info.zombie && info.name.as_bytes() == name)
        .find_map(|info| describe(info.instance.pid).filter(|process| process.path == path))?;
    let usage = of(found.instance.pid)?;
    Some((found, usage))
}

/// Process `pid`'s memory now, or none when the kernel refuses to say.
#[allow(unsafe_code, reason = "FFI: proc_pid_rusage has no safe binding")]
fn of(pid: i32) -> Option<Usage> {
    let mut info = std::mem::MaybeUninit::<libc::rusage_info_v4>::zeroed();
    // SAFETY: for RUSAGE_INFO_V4 the kernel writes at most one
    // rusage_info_v4 into the buffer, which is exactly that size and aligned
    // for it; the buffer is read only after the call reports success.
    let status =
        unsafe { libc::proc_pid_rusage(pid, libc::RUSAGE_INFO_V4, info.as_mut_ptr().cast()) };
    if status != 0 {
        return None;
    }
    // SAFETY: the call succeeded, so the structure is initialized; it was
    // zeroed before, so no field is left undefined either way.
    let info = unsafe { info.assume_init() };
    Some(Usage {
        footprint: info.ri_phys_footprint,
        peak_footprint: info.ri_lifetime_max_phys_footprint,
        resident: info.ri_resident_size,
    })
}

#[cfg(test)]
#[path = "resources_tests.rs"]
mod tests;

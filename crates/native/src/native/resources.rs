//! What a process uses, as the kernel accounts it: physical footprint, its
//! lifetime peak, and resident size. Only this account's processes are read;
//! another account's accounting would need its owner's rights.
use serde::Serialize;

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

/// The memory of this account's running process named `name`, or none when
/// it has none or the kernel refuses to say. Nothing is started to find out.
pub fn named(name: &str) -> Option<Usage> {
    let uid = nix::unistd::getuid().as_raw();
    let process = atlas_core::process::of_user(uid)
        .ok()?
        .into_iter()
        .find(|process| process.name == name && process.ruid == uid && !process.zombie)?;
    of(process.instance.pid)
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
mod tests;

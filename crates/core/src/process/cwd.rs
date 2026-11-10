//! A transient working directory, checked against the same account and start.
use super::{Instance, Own, own};
use std::{os::unix::ffi::OsStringExt, path::PathBuf};

pub(crate) fn cwd(instance: Instance) -> Option<PathBuf> {
    read(instance, own, native)
}

fn read(
    instance: Instance,
    describe: impl Fn(i32) -> Own,
    directory: impl FnOnce(i32) -> Option<Vec<u8>>,
) -> Option<PathBuf> {
    let matches = || {
        matches!(describe(instance.pid), Own::Found(info)
        if info.instance == instance && info.uid == nix::unistd::geteuid().as_raw())
    };
    if !matches() {
        return None;
    }
    let mut bytes = directory(instance.pid)?;
    let end = bytes.iter().position(|byte| *byte == 0)?;
    bytes.truncate(end);
    let path = PathBuf::from(std::ffi::OsString::from_vec(bytes));
    (path.is_absolute() && matches()).then_some(path)
}

#[cfg(target_os = "macos")]
#[allow(
    unsafe_code,
    reason = "libc is the binding for proc_pidinfo and its SDK layout"
)]
fn native(pid: i32) -> Option<Vec<u8>> {
    let mut info = std::mem::MaybeUninit::<libc::proc_vnodepathinfo>::uninit();
    let size = i32::try_from(std::mem::size_of_val(&info)).ok()?;
    // SAFETY: the writable buffer has exactly size bytes; no field is read
    // unless the kernel reports that it filled the complete structure.
    let read = unsafe {
        libc::proc_pidinfo(
            pid,
            libc::PROC_PIDVNODEPATHINFO,
            0,
            info.as_mut_ptr().cast(),
            size,
        )
    };
    if read != size {
        return None;
    }
    // SAFETY: a successful full proc_pidinfo reply initialized this SDK structure.
    let info = unsafe { info.assume_init() };
    Some(
        info.pvi_cdir
            .vip_path
            .into_iter()
            .flatten()
            .map(|byte| byte as u8)
            .collect(),
    )
}

#[cfg(not(target_os = "macos"))]
fn native(_pid: i32) -> Option<Vec<u8>> {
    None
}

#[cfg(test)]
#[path = "cwd_tests.rs"]
mod tests;

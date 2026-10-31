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
    let bytes = directory(instance.pid)?;
    let end = bytes.iter().position(|byte| *byte == 0)?;
    let path = PathBuf::from(std::ffi::OsString::from_vec(bytes[..end].to_vec()));
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
mod tests {
    use super::*;

    #[test]
    fn a_hosts_working_directory_is_read_only_for_the_same_process() {
        let instance = Instance::current().unwrap();
        assert_eq!(cwd(instance), std::env::current_dir().ok());
        assert_eq!(cwd(Instance::new(instance.pid, [1, 0])), None);
    }

    #[test]
    fn a_partial_or_unterminated_working_directory_is_unknown() {
        let instance = Instance::current().unwrap();
        for bytes in [
            None,
            Some(b"/unterminated".to_vec()),
            Some(b"relative\0".to_vec()),
        ] {
            assert_eq!(read(instance, own, |_| bytes), None);
        }
    }

    #[test]
    fn a_reused_or_foreign_pid_has_no_working_directory() {
        let instance = Instance::current().unwrap();
        assert_eq!(
            read(instance, |_| Own::Unknown, |_| panic!("must not read")),
            None
        );
        let calls = std::cell::Cell::new(0);
        assert_eq!(
            read(
                instance,
                |pid| {
                    calls.set(calls.get() + 1);
                    if calls.get() == 1 {
                        own(pid)
                    } else {
                        Own::Gone
                    }
                },
                |_| Some(b"/tmp\0".to_vec())
            ),
            None
        );
    }
}

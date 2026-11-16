//! The file this program runs from, as the kernel mapped its image: never
//! what its launch path names now, which an update may have replaced
//! before anyone looked.
#![allow(
    unsafe_code,
    reason = "FFI: proc_pidinfo's region path information has no safe binding"
)]

use std::{ffi::c_int, mem::MaybeUninit};

/// `proc_pidinfo`'s flavor for one region of memory and the file it maps.
const PROC_PIDREGIONPATHINFO: c_int = 8;

/// `struct proc_regioninfo`, as `<sys/proc_info.h>` lays it out.
#[repr(C)]
#[derive(Clone, Copy)]
struct RegionInfo {
    protection: u32,
    max_protection: u32,
    inheritance: u32,
    flags: u32,
    offset: u64,
    behavior: u32,
    user_wired_count: u32,
    user_tag: u32,
    pages_resident: u32,
    pages_shared_now_private: u32,
    pages_swapped_out: u32,
    pages_dirtied: u32,
    ref_count: u32,
    shadow_depth: u32,
    share_mode: u32,
    private_pages_resident: u32,
    shared_pages_resident: u32,
    obj_id: u32,
    depth: u32,
    address: u64,
    size: u64,
}

/// `struct proc_regionwithpathinfo`: a region and the file it maps.
#[repr(C)]
struct RegionWithPath {
    region: RegionInfo,
    path: libc::vnode_info_path,
}

/// The file an image was mapped from: its device and inode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mapped {
    pub device: u64,
    pub inode: u64,
}

/// The file this program's own image was mapped from; none when the kernel
/// does not say which file it is.
#[must_use]
pub fn mapped() -> Option<Mapped> {
    let size = c_int::try_from(size_of::<RegionWithPath>()).ok()?;
    // Any address in this program's own code lies in its image.
    let address = mapped as *const () as usize as u64;
    let mut found = MaybeUninit::<RegionWithPath>::zeroed();
    // SAFETY: the buffer is writable for `size` bytes, as many as the call
    // writes at most.
    let written = unsafe {
        libc::proc_pidinfo(
            libc::getpid(),
            PROC_PIDREGIONPATHINFO,
            address,
            found.as_mut_ptr().cast(),
            size,
        )
    };
    if written != size {
        return None;
    }
    // SAFETY: the call filled the whole buffer, and every field is plain
    // data, valid whatever its bits.
    let found = unsafe { found.assume_init() };
    let stat = found.path.vip_vi.vi_stat;
    // The kernel may answer whole without naming the file: no inode, or no
    // path, names none.
    let named = found.path.vip_path[0][0] != 0;
    (named && stat.vst_ino != 0).then_some(Mapped {
        device: u64::from(stat.vst_dev),
        inode: stat.vst_ino,
    })
}

#[cfg(test)]
#[path = "program_tests.rs"]
mod tests;

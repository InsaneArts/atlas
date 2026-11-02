//! Where macOS's App Translocation runs a quarantined app from, traced back
//! to the bundle it was opened from. Gatekeeper runs such an app from a
//! read-only copy under a random `AppTranslocation` path, so a process
//! running from an install may not run from its path at all. Security's
//! `SecTranslocateCreateOriginalPathForURL` has no safe binding.
use objc2_core_foundation::{CFError, CFRetained, CFString, CFURL, CFURLPathStyle};
use std::{
    path::{Path, PathBuf},
    ptr::{self, NonNull},
};

#[link(name = "Security", kind = "framework")]
unsafe extern "C" {
    fn SecTranslocateCreateOriginalPathForURL(
        translocated: &CFURL,
        error: *mut *mut CFError,
    ) -> Option<NonNull<CFURL>>;
}

/// The bundle a translocated bundle at `translocated` was opened from, for
/// the account running this process; none when macOS cannot say.
pub(super) fn original(translocated: &Path) -> Option<PathBuf> {
    let path = CFString::from_str(translocated.to_str()?);
    let url =
        CFURL::with_file_system_path(None, Some(&path), CFURLPathStyle::CFURLPOSIXPathStyle, true)?;
    let mut error: *mut CFError = ptr::null_mut();
    // SAFETY: `url` is a live CFURL borrowed for the call and `error` is a
    // valid place for an out-pointer. The function follows Core Foundation's
    // Create rule: a URL it returns is owned by the caller, which
    // CFRetained::from_raw takes over and releases.
    let returned = unsafe { SecTranslocateCreateOriginalPathForURL(&url, &raw mut error) };
    if let Some(error) = NonNull::new(error) {
        // SAFETY: a non-null error the function wrote is owned by the
        // caller under the same rule, and is released here.
        drop(unsafe { CFRetained::from_raw(error) });
    }
    // SAFETY: as above, a returned URL is owned by the caller.
    let original = returned.map(|url| unsafe { CFRetained::from_raw(url) })?;
    let path = original.file_system_path(CFURLPathStyle::CFURLPOSIXPathStyle)?;
    Some(PathBuf::from(path.to_string()))
}

//! App icons for agent apps to show beside the helper's apps, read on the
//! person's side from the app bundles the helper runs.
use crate::error::{Code, Error, Result};
use objc2_app_kit::NSWorkspace;
use objc2_foundation::NSString;
use std::path::Path;

/// The icon of the app bundle at `bundle` as a `size`×`size` PNG.
pub fn png(bundle: &Path, size: usize) -> Result<Vec<u8>> {
    let missing = || Error::new(Code::Apps, "The app's icon could not be read");
    // Finder's generic icon stands in for a bundle that is not there; no
    // icon is better than a wrong one.
    if !bundle.join("Contents").is_dir() {
        return Err(missing());
    }

    // Called on worker threads, so each call drains its own autorelease pool.
    let tiff = objc2::rc::autoreleasepool(|_| {
        let path = NSString::from_str(&bundle.to_string_lossy());
        let image = NSWorkspace::sharedWorkspace().iconForFile(&path);
        image.TIFFRepresentation().map(|tiff| tiff.to_vec())
    })
    .ok_or_else(missing)?;
    super::capture::icon_png(&tiff, size)
}

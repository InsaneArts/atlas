//! Screenshots of the helper's main display, taken by the helper daemon.
//!
//! Without Screen Recording consent CoreGraphics still returns an image — of
//! the wallpaper alone — so consent is checked first and an unconsented
//! capture is refused rather than returned.
use crate::error::{Error, Result};
use objc2_core_foundation::{CFMutableData, CFRetained, CFString, CGPoint, CGRect, CGSize};
use objc2_core_graphics::{
    CGBitmapContextCreate, CGBitmapContextCreateImage, CGColorSpace, CGContext, CGDisplayBounds,
    CGImage, CGInterpolationQuality, CGMainDisplayID, CGPreflightScreenCaptureAccess,
    CGRequestScreenCaptureAccess,
};
use objc2_image_io::CGImageDestination;

/// premultipliedFirst | byteOrder32Little: the native BGRA layout.
const BGRA: u32 = 2 | (2 << 12);

/// One encoded screenshot.
pub struct Shot {
    pub png: Vec<u8>,
    pub width: u32,
    pub height: u32,
    /// Backing pixels per display point.
    pub scale: f64,
}

/// Whether this process may record the screen.
pub fn permitted() -> bool {
    CGPreflightScreenCaptureAccess()
}

/// Ask macOS for Screen Recording, so Understudy appears in System Settings
/// with a prompt in the helper's session. Returns whether it is allowed now;
/// a grant takes effect for the next process.
pub fn request() -> bool {
    CGRequestScreenCaptureAccess()
}

/// The size `width`×`height` scales to so that neither side exceeds `limit`,
/// keeping the aspect ratio; unchanged when it already fits.
pub fn fit(width: usize, height: usize, limit: Option<u32>) -> (usize, usize) {
    let Some(limit) = limit.map(|l| l as usize) else {
        return (width, height);
    };
    let longest = width.max(height);
    if longest <= limit || longest == 0 {
        return (width, height);
    }
    let scale = limit as f64 / longest as f64;
    let side = |n: usize| ((n as f64 * scale).round() as usize).max(1);
    (side(width), side(height))
}

/// The sRGB color space, by its documented name.
fn srgb() -> Option<CFRetained<CGColorSpace>> {
    CGColorSpace::with_name(Some(&CFString::from_static_str("kCGColorSpaceSRGB")))
}

/// An empty `width`×`height` BGRA sRGB bitmap context.
fn bitmap(width: usize, height: usize) -> Option<CFRetained<CGContext>> {
    let space = srgb()?;
    // SAFETY: a null data pointer asks CoreGraphics to allocate and own the
    // pixel buffer; the other arguments describe a standard BGRA layout.
    unsafe {
        CGBitmapContextCreate(
            std::ptr::null_mut(),
            width,
            height,
            8,
            0,
            Some(&space),
            BGRA,
        )
    }
}

/// Draw `image` into a new `width`×`height` sRGB bitmap.
fn scaled(image: &CGImage, width: usize, height: usize) -> Result<CFRetained<CGImage>> {
    let failed = || Error::new("capture", "The screenshot could not be scaled");
    let context = bitmap(width, height).ok_or_else(failed)?;
    CGContext::set_interpolation_quality(Some(&context), CGInterpolationQuality::High);
    let rect = CGRect::new(CGPoint::ZERO, CGSize::new(width as f64, height as f64));
    CGContext::draw_image(Some(&context), rect, Some(image));
    CGBitmapContextCreateImage(Some(&context)).ok_or_else(failed)
}

/// `image` encoded as PNG.
fn png(image: &CGImage) -> Result<Vec<u8>> {
    let failed = || Error::new("capture", "The screenshot could not be encoded");
    let data = CFMutableData::new(None, 0).ok_or_else(failed)?;
    let kind = CFString::from_static_str("public.png");
    // SAFETY: a live mutable data and type identifier, one image, and no
    // options or properties dictionaries.
    let finished = unsafe {
        let destination =
            CGImageDestination::with_data(&data, &kind, 1, None).ok_or_else(failed)?;
        destination.add_image(image, None);
        destination.finalize()
    };
    if !finished {
        return Err(failed());
    }
    Ok(data.to_vec())
}

/// The main display, at most `limit` pixels on its longest side.
pub fn main_display(limit: Option<u32>) -> Result<Shot> {
    if !permitted() {
        return Err(Error::new(
            "permission",
            "Screen Recording is not allowed for Understudy in the helper account",
        ));
    }
    let display = CGMainDisplayID();
    let points = CGDisplayBounds(display).size;
    // Deprecated for ScreenCaptureKit, but still a synchronous, consent-gated
    // capture of exactly this display on current macOS (qualified live).
    #[allow(deprecated)]
    let image = objc2_core_graphics::CGDisplayCreateImage(display)
        .ok_or_else(|| Error::new("capture", "The display could not be captured"))?;
    let (width, height) = (CGImage::width(Some(&image)), CGImage::height(Some(&image)));
    let scale = if points.width > 0.0 {
        width as f64 / points.width
    } else {
        1.0
    };
    let (target_width, target_height) = fit(width, height, limit);
    let image = if (target_width, target_height) == (width, height) {
        image
    } else {
        scaled(&image, target_width, target_height)?
    };
    let size =
        |side: usize| u32::try_from(side).map_err(|_| Error::new("capture", "Image too large"));
    Ok(Shot {
        png: png(&image)?,
        width: size(target_width)?,
        height: size(target_height)?,
        scale,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_fit_the_longest_side_and_keep_their_aspect() {
        assert_eq!(fit(2560, 1440, Some(320)), (320, 180));
        assert_eq!(fit(1440, 2560, Some(320)), (180, 320));
        assert_eq!(fit(2560, 1440, None), (2560, 1440));
        assert_eq!(fit(800, 600, Some(1600)), (800, 600), "never enlarged");
        assert_eq!(fit(5000, 1, Some(100)), (100, 1), "never a zero side");
    }

    #[test]
    fn a_bitmap_scales_and_encodes_as_png() {
        assert!(srgb().is_some(), "the sRGB name resolves");
        // A real image from a bitmap context stands in for a display capture.
        let context = bitmap(64, 36).unwrap();
        let image = CGBitmapContextCreateImage(Some(&context)).unwrap();
        let small = scaled(&image, 32, 18).unwrap();
        let encoded = png(&small).unwrap();
        let (width, height) = crate::client::png_dimensions(&encoded).unwrap();
        assert_eq!((width, height), (32, 18));
    }
}

//! Screenshots of the helper's main display, taken by the helper daemon,
//! and smaller JPEG copies of them for agent apps to show.
//!
//! Without Screen Recording consent CoreGraphics still returns an image — of
//! the wallpaper alone — so consent is checked first and an unconsented
//! capture is refused rather than returned.
// FFI boundary: bitmap context and ImageIO encoding.
#![allow(unsafe_code)]
use crate::error::{Error, Result};
use objc2_core_foundation::{
    CFData, CFDictionary, CFMutableData, CFNumber, CFRetained, CFString, CGPoint, CGRect, CGSize,
};
use objc2_core_graphics::{
    CGBitmapContextCreate, CGBitmapContextCreateImage, CGColorSpace, CGContext, CGDisplayBounds,
    CGImage, CGImageAlphaInfo, CGImageByteOrderInfo, CGInterpolationQuality,
    CGPreflightScreenCaptureAccess, CGRequestScreenCaptureAccess,
};
use objc2_image_io::{
    CGImageDestination, CGImageSource, kCGImageDestinationLossyCompressionQuality,
};

/// premultipliedFirst | byteOrder32Little: the native BGRA layout.
const BGRA: u32 = CGImageAlphaInfo::PremultipliedFirst.0 | CGImageByteOrderInfo::Order32Little.0;

/// One encoded screenshot.
pub struct Shot {
    pub png: Vec<u8>,
    pub width: u32,
    pub height: u32,
    /// The display's backing pixels per point, measured before any
    /// downscaling: an image shrunk to fit a limit holds fewer pixels per
    /// point than this.
    pub scale: f64,
}

/// Whether this process may record the screen.
pub fn permitted() -> bool {
    CGPreflightScreenCaptureAccess()
}

/// Ask macOS for Screen Recording, so Atlas appears in System Settings
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
    encoded(image, "public.png", None)
}

/// `image` encoded as the image type `kind`, with `properties` such as a
/// JPEG's quality.
fn encoded(
    image: &CGImage,
    kind: &'static str,
    properties: Option<&CFDictionary>,
) -> Result<Vec<u8>> {
    let failed = || Error::new("capture", "The screenshot could not be encoded");
    let data = CFMutableData::new(None, 0).ok_or_else(failed)?;
    let kind = CFString::from_static_str(kind);
    // SAFETY: a live mutable data and type identifier, room for one image,
    // and no options dictionary.
    let destination =
        unsafe { CGImageDestination::with_data(&data, &kind, 1, None) }.ok_or_else(failed)?;
    // SAFETY: a live destination and image; the properties, when given, map
    // ImageIO's own keys to numbers.
    unsafe { destination.add_image(image, properties) };

    // SAFETY: a live destination holding the one image it was made for.
    if !unsafe { destination.finalize() } {
        return Err(failed());
    }

    Ok(data.to_vec())
}

/// A screenshot's PNG as a JPEG of `quality` (0 to 1), at most `limit`
/// pixels on its longest side: small enough to send to an agent app many
/// times a minute. Its size comes with it.
pub fn jpeg(png: &[u8], limit: u32, quality: f64) -> Result<(Vec<u8>, u32, u32)> {
    let failed = || Error::new("capture", "The screenshot could not be read");
    let data = CFData::from_bytes(png);
    // SAFETY: live data holding the image, and no options dictionary.
    let source = unsafe { CGImageSource::with_data(&data, None) }.ok_or_else(failed)?;
    // SAFETY: a live source; index 0 is its one image, and no options.
    let image = unsafe { source.image_at_index(0, None) }.ok_or_else(failed)?;

    let (width, height) = (CGImage::width(Some(&image)), CGImage::height(Some(&image)));
    let (target_width, target_height) = fit(width, height, Some(limit));
    let image = if (target_width, target_height) == (width, height) {
        image
    } else {
        scaled(&image, target_width, target_height)?
    };

    // SAFETY: ImageIO's own key, a static string it exports.
    let key = unsafe { kCGImageDestinationLossyCompressionQuality };
    let quality = CFNumber::new_f64(quality.clamp(0.0, 1.0));
    let properties = CFDictionary::<CFString, CFNumber>::from_slices(&[key], &[&*quality]);
    let bytes = encoded(&image, "public.jpeg", Some(properties.as_opaque()))?;

    let size =
        |side: usize| u32::try_from(side).map_err(|_| Error::new("capture", "Image too large"));
    Ok((bytes, size(target_width)?, size(target_height)?))
}

/// An app's icon as a `size`×`size` PNG, from `image` holding it at
/// several sizes (the TIFF AppKit gives): drawn from the smallest of them at
/// least `size` across, else from the largest.
pub fn icon_png(image: &[u8], size: usize) -> Result<Vec<u8>> {
    let failed = || Error::new("capture", "The icon could not be read");
    let data = CFData::from_bytes(image);
    // SAFETY: live data holding the image, and no options dictionary.
    let source = unsafe { CGImageSource::with_data(&data, None) }.ok_or_else(failed)?;
    // SAFETY: a live source.
    let count = unsafe { source.count() };

    let mut chosen: Option<(usize, CFRetained<CGImage>)> = None;
    for index in 0..count {
        // SAFETY: a live source, an index below its count, and no options.
        let Some(picture) = (unsafe { source.image_at_index(index, None) }) else {
            continue;
        };
        let width = CGImage::width(Some(&picture));
        let better = chosen.as_ref().is_none_or(|(best, _)| {
            if *best >= size {
                width >= size && width < *best
            } else {
                width > *best
            }
        });
        if better {
            chosen = Some((width, picture));
        }
    }

    let (_, picture) = chosen.ok_or_else(failed)?;
    let icon = scaled(&picture, size, size)?;
    png(&icon)
}

/// A blank `width`×`height` PNG, for tests of what handles screenshots.
#[cfg(test)]
pub(crate) fn sample_png(width: usize, height: usize) -> Vec<u8> {
    let context = bitmap(width, height).expect("a bitmap context");
    let image = CGBitmapContextCreateImage(Some(&context)).expect("an image");
    png(&image).expect("a PNG")
}

/// The checked main display, or only `area` of it (in global display
/// points), at most `limit` pixels on its longest side.
pub fn main_display(display: u32, area: Option<CGRect>, limit: Option<u32>) -> Result<Shot> {
    if !permitted() {
        return Err(Error::new(
            "permission",
            "Screen Recording is not allowed for Atlas in the helper account",
        ));
    }

    let failed = || Error::new("capture", "The display could not be captured");
    let bounds = CGDisplayBounds(display);
    // Deprecated for ScreenCaptureKit, but still a synchronous, consent-gated
    // capture of exactly this display on current macOS (qualified live).
    let (image, points) = match area {
        None => {
            #[allow(deprecated)]
            let image = objc2_core_graphics::CGDisplayCreateImage(display).ok_or_else(failed)?;
            (image, bounds.size.width)
        }
        Some(area) => {
            // The rectangle is in the display's own space: points from its
            // top-left corner. The image holds its pixels.
            let local = CGRect::new(
                CGPoint::new(
                    area.origin.x - bounds.origin.x,
                    area.origin.y - bounds.origin.y,
                ),
                area.size,
            );
            #[allow(deprecated)]
            let image = objc2_core_graphics::CGDisplayCreateImageForRect(display, local)
                .ok_or_else(failed)?;
            (image, area.size.width)
        }
    };

    let (width, height) = (CGImage::width(Some(&image)), CGImage::height(Some(&image)));
    let scale = if points > 0.0 {
        width as f64 / points
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
    fn the_bitmap_layout_keeps_its_raw_value() {
        assert_eq!(BGRA, 2 | (2 << 12));
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

    #[test]
    fn a_png_becomes_a_smaller_jpeg_of_the_same_aspect() {
        let context = bitmap(640, 360).unwrap();
        let image = CGBitmapContextCreateImage(Some(&context)).unwrap();
        let png = png(&image).unwrap();

        let (small, width, height) = jpeg(&png, 320, 0.6).unwrap();
        assert_eq!((width, height), (320, 180));
        assert_eq!(&small[..3], &[0xFF, 0xD8, 0xFF], "a JPEG's start of image");

        let (_, width, _) = jpeg(&png, 4096, 0.6).unwrap();
        assert_eq!(width, 640, "never enlarged");
        assert_eq!(
            jpeg(b"not a picture", 320, 0.6).unwrap_err().code,
            "capture"
        );
    }
}

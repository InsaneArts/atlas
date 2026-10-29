//! Screenshots of the helper's main display, taken by the helper daemon,
//! and smaller JPEG copies of them for agent apps to show.
//!
//! Without Screen Recording consent CoreGraphics still returns an image — of
//! the wallpaper alone — so consent is checked first and an unconsented
//! capture is refused rather than returned.
#![allow(
    unsafe_code,
    reason = "FFI: the bitmap context and ImageIO encoding have no safe binding"
)]
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

/// `limit`, lowered to `points`, the captured area's longer side in display
/// points (rounded up), so the picture holds at most one pixel per point.
pub fn per_point(limit: Option<u32>, points: f64) -> Option<u32> {
    let side = (points.ceil() as u32).max(1);

    Some(limit.map_or(side, |limit| limit.min(side)))
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

/// A `width`×`height` PNG, blank but for its top `rows` rows, which are
/// filled: two of these differ only where a menu bar would be.
#[cfg(test)]
pub(crate) fn banded_png(width: usize, height: usize, rows: usize) -> Vec<u8> {
    png(&banded(width, height, rows)).expect("a PNG")
}

#[cfg(test)]
fn banded(width: usize, height: usize, rows: usize) -> CFRetained<CGImage> {
    let context = bitmap(width, height).expect("a bitmap context");
    CGContext::set_rgb_fill_color(Some(&context), 0.2, 0.4, 0.8, 1.0);
    // A bitmap context's origin is its bottom left.
    let band = CGRect::new(
        CGPoint::new(0.0, (height - rows) as f64),
        CGSize::new(width as f64, rows as f64),
    );
    CGContext::fill_rect(Some(&context), band);

    CGBitmapContextCreateImage(Some(&context)).expect("an image")
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
    fn a_picture_per_point_holds_one_pixel_per_point_up_to_the_limit() {
        let sized = |pixels: (usize, usize), points: (f64, f64)| {
            fit(
                pixels.0,
                pixels.1,
                per_point(Some(1456), points.0.max(points.1)),
            )
        };

        assert_eq!(sized((2560, 1440), (2560.0, 1440.0)), (1456, 819));
        assert_eq!(sized((3024, 1964), (1512.0, 982.0)), (1456, 946));
        assert_eq!(sized((1280, 800), (1280.0, 800.0)), (1280, 800));
        assert_eq!(sized((2560, 1600), (1280.0, 800.0)), (1280, 800));
        assert_eq!(sized((602, 504), (602.0, 504.0)), (602, 504));
        assert_eq!(sized((1204, 1008), (602.0, 504.0)), (602, 504));
        assert_eq!(sized((400, 1200), (200.0, 600.0)), (200, 600));
        assert_eq!(sized((4000, 2000), (2000.0, 1000.0)), (1456, 728));
    }

    #[test]
    fn the_per_point_limit_keeps_the_lower_bound_and_rounds_points_up() {
        assert_eq!(per_point(Some(1456), 602.0), Some(602));
        assert_eq!(per_point(Some(400), 602.0), Some(400));
        assert_eq!(per_point(None, 1512.0), Some(1512));
        assert_eq!(per_point(Some(1456), 600.5), Some(601));
        assert_eq!(per_point(Some(1456), 0.0), Some(1));
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
    fn the_same_pixels_encode_to_the_same_png() {
        let plain = || png(&banded(640, 360, 24)).unwrap();
        assert_eq!(plain(), plain());
        assert_ne!(plain(), sample_png(640, 360), "different pixels differ");

        let shrunk = || png(&scaled(&banded(640, 360, 24), 320, 180).unwrap()).unwrap();
        assert_eq!(shrunk(), shrunk());
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

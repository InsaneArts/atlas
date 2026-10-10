//! Screenshots of the helper's main display, taken by the helper daemon.
//!
//! Without Screen Recording consent CoreGraphics still returns an image — of
//! the wallpaper alone — so consent is checked first and an unconsented
//! capture is refused rather than returned.
use crate::{
    error::{Error, Result},
    native::cf::{self, CfRef, Owned},
};
use std::ffi::c_void;

#[repr(C)]
#[derive(Clone, Copy)]
struct Point {
    x: f64,
    y: f64,
}
#[repr(C)]
#[derive(Clone, Copy)]
struct Size {
    width: f64,
    height: f64,
}
#[repr(C)]
#[derive(Clone, Copy)]
struct Rect {
    origin: Point,
    size: Size,
}

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGPreflightScreenCaptureAccess() -> bool;
    fn CGRequestScreenCaptureAccess() -> bool;
    fn CGMainDisplayID() -> u32;
    fn CGDisplayBounds(display: u32) -> Rect;
    fn CGDisplayCreateImage(display: u32) -> CfRef;
    fn CGImageGetWidth(image: CfRef) -> usize;
    fn CGImageGetHeight(image: CfRef) -> usize;
    fn CGColorSpaceCreateWithName(name: CfRef) -> CfRef;
    fn CGBitmapContextCreate(
        data: *mut c_void,
        width: usize,
        height: usize,
        bits_per_component: usize,
        bytes_per_row: usize,
        space: CfRef,
        bitmap_info: u32,
    ) -> CfRef;
    fn CGContextSetInterpolationQuality(context: CfRef, quality: i32);
    fn CGContextDrawImage(context: CfRef, rect: Rect, image: CfRef);
    fn CGBitmapContextCreateImage(context: CfRef) -> CfRef;
    static kCGColorSpaceSRGB: CfRef;
}
#[link(name = "ImageIO", kind = "framework")]
unsafe extern "C" {
    fn CGImageDestinationCreateWithData(
        data: CfRef,
        kind: CfRef,
        count: usize,
        options: CfRef,
    ) -> CfRef;
    fn CGImageDestinationAddImage(destination: CfRef, image: CfRef, properties: CfRef);
    fn CGImageDestinationFinalize(destination: CfRef) -> bool;
}
#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFDataCreateMutable(allocator: CfRef, capacity: isize) -> CfRef;
}

/// premultipliedFirst | byteOrder32Little: the native BGRA layout.
const BGRA: u32 = 2 | (2 << 12);
const HIGH_QUALITY: i32 = 3;

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
    // SAFETY: no arguments; only reads this process's consent.
    unsafe { CGPreflightScreenCaptureAccess() }
}

/// Ask macOS for Screen Recording, so Understudy appears in System Settings
/// with a prompt in the helper's session. Returns whether it is allowed now;
/// a grant takes effect for the next process.
pub fn request() -> bool {
    // SAFETY: no arguments; shows the system prompt at most once.
    unsafe { CGRequestScreenCaptureAccess() }
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

/// Draw `image` into a new `width`×`height` sRGB bitmap.
fn scaled(image: &Owned, width: usize, height: usize) -> Result<Owned> {
    let failed = || Error::new("capture", "The screenshot could not be scaled");
    // SAFETY: every reference is checked non-null and owned; the rectangle
    // covers exactly the new bitmap.
    unsafe {
        let space =
            Owned::adopt(CGColorSpaceCreateWithName(kCGColorSpaceSRGB)).ok_or_else(failed)?;
        let context = Owned::adopt(CGBitmapContextCreate(
            std::ptr::null_mut(),
            width,
            height,
            8,
            0,
            space.get(),
            BGRA,
        ))
        .ok_or_else(failed)?;
        CGContextSetInterpolationQuality(context.get(), HIGH_QUALITY);
        let rect = Rect {
            origin: Point { x: 0.0, y: 0.0 },
            size: Size {
                width: width as f64,
                height: height as f64,
            },
        };
        CGContextDrawImage(context.get(), rect, image.get());
        Owned::adopt(CGBitmapContextCreateImage(context.get())).ok_or_else(failed)
    }
}

/// `image` encoded as PNG.
fn png(image: &Owned) -> Result<Vec<u8>> {
    let failed = || Error::new("capture", "The screenshot could not be encoded");
    // SAFETY: references are checked non-null and owned; the destination is
    // finalized before its data is read.
    unsafe {
        let data = Owned::adopt(CFDataCreateMutable(std::ptr::null(), 0)).ok_or_else(failed)?;
        let kind = cf::string(c"public.png").ok_or_else(failed)?;
        let destination = Owned::adopt(CGImageDestinationCreateWithData(
            data.get(),
            kind.get(),
            1,
            std::ptr::null(),
        ))
        .ok_or_else(failed)?;
        CGImageDestinationAddImage(destination.get(), image.get(), std::ptr::null());
        if !CGImageDestinationFinalize(destination.get()) {
            return Err(failed());
        }
        Ok(cf::bytes(data.get()))
    }
}

/// The main display, at most `limit` pixels on its longest side.
pub fn main_display(limit: Option<u32>) -> Result<Shot> {
    if !permitted() {
        return Err(Error::new(
            "permission",
            "Screen Recording is not allowed for Understudy in the helper account",
        ));
    }
    // SAFETY: the display ID comes from the system; Create returns an owned
    // image or null.
    let (image, points) = unsafe {
        let display = CGMainDisplayID();
        (
            Owned::adopt(CGDisplayCreateImage(display)),
            CGDisplayBounds(display).size,
        )
    };
    let image = image.ok_or_else(|| Error::new("capture", "The display could not be captured"))?;
    // SAFETY: a live image.
    let (width, height) = unsafe { (CGImageGetWidth(image.get()), CGImageGetHeight(image.get())) };
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
    Ok(Shot {
        png: png(&image)?,
        width: u32::try_from(target_width).map_err(|_| Error::new("capture", "Image too large"))?,
        height: u32::try_from(target_height)
            .map_err(|_| Error::new("capture", "Image too large"))?,
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
        // A real image from a bitmap context stands in for a display capture.
        unsafe {
            let space = Owned::adopt(CGColorSpaceCreateWithName(kCGColorSpaceSRGB)).unwrap();
            let context = Owned::adopt(CGBitmapContextCreate(
                std::ptr::null_mut(),
                64,
                36,
                8,
                0,
                space.get(),
                BGRA,
            ))
            .unwrap();
            let image = Owned::adopt(CGBitmapContextCreateImage(context.get())).unwrap();
            let small = scaled(&image, 32, 18).unwrap();
            let encoded = png(&small).unwrap();
            let (width, height) = crate::client::png_dimensions(&encoded).unwrap();
            assert_eq!((width, height), (32, 18));
        }
    }
}

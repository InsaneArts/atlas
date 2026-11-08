//! Unencoded display pixels for the private live-frame transport.
use super::{
    BGRA, fit,
    shape::{Area, Shape},
    srgb,
};
use atlas_core::{
    buffer::Buffer,
    error::{Code, Error, Result},
};
use objc2_core_foundation::{CFData, CGPoint, CGRect, CGSize};
use objc2_core_graphics::{
    CGBitmapContextCreate, CGBitmapInfo, CGColorRenderingIntent, CGContext, CGDataProvider,
    CGImage, CGInterpolationQuality,
};

/// Tightly packed sRGB bytes, left to right and top to bottom, three per pixel.
pub struct RawShot {
    pub rgb: Buffer,
    pub width: u32,
    pub height: u32,
    /// Original display backing pixels per point, before downscaling.
    pub scale: f64,
}

/// Draw once into the target-sized bitmap, combining scaling and color
/// conversion. Reading that bitmap avoids a second image allocation or codec.
pub(super) fn from_image(image: &CGImage, limit: Option<u32>, scale: f64) -> Result<RawShot> {
    let failed = || Error::new(Code::Capture, "The screenshot pixels could not be read");
    let (width, height) = fit(
        CGImage::width(Some(image)),
        CGImage::height(Some(image)),
        limit,
    );
    Ok(RawShot {
        rgb: drawn(image, width, height)?,
        width: u32::try_from(width).map_err(|_| failed())?,
        height: u32::try_from(height).map_err(|_| failed())?,
        scale,
    })
}

/// The `shape` of a BGRA picture whose rows of `width`×`height` pixels lie
/// `stride` bytes apart in `bytes`, as packed sRGB: the bytes repacked when
/// it is the whole picture at its own size, else its area drawn at the size
/// asked for.
///
/// # Errors
///
/// Returns an error when the layout or the area does not fit the bytes, or
/// the picture cannot be drawn.
pub(super) fn shaped(
    bytes: &[u8],
    (width, height, stride): (usize, usize, usize),
    shape: Shape,
) -> Result<Buffer> {
    const INVALID: &str = "The stream picture layout is invalid";
    if shape.is_whole((width, height)) {
        return pack(bytes, width, height, stride, INVALID);
    }
    let failed = || Error::new(Code::Capture, INVALID);
    let length = stride.checked_mul(height).ok_or_else(failed)?;
    if width == 0 || height == 0 || stride < width * 4 || bytes.len() < length {
        return Err(failed());
    }
    let area = shape.area.unwrap_or(Area {
        x: 0,
        y: 0,
        width,
        height,
    });
    if area.x + area.width > width || area.y + area.height > height || shape.width == 0 {
        return Err(failed());
    }
    let data = CFData::from_bytes(&bytes[..length]);
    let provider = CGDataProvider::with_cf_data(Some(&data)).ok_or_else(failed)?;
    let space = srgb().ok_or_else(failed)?;
    // SAFETY: the provider holds every row of 8-bit BGRA the size and stride
    // describe, checked above, and no decode array is given.
    let image = unsafe {
        CGImage::new(
            width,
            height,
            8,
            32,
            stride,
            Some(&space),
            CGBitmapInfo(BGRA),
            Some(&provider),
            std::ptr::null(),
            false,
            CGColorRenderingIntent::RenderingIntentDefault,
        )
    }
    .ok_or_else(failed)?;
    let rect = CGRect::new(
        CGPoint::new(area.x as f64, area.y as f64),
        CGSize::new(area.width as f64, area.height as f64),
    );
    let part = CGImage::with_image_in_rect(Some(&image), rect).ok_or_else(failed)?;
    drawn(&part, shape.width, shape.height)
}

/// `image` drawn once into a `width`×`height` bitmap, scaled and converted
/// to sRGB, as packed bytes.
fn drawn(image: &CGImage, width: usize, height: usize) -> Result<Buffer> {
    let failed = || Error::new(Code::Capture, "The screenshot pixels could not be read");
    let stride = width
        .checked_mul(4)
        .and_then(|row| row.checked_next_multiple_of(64))
        .ok_or_else(failed)?;
    let length = stride
        .checked_mul(height)
        .filter(|length| *length > 0)
        .ok_or_else(failed)?;
    // Own the staging pixels too: a null CGContext data pointer would make
    // CoreGraphics allocate a large malloc bitmap with deferred reclamation.
    let mut backing = Buffer::zeroed(length)?;
    let space = srgb().ok_or_else(failed)?;
    // SAFETY: stable, uniquely owned initialized backing covers stride*height;
    // stride is a multiple of four and at least width*4. No context or image
    // escapes. Context is dropped before backing on every path, and backing
    // is not accessed while the context can draw into its pointer.
    let context = unsafe {
        CGBitmapContextCreate(
            backing.as_mut_ptr().cast(),
            width,
            height,
            8,
            stride,
            Some(&space),
            BGRA,
        )
    }
    .ok_or_else(failed)?;
    CGContext::set_interpolation_quality(Some(&context), CGInterpolationQuality::High);
    CGContext::draw_image(
        Some(&context),
        CGRect::new(CGPoint::ZERO, CGSize::new(width as f64, height as f64)),
        Some(image),
    );

    drop(context);
    pack(
        &backing,
        width,
        height,
        stride,
        "The screenshot bitmap layout is invalid",
    )
}

/// Strip BGRA alpha and row padding. Whole-display captures are opaque; their
/// RGB channels are already in the bitmap's sRGB color space. A layout that
/// does not fit is refused with `invalid`.
pub(super) fn pack(
    bytes: &[u8],
    width: usize,
    height: usize,
    stride: usize,
    invalid: &'static str,
) -> Result<Buffer> {
    let failed = || Error::new(Code::Capture, invalid);
    let row = width.checked_mul(4).ok_or_else(failed)?;
    let length = stride.checked_mul(height).ok_or_else(failed)?;
    let capacity = width
        .checked_mul(height)
        .and_then(|pixels| pixels.checked_mul(3))
        .filter(|length| *length <= isize::MAX as usize)
        .ok_or_else(failed)?;
    if width == 0 || height == 0 || stride < row || bytes.len() < length {
        return Err(failed());
    }
    // Fixed output slices let LLVM vectorize the channel shuffle, rather
    // than checking and updating Vec length for every three-byte pixel.
    let mut rgb = Buffer::zeroed(capacity)?;
    for (line, output) in bytes[..length]
        .chunks_exact(stride)
        .zip(rgb.chunks_exact_mut(width * 3))
    {
        for (pixel, target) in line[..row].chunks_exact(4).zip(output.chunks_exact_mut(3)) {
            target.copy_from_slice(&[pixel[2], pixel[1], pixel[0]]);
        }
    }
    Ok(rgb)
}

#[cfg(test)]
mod tests;

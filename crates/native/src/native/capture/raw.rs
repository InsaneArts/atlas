//! Unencoded display pixels for the private live-frame transport.
use super::{BGRA, fit, srgb};
use atlas_core::{
    buffer::Buffer,
    error::{Code, Error, Result},
};
use objc2_core_foundation::{CGPoint, CGRect, CGSize};
use objc2_core_graphics::{CGBitmapContextCreate, CGContext, CGImage, CGInterpolationQuality};

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
    let output_width = u32::try_from(width).map_err(|_| failed())?;
    let output_height = u32::try_from(height).map_err(|_| failed())?;
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
    let rgb = pack(
        &backing,
        width,
        height,
        stride,
        "The screenshot bitmap layout is invalid",
    )?;

    Ok(RawShot {
        rgb,
        width: output_width,
        height: output_height,
        scale,
    })
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

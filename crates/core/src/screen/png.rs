//! The size a PNG says it has, read without decoding it.
use crate::error::{Code, Error, Result};

/// Read width and height from a PNG header without decoding its pixels.
///
/// # Errors
///
/// Rejects missing or malformed PNG signature and header dimensions without decoding pixel
/// data.
pub fn png_dimensions(png: &[u8]) -> Result<(u32, u32)> {
    let invalid = || Error::new(Code::Capture, "Invalid PNG header or dimensions");

    if png.len() < 33 || &png[..16] != b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR" {
        return Err(invalid());
    }

    let width = u32::from_be_bytes(png[16..20].try_into().map_err(|_| invalid())?);
    let height = u32::from_be_bytes(png[20..24].try_into().map_err(|_| invalid())?);
    if width == 0 || height == 0 || width > 32768 || height > 32768 {
        return Err(invalid());
    }

    Ok((width, height))
}

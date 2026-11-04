//! Lossless RGB compression. Large, nearly incompressible pictures use stored
//! deflate blocks while retaining the already-probed zlib transport profile.
use atlas_core::buffer::Buffer;
use flate2::{Compression, write::ZlibEncoder};
use std::io::{self, Write};

const ADAPTIVE_MIN: usize = 256 * 1024;
const SAMPLE_MIN: usize = 16 * 1024;
const SAMPLE_WINDOWS: usize = 4;

pub(super) fn compress(pixels: &[u8], width: u32) -> io::Result<Buffer> {
    let compression = choose(pixels, width)?;
    // Reserve conservative zlib output space once for either compression
    // level. Reallocating a large growing payload left dirty empty VM regions
    // across repeated resizes on macOS. Large output uses private anonymous
    // pages; only compressed bytes are touched, and Drop returns the mapping.
    let capacity = pixels
        .len()
        .saturating_add(pixels.len() / 1000)
        .saturating_add(128);
    encode(pixels, compression, capacity)
}

fn encode(pixels: &[u8], compression: Compression, capacity: usize) -> io::Result<Buffer> {
    let mut encoder = ZlibEncoder::new(Buffer::with_capacity(capacity)?, compression);
    encoder.write_all(pixels)?;
    encoder.finish()
}

fn choose(pixels: &[u8], width: u32) -> io::Result<Compression> {
    if pixels.len() < ADAPTIVE_MIN {
        return Ok(Compression::fast());
    }
    let row = usize::try_from(width)
        .ok()
        .and_then(|width| width.checked_mul(3));
    let Some(row) = row.filter(|row| *row > 0 && *row <= 64 * 1024) else {
        return Ok(Compression::fast());
    };
    // Dispersed tiny samples miss correlation between neighboring scanlines.
    // Each contiguous window includes at least two complete source rows.
    let rows = SAMPLE_MIN.div_ceil(row).max(2);
    let block = row.saturating_mul(rows).min(pixels.len());
    let mut sampled = Vec::with_capacity(block * SAMPLE_WINDOWS);
    let step = (pixels.len() - block) / (SAMPLE_WINDOWS - 1);
    for index in 0..SAMPLE_WINDOWS {
        let offset = (index * step / row) * row;
        sampled.extend_from_slice(&pixels[offset..offset + block]);
    }
    let encoded = encode(&sampled, Compression::fast(), sampled.len() + 128)?;
    // This is a conservative estimate, not a promise about every scene. Both
    // choices preserve every RGB byte; stored blocks avoid expensive search
    // when the sampled windows save less than two percent.
    if encoded.len() * 100 >= sampled.len() * 98 {
        Ok(Compression::none())
    } else {
        Ok(Compression::fast())
    }
}

#[cfg(test)]
mod tests;

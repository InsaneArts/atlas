//! What the person's side shows of the helper without acting there: the
//! screenshots agents took, and the pictures made from them. `atlas mcp`'s
//! view and the menu bar's viewer service both read through here, so each
//! shows the same things under the same rules.
use atlas_core::{
    error::{Code, Error, Result},
    files::open_plain,
    name::name_type,
};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use std::path::Path;

/// The small picture of the whole screen a view gets before its tiles: its
/// longest side and JPEG quality.
pub(crate) const BASE: (u32, f64) = (768, 0.45);
/// The most base64 characters one answer's pictures may take: well under
/// the ~150,000 characters an agent app hydrates into a view.
pub(crate) const FRAME_BUDGET: usize = 120_000;
/// The largest screen or screenshot read, in bytes: a 5K display's PNG
/// takes well under it.
pub const FRAME_LIMIT: usize = 16 << 20;
/// A screenshot's thumbnail: its widest side and JPEG quality.
pub const THUMB: (u32, f64) = (480, 0.6);
/// The bytes of a PNG that say its size: the signature and the IHDR chunk.
const PNG_HEAD: u64 = 33;

/// A screenshot's thumbnail: a small JPEG, as base64, with its size.
///
/// # Errors
///
/// Returns an error when the PNG can't be read.
pub fn thumbnail(png: &[u8]) -> Result<Value> {
    Ok(picture(&jpeg(png, THUMB)?))
}

/// A JPEG and its size as a view takes it.
fn picture((bytes, width, height): &(Vec<u8>, u32, u32)) -> Value {
    json!({
        "mimeType": "image/jpeg",
        "data": STANDARD.encode(bytes),
        "width": width,
        "height": height,
    })
}

/// When a screenshot named `name` was taken, in nanoseconds since 1970:
/// only names an agent's capture writes have one.
pub fn capture_time(name: &str) -> Option<u128> {
    let digits = name.strip_suffix("-capture.png")?;
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

name_type! {
    /// The file name of a screenshot an agent's capture wrote: Atlas's own
    /// timestamp name, never a path or a person's file.
    #[derive(PartialOrd, Ord)]
    pub struct Capture;
    refused(name) = format!("{name} is not a capture's name");
}

impl Capture {
    /// `name` as a capture's, when [`capture_time`] reads it.
    pub fn parse(name: &str) -> Option<Self> {
        capture_time(name).map(|_| Self(name.to_owned()))
    }
}

/// The agents' screenshots in `output`, newest first, at most `limit`:
/// each one's name and when it was taken, in nanoseconds since 1970. Only
/// plain files a capture names count; links and other files never do.
pub fn captures(output: &Path, limit: usize) -> Vec<(u128, String)> {
    let Ok(entries) = std::fs::read_dir(output) else {
        return Vec::new();
    };

    let mut shots = entries
        .flatten()
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
        .filter_map(|entry| {
            let name = entry.file_name().to_str()?.to_owned();
            Some((capture_time(&name)?, name))
        })
        .collect::<Vec<_>>();
    shots.sort_by_key(|(at, _)| std::cmp::Reverse(*at));
    shots.truncate(limit);
    shots
}

/// The screenshot `name` in `output`, whole. Only a name a capture writes,
/// and only a plain file the agent's capture wrote: never through a link,
/// and no more of it than a screenshot takes.
///
/// # Errors
///
/// Returns an error for a name no capture writes, or a file that is missing,
/// too large or not plain.
pub fn read_capture(output: &Path, name: &str) -> Result<Vec<u8>> {
    let unknown = || Error::new(Code::BadRequest, "No screenshot has that name");
    if capture_time(name).is_none() {
        return Err(unknown());
    }

    let file = open_plain(&output.join(name)).ok_or_else(unknown)?;
    let png = atlas_core::files::read_up_to(file, FRAME_LIMIT as u64 + 1)?;
    if png.len() > FRAME_LIMIT {
        return Err(Error::new(
            Code::Capture,
            "The screenshot is too large to read",
        ));
    }

    Ok(png)
}

/// The size in pixels of the screenshot `name` in `output`, read from its
/// PNG header under [`read_capture`]'s rules.
pub fn capture_size(output: &Path, name: &str) -> Option<(u32, u32)> {
    capture_time(name)?;
    let file = open_plain(&output.join(name))?;
    let head = atlas_core::files::read_up_to(file, PNG_HEAD).ok()?;
    atlas_core::screen::png::png_dimensions(&head).ok()
}

/// How many base64 characters `bytes` bytes take.
pub(crate) fn base64_length(bytes: usize) -> usize {
    bytes.div_ceil(3) * 4
}

/// `png` as a JPEG at most `limit` pixels on its longest side, at
/// `quality`, with its size.
///
/// # Errors
///
/// Returns an error when the PNG can't be read.
#[cfg(target_os = "macos")]
pub fn jpeg(png: &[u8], (limit, quality): (u32, f64)) -> Result<(Vec<u8>, u32, u32)> {
    atlas_native::native::capture::jpeg(png, limit, quality)
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn jpeg(_png: &[u8], _: (u32, f64)) -> Result<(Vec<u8>, u32, u32)> {
    Err(Error::new(Code::Platform, "Atlas runs on macOS only"))
}

#[cfg(target_os = "macos")]
pub(crate) fn jpeg_rgb(
    rgb: &[u8],
    pixels: (usize, usize, usize),
    (limit, quality): (u32, f64),
) -> Result<(Vec<u8>, u32, u32)> {
    atlas_native::native::capture::jpeg_rgb(rgb, pixels, limit, quality)
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn jpeg_rgb(
    _: &[u8],
    _: (usize, usize, usize),
    _: (u32, f64),
) -> Result<(Vec<u8>, u32, u32)> {
    Err(Error::new(Code::Platform, "Atlas runs on macOS only"))
}

#[cfg(test)]
#[path = "helper_info_tests.rs"]
mod tests;

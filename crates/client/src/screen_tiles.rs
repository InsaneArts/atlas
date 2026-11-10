//! The helper's screen as the view in an agent app draws it: cut into
//! tiles, and only the tiles that changed since the picture the view
//! already shows sent again. The view stays as sharp as the screen while no
//! one answer grows past what an agent app hands a view.
//!
//! Every answer leaves the view showing one screen everywhere: the tiles
//! that changed all come in it, or a small picture of the whole screen
//! does, so what the person clicks is what is there. Only sharpness may
//! wait for the next answers.
use crate::helper_info::{BASE, FRAME_BUDGET, base64_length, jpeg_rgb};
use atlas_core::error::{Code, Error, Result};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::VecDeque;

/// A tile's side, in pixels.
pub const TILE: usize = 256;
/// A tile's JPEG quality, and the lower one a tile too detailed to fit an
/// answer on its own is sent at.
const QUALITY: f64 = 0.65;
const QUALITY_SMALL: f64 = 0.45;
/// The smaller whole picture sent when even the base is too detailed.
const BASE_SMALL: (u32, f64) = (512, 0.35);
/// How many answers a tile waits for room to sharpen, behind tiles that
/// keep changing, before it stays as the base drew it.
const MOST_WAITS: u8 = 8;
/// The characters a tile's place takes in an answer, beside its picture.
const TILE_OVERHEAD: usize = 40;
/// How many pictures sent are remembered, so a view naming one of them
/// gets only what changed since.
const KEPT: usize = 16;

/// Packed 8-bit RGB pixels, `width`×`height`, with rows `stride` bytes
/// apart.
#[derive(Clone, Copy)]
pub struct Pixels<'a> {
    pub rgb: &'a [u8],
    pub width: usize,
    pub height: usize,
    pub stride: usize,
}

impl Pixels<'_> {
    /// Whether the rows the size and stride describe are all there.
    fn filled(&self) -> bool {
        let row = self.width.checked_mul(3);
        let last = self
            .height
            .checked_sub(1)
            .and_then(|rows| rows.checked_mul(self.stride));
        match (row, last) {
            (Some(row), Some(last)) => {
                row > 0
                    && self.stride >= row
                    && last
                        .checked_add(row)
                        .is_some_and(|end| end <= self.rgb.len())
            }
            _ => false,
        }
    }

    /// The rows of the part at `x`, `y`, `width`×`height`, top to bottom.
    fn rows(&self, (x, y, width, height): Rect) -> impl Iterator<Item = &[u8]> {
        (y..y + height).map(move |row| {
            let start = row * self.stride + x * 3;
            &self.rgb[start..start + width * 3]
        })
    }
}

/// A tile's place and size: x, y, width, height, in pixels.
type Rect = (usize, usize, usize, usize);

/// Makes a JPEG of pixels at most a longest side, at a quality.
pub(crate) type Encode<'a> = dyn FnMut(Pixels<'_>, (u32, f64)) -> Result<Vec<u8>> + 'a;

/// What the view draws next: tiles to put on its picture, the picture
/// they make, and whether more are still to come.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Tiled {
    /// The picture the view shows once it draws this; named again as
    /// `since`, it gets what changed after it.
    pub id: String,
    pub width: usize,
    pub height: usize,
    pub mime_type: &'static str,
    /// A small picture of the whole screen, drawn across the view's picture
    /// first: when the view has nothing to build on, or what changed does
    /// not fit one answer.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base: Option<String>,
    pub tiles: Vec<Tile>,
    /// Whether some of the picture is still only as sharp as a base, so the
    /// view asks again now.
    pub more: bool,
}

impl Tiled {
    /// Whether this draws nothing: the view's picture is the screen.
    pub fn unchanged(&self) -> bool {
        self.base.is_none() && self.tiles.is_empty()
    }
}

/// One tile: where it goes on the picture, and its JPEG as base64.
#[derive(Serialize)]
pub struct Tile {
    pub x: usize,
    pub y: usize,
    pub data: String,
}

/// What the view shows of one tile: the hash of the screen it shows
/// there, whether sharp or only as a base draws it, and how many answers
/// had no room to sharpen it.
#[derive(Clone, Copy, PartialEq, Eq)]
struct Shown {
    hash: u64,
    sharp: bool,
    waited: u8,
}

/// A picture sent: its id, its size, and what the view shows of each tile;
/// none before anything was drawn.
struct Sent {
    id: String,
    size: (usize, usize),
    tiles: Vec<Option<Shown>>,
}

/// The tiles one answer carries, and the room left for more.
struct Answer {
    room: usize,
    tiles: Vec<Tile>,
}

impl Answer {
    /// Add a tile at `rect` when it fits; whether it did.
    fn add(&mut self, rect: Rect, data: &[u8]) -> bool {
        let cost = cost(data);
        if cost > self.room {
            return false;
        }
        self.room -= cost;
        self.tiles.push(Tile {
            x: rect.0,
            y: rect.1,
            data: STANDARD.encode(data),
        });
        true
    }
}

/// The pictures sent lately, newest first.
#[derive(Default)]
pub struct Tiles {
    sent: VecDeque<Sent>,
}

impl Tiles {
    /// The tiles of `pixels` the view showing `since` lacks, as many as fit
    /// one answer.
    ///
    /// # Errors
    ///
    /// Returns an error when the pixels don't fill their size or a tile
    /// can't be encoded.
    pub fn next(&mut self, since: Option<&str>, pixels: Pixels<'_>) -> Result<Tiled> {
        self.next_with(since, pixels, &mut |picture, shape| {
            jpeg_rgb(
                picture.rgb,
                (picture.width, picture.height, picture.stride),
                shape,
            )
            .map(|(bytes, _, _)| bytes)
        })
    }

    /// [`Self::next`], encoding each picture with `encode`: its pixels, and
    /// the longest side and quality of the JPEG made of them.
    pub(crate) fn next_with(
        &mut self,
        since: Option<&str>,
        pixels: Pixels<'_>,
        encode: &mut Encode<'_>,
    ) -> Result<Tiled> {
        if !pixels.filled() {
            return Err(Error::new(Code::Capture, "The screen could not be read"));
        }
        let size = (pixels.width, pixels.height);
        let rects = rects(size);
        let hashes: Vec<u64> = rects.iter().map(|rect| hash(pixels, *rect)).collect();

        let known = since
            .and_then(|id| {
                self.sent
                    .iter()
                    .find(|sent| sent.id == id && sent.size == size)
            })
            .map(|sent| sent.tiles.clone());

        // What changed since the view's picture comes whole, or a base does.
        let mut answer = Answer {
            room: FRAME_BUDGET,
            tiles: Vec::new(),
        };
        let mut base = None;
        let mut shown = match known {
            Some(mut shown) => {
                let stale: Vec<usize> = (0..rects.len())
                    .filter(|&at| shown[at].map(|tile| tile.hash) != Some(hashes[at]))
                    .collect();
                let mut whole = true;
                for at in stale {
                    let data = encode(cut(pixels, rects[at]).view(), (side(&rects[at]), QUALITY))?;
                    if !answer.add(rects[at], &data) {
                        whole = false;
                        break;
                    }
                    shown[at] = Some(Shown {
                        hash: hashes[at],
                        sharp: true,
                        waited: 0,
                    });
                }
                if whole {
                    shown
                } else {
                    answer.tiles.clear();
                    answer.room = FRAME_BUDGET;
                    Vec::new()
                }
            }
            None => Vec::new(),
        };
        if shown.is_empty() {
            let small = whole_picture(pixels, encode)?;
            answer.room -= small.len();
            base = Some(small);
            shown = hashes
                .iter()
                .map(|&hash| {
                    Some(Shown {
                        hash,
                        sharp: false,
                        waited: 0,
                    })
                })
                .collect();
        }

        // Then the tiles still only as sharp as a base, as many as fit, the
        // first one at a lower quality when that is what fits. One too
        // detailed to fit an answer alone, or that waited too long behind
        // tiles that keep changing, stays as the base drew it.
        let mut first = true;
        for (at, rect) in rects.iter().enumerate() {
            let Some(tile) = shown[at].filter(|tile| !tile.sharp) else {
                continue;
            };
            let part = cut(pixels, *rect);
            let alone = base.is_none() && answer.tiles.is_empty();
            let mut data = encode(part.view(), (side(rect), QUALITY))?;
            if cost(&data) > answer.room && first {
                data = encode(part.view(), (side(rect), QUALITY_SMALL))?;
            }
            if answer.add(*rect, &data) {
                shown[at] = Some(Shown {
                    sharp: true,
                    ..tile
                });
            } else {
                let waited = tile.waited.saturating_add(1);
                shown[at] = Some(Shown {
                    sharp: alone || waited >= MOST_WAITS,
                    waited,
                    ..tile
                });
                break;
            }
            first = false;
        }
        let more = shown.iter().flatten().any(|tile| !tile.sharp);

        let id = id(size, &shown);
        self.remember(Sent {
            id: id.clone(),
            size,
            tiles: shown,
        });
        Ok(Tiled {
            id,
            width: size.0,
            height: size.1,
            mime_type: "image/jpeg",
            base,
            tiles: answer.tiles,
            more,
        })
    }

    /// Keep `sent` as the newest picture, once.
    fn remember(&mut self, sent: Sent) {
        self.sent.retain(|kept| kept.id != sent.id);
        self.sent.push_front(sent);
        self.sent.truncate(KEPT);
    }
}

/// The tiles covering a picture of `size`, row by row.
fn rects((width, height): (usize, usize)) -> Vec<Rect> {
    (0..height)
        .step_by(TILE)
        .flat_map(|y| {
            (0..width)
                .step_by(TILE)
                .map(move |x| (x, y, TILE.min(width - x), TILE.min(height - y)))
        })
        .collect()
}

/// A tile's pixels, as a hash.
fn hash(pixels: Pixels<'_>, rect: Rect) -> u64 {
    let mut hasher = Sha256::new();
    for row in pixels.rows(rect) {
        hasher.update(row);
    }
    let digest = hasher.finalize();
    let mut first = [0; 8];
    first.copy_from_slice(&digest[..8]);
    u64::from_le_bytes(first)
}

/// A picture's id: its size and what each of its tiles shows.
fn id(size: (usize, usize), tiles: &[Option<Shown>]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(size.0.to_le_bytes());
    hasher.update(size.1.to_le_bytes());
    for tile in tiles {
        let (hash, kind) = tile.map_or((0, 0), |tile| (tile.hash, 1 + u8::from(tile.sharp)));
        hasher.update(hash.to_le_bytes());
        hasher.update([kind]);
    }
    atlas_core::name::hex(&hasher.finalize()[..8])
}

/// The small picture of the whole screen, as base64: at the base's size,
/// or smaller still when that is too detailed for an answer.
///
/// # Errors
///
/// Returns an error when it can't be encoded, or not even the smaller one
/// fits an answer.
fn whole_picture(pixels: Pixels<'_>, encode: &mut Encode<'_>) -> Result<String> {
    for shape in [BASE, BASE_SMALL] {
        let data = encode(pixels, shape)?;
        if base64_length(data.len()) <= FRAME_BUDGET {
            return Ok(STANDARD.encode(data));
        }
    }
    Err(Error::new(
        Code::Capture,
        "The screen is too detailed to send",
    ))
}

/// The characters a tile of JPEG `bytes` takes in an answer.
fn cost(bytes: &[u8]) -> usize {
    base64_length(bytes.len()) + TILE_OVERHEAD
}

/// A tile's pixels, packed.
struct Cut {
    rgb: Vec<u8>,
    width: usize,
    height: usize,
}

impl Cut {
    fn view(&self) -> Pixels<'_> {
        Pixels {
            rgb: &self.rgb,
            width: self.width,
            height: self.height,
            stride: self.width * 3,
        }
    }
}

/// The part `rect` of `pixels`, packed.
fn cut(pixels: Pixels<'_>, rect: Rect) -> Cut {
    Cut {
        rgb: pixels.rows(rect).flatten().copied().collect(),
        width: rect.2,
        height: rect.3,
    }
}

/// A tile's longest side as the encoder takes it: never more than [`TILE`].
fn side(rect: &Rect) -> u32 {
    u32::try_from(rect.2.max(rect.3)).unwrap_or(u32::MAX)
}

#[cfg(test)]
#[path = "screen_tiles_tests.rs"]
mod tests;

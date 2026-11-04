//! One lossless image canvas. Optional root-frame patches update dirty source
//! rectangles without creating independently scaled image placements or frames.
use super::graphics::{IMAGE, Layout, Painter};
use atlas_core::buffer::Buffer;
use image::RgbImage;
use sha2::{Digest, Sha256};
use std::io::{self, Write};

const TILE_PIXELS: u32 = 128;
const MAX_SIDE: u32 = 4096;

#[derive(Clone, Copy)]
struct Tile {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Geometry {
    layout: Layout,
    width: u32,
    height: u32,
}

struct Picture<'a> {
    pixels: &'a [u8],
    size: (u32, u32),
}

pub(super) struct Tiles {
    source: Option<([u8; 32], RgbImage)>,
    geometry: Option<Geometry>,
    tiles: Vec<Tile>,
    canvas: Painter,
    patches: bool,
    previous: Buffer,
    pixels: Vec<u8>,
    timing: Timing,
}

#[derive(Clone, Copy, Default)]
pub(super) struct Timing {
    pub(super) encode_ms: f64,
    pub(super) write_ms: f64,
    pub(super) bytes: usize,
}

impl Default for Tiles {
    fn default() -> Self {
        Self {
            source: None,
            geometry: None,
            tiles: Vec::new(),
            canvas: Painter::at(IMAGE + 10, (0, 0)),
            patches: false,
            previous: Buffer::default(),
            pixels: Vec::new(),
            timing: Timing::default(),
        }
    }
}

impl Tiles {
    pub(super) fn timing(&self) -> Timing {
        self.timing
    }
    pub(super) fn patches_enabled(&self) -> bool {
        self.patches
    }
    /// Enable only after an acknowledgment of the root-frame edit probe.
    /// Unsupported terminals retain one seam-free image placement per frame.
    pub(super) fn set_patches(&mut self, supported: bool) {
        if self.patches != supported {
            self.patches = supported;
            self.geometry = None;
            self.tiles.clear();
        }
    }

    pub(super) fn clear(&mut self, out: &mut impl Write) -> io::Result<()> {
        self.canvas.clear(out)?;
        self.tiles = Vec::new();
        self.geometry = None;
        self.source = None;
        self.previous = Buffer::default();
        self.pixels = Vec::new();
        Ok(())
    }

    /// Preserve capture resolution. The terminal applies one final fit to the
    /// whole image, so arbitrary aspect ratios cannot create internal seams.
    pub(super) fn draw(
        &mut self,
        out: &mut impl Write,
        png: &[u8],
        layout: Layout,
        terminal: (u16, u16, u16, u16),
    ) -> io::Result<usize> {
        let encoded: [u8; 32] = Sha256::digest(png).into();
        if self
            .source
            .as_ref()
            .is_none_or(|(previous, _)| *previous != encoded)
        {
            let source = image::load_from_memory_with_format(png, image::ImageFormat::Png)
                .map_err(io::Error::other)?
                .to_rgb8();
            self.source = Some((encoded, source));
        }
        // Move the cached image out temporarily, never clone its pixel buffer.
        let source = self
            .source
            .take()
            .ok_or_else(|| io::Error::other("No decoded frame"))?;
        let result = self.draw_picture(
            out,
            Picture {
                pixels: source.1.as_raw(),
                size: source.1.dimensions(),
            },
            layout,
            terminal,
        );
        self.source = Some(source);
        result
    }

    /// Raw transport bytes stay borrowed through hashing and patch packing.
    pub(super) fn draw_rgb(
        &mut self,
        out: &mut impl Write,
        rgb: &[u8],
        size: (u32, u32),
        layout: Layout,
        terminal: (u16, u16, u16, u16),
    ) -> io::Result<usize> {
        if size.0 == 0
            || size.1 == 0
            || size.0 > MAX_SIDE
            || size.1 > MAX_SIDE
            || rgb.len() != size.0 as usize * size.1 as usize * 3
        {
            return Err(io::Error::other("Invalid terminal RGB frame"));
        }
        // A previous compatibility PNG is no longer useful on the raw path.
        self.source = None;
        self.draw_picture(out, Picture { pixels: rgb, size }, layout, terminal)
    }

    fn draw_picture(
        &mut self,
        out: &mut impl Write,
        source: Picture<'_>,
        layout: Layout,
        terminal: (u16, u16, u16, u16),
    ) -> io::Result<usize> {
        let began = std::time::Instant::now();
        self.timing = Timing::default();
        let (columns, rows, pixel_width, pixel_height) = terminal;
        if columns == 0
            || rows == 0
            || pixel_width == 0
            || pixel_height == 0
            || layout.columns == 0
            || layout.rows == 0
        {
            return Err(io::Error::other("No terminal pixel geometry"));
        }
        let (width, height) = source.size;
        let geometry = Geometry {
            layout,
            width,
            height,
        };
        let rebuilding = self.geometry != Some(geometry);
        if !rebuilding && self.previous.as_slice() == source.pixels {
            self.timing.encode_ms = began.elapsed().as_secs_f64() * 1000.0;
            return Ok(0);
        }
        if rebuilding {
            // Invalidate before releasing an old baseline: a failed upload
            // must retry a complete image, never compare against freed pixels.
            self.geometry = None;
            if self.previous.len() != source.pixels.len() {
                self.previous = Buffer::default();
            }
            self.tiles = Vec::new();
            if self.patches {
                for y in (0..height).step_by(TILE_PIXELS as usize) {
                    for x in (0..width).step_by(TILE_PIXELS as usize) {
                        self.tiles.push(Tile {
                            x,
                            y,
                            width: TILE_PIXELS.min(width - x),
                            height: TILE_PIXELS.min(height - y),
                        });
                    }
                }
            }
            self.canvas.invalidate();
        }
        let mut dirty = Vec::new();
        let mut changed_pixels = 0_u64;
        for tile in &self.tiles {
            let bounds = if rebuilding {
                Some(*tile)
            } else {
                changed_bounds(source.pixels, &self.previous, width, *tile)
            };
            if let Some(bounds) = bounds {
                changed_pixels += u64::from(bounds.width) * u64::from(bounds.height);
                dirty.push(bounds);
            }
        }
        if !rebuilding && self.patches && dirty.is_empty() {
            self.timing.encode_ms = began.elapsed().as_secs_f64() * 1000.0;
            return Ok(0);
        }
        // One full upload is cheaper than hundreds of patch commands during a
        // scroll or scene change. Both paths retain a single image placement.
        let full = rebuilding
            || !self.patches
            || changed_pixels > u64::from(width) * u64::from(height) / 2;
        let sent = super::output::frame(out, |wire| {
            if full {
                // Exact baseline comparisons already decided that this upload is
                // necessary. Bypass Painter's digest cache instead of hashing the
                // whole picture again or relying on a hash for pixel equality.
                self.canvas.invalidate();
                self.canvas
                    .draw_rgb(wire, source.pixels, source.size, layout, [0; 32])?;
            } else {
                for tile in &dirty {
                    self.pixels.clear();
                    self.pixels
                        .reserve_exact(tile.width as usize * tile.height as usize * 3);
                    for row in tile.y..tile.y + tile.height {
                        self.pixels.extend_from_slice(pixel_row(
                            source.pixels,
                            width,
                            tile.x,
                            tile.x + tile.width,
                            row,
                        ));
                    }
                    self.canvas.patch_rgb(
                        wire,
                        &self.pixels,
                        (tile.width, tile.height),
                        (tile.x, tile.y),
                    )?;
                }
            }
            Ok(())
        });
        let sent = match sent {
            Ok(sent) => sent,
            Err(error) => {
                self.geometry = None;
                self.canvas.invalidate();
                return Err(error);
            }
        };
        // Exact row comparisons replace full-frame and per-tile hashing on
        // the hot path. Keep one reusable RGB baseline and copy only changed
        // rectangles after their presentation has succeeded.
        if self.previous.len() != source.pixels.len() {
            self.previous = Buffer::copy_from(source.pixels)?;
        } else if full {
            self.previous.copy_from_slice(source.pixels);
        } else {
            for tile in dirty {
                for row in tile.y..tile.y + tile.height {
                    let start = (row as usize * width as usize + tile.x as usize) * 3;
                    let end = start + tile.width as usize * 3;
                    self.previous[start..end].copy_from_slice(&source.pixels[start..end]);
                }
            }
        }
        self.timing.write_ms = sent.write_ms;
        self.timing.encode_ms = (began.elapsed().as_secs_f64() * 1000.0 - sent.write_ms).max(0.0);
        self.timing.bytes = sent.bytes;
        self.geometry = Some(geometry);
        Ok(sent.bytes)
    }
}

fn pixel_row(pixels: &[u8], width: u32, left: u32, right: u32, row: u32) -> &[u8] {
    let offset = (row as usize * width as usize + left as usize) * 3;
    let length = (right - left) as usize * 3;
    &pixels[offset..offset + length]
}

/// A cursor or one changed glyph must not retransmit an entire noisy tile.
/// Keep exact row comparisons, then bound only the pixels that differ.
fn changed_bounds(current: &[u8], previous: &[u8], width: u32, tile: Tile) -> Option<Tile> {
    let (mut left, mut right) = (tile.width, 0);
    let (mut top, mut bottom) = (tile.y + tile.height, tile.y);
    for row in tile.y..tile.y + tile.height {
        let current = pixel_row(current, width, tile.x, tile.x + tile.width, row);
        let previous = pixel_row(previous, width, tile.x, tile.x + tile.width, row);
        if current == previous {
            continue;
        }
        // A changed channel still requires all three bytes of its RGB pixel.
        let first = current.iter().zip(previous).position(|(a, b)| a != b)? / 3;
        let last = current.iter().zip(previous).rposition(|(a, b)| a != b)? / 3;
        left = left.min(first as u32);
        right = right.max(last as u32 + 1);
        top = top.min(row);
        bottom = row + 1;
    }
    (left < right && top < bottom).then_some(Tile {
        x: tile.x + left,
        y: top,
        width: right.saturating_sub(left),
        height: bottom.saturating_sub(top),
    })
}

#[cfg(test)]
mod tests;

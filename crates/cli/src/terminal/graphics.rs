//! Kitty's lossless direct pixel transport. Cell placements retain the source
//! resolution and let the terminal perform the final fit.
use base64::{Engine, engine::general_purpose::STANDARD};
use flate2::{Compression, write::ZlibEncoder};
use std::io::{self, Write};

mod patches;
mod payload;
pub(super) use patches::{patch_query, patch_query_clear, patch_supported};

pub(super) const IMAGE: u32 = 47471;
pub(super) const QUERY: u32 = 47472;

/// Query the compressed RGB profile used by the tiled renderer without
/// creating a visible placement or retaining an image in the terminal.
pub(super) fn query(out: &mut impl Write) -> io::Result<()> {
    let mut compressed = ZlibEncoder::new(Vec::new(), Compression::fast());
    compressed.write_all(&[0, 0, 0])?;
    let payload = STANDARD.encode(compressed.finish()?);
    write!(
        out,
        "\x1b_Ga=q,f=24,s=1,v=1,o=z,t=d,i={QUERY};{payload}\x1b\\"
    )
}

/// Fit inside whole terminal cells using the terminal's physical cell aspect ratio.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Layout {
    pub(super) columns: u16,
    pub(super) rows: u16,
}

impl Layout {
    pub(super) fn fit(
        columns: u16,
        rows: u16,
        pixels: (u16, u16),
        image: (u32, u32),
    ) -> Option<Self> {
        if columns < 2 || rows < 3 || pixels.0 == 0 || pixels.1 == 0 || image.0 == 0 || image.1 == 0
        {
            return None;
        }
        let available = f64::from(rows - 2);
        let cell_ratio =
            (f64::from(pixels.0) / f64::from(columns)) / (f64::from(pixels.1) / f64::from(rows));
        let ratio = f64::from(image.0) / f64::from(image.1) / cell_ratio;
        let width = f64::from(columns).min(available * ratio);
        Some(Self {
            columns: width.floor().max(1.0) as u16,
            rows: (width / ratio).floor().max(1.0) as u16,
        })
    }

    /// Mouse coordinates are cells, starting at zero; the footer is never input.
    pub(super) fn point(self, column: u16, row: u16, display: (f64, f64)) -> Option<(f64, f64)> {
        if column >= self.columns
            || row >= self.rows
            || !display.0.is_finite()
            || !display.1.is_finite()
            || display.0 <= 0.0
            || display.1 <= 0.0
        {
            return None;
        }
        Some((
            (f64::from(column) + 0.5) / f64::from(self.columns) * display.0,
            (f64::from(row) + 0.5) / f64::from(self.rows) * display.1,
        ))
    }
}

pub(super) fn delete(out: &mut impl Write) -> io::Result<()> {
    delete_image(out, IMAGE)?;
    delete_image(out, IMAGE + 2)
}

fn delete_image(out: &mut impl Write, image: u32) -> io::Result<()> {
    write!(out, "\x1b_Ga=d,d=I,i={image},q=2\x1b\\")
}

/// The visible image is never retransmitted: Kitty deletes its placements
/// as soon as an upload reuses its ID. Upload into the other buffer first.
pub(super) struct Painter {
    base: u32,
    origin: (u16, u16),
    current: Option<u32>,
}

impl Painter {
    pub(super) fn at(base: u32, origin: (u16, u16)) -> Self {
        Self {
            base,
            origin,
            current: None,
        }
    }
    pub(super) fn clear(&mut self, out: &mut impl Write) -> io::Result<()> {
        if self.current.is_some() {
            delete_image(out, self.base)?;
            delete_image(out, self.base + 2)?;
            self.current = None;
        }
        Ok(())
    }

    /// Root edits change the current image's data in place.
    pub(super) fn patch_rgb(
        &mut self,
        out: &mut impl Write,
        rgb: &[u8],
        size: (u32, u32),
        origin: (u32, u32),
    ) -> io::Result<()> {
        let image = self
            .current
            .ok_or_else(|| io::Error::other("No image canvas for a root edit"))?;
        patches::upload(out, image, rgb, size, origin, 2)
    }

    pub(super) fn draw_rgb(
        &mut self,
        out: &mut impl Write,
        rgb: &[u8],
        size: (u32, u32),
        layout: Layout,
    ) -> io::Result<()> {
        let image = if self.current == Some(self.base) {
            self.base + 2
        } else {
            self.base
        };
        upload(out, rgb, image, size)?;
        // The new image is placed before the old one is removed.
        write!(
            out,
            "\x1b[{};{}H\x1b_Ga=p,i={image},p=1,c={},r={},C=1,q=2\x1b\\",
            self.origin.1 + 1,
            self.origin.0 + 1,
            layout.columns,
            layout.rows
        )?;
        if let Some(previous) = self.current {
            delete_image(out, previous)?;
        }
        self.current = Some(image);
        Ok(())
    }
}

/// Transmit an image without creating a placement or disturbing the visible ID.
fn upload(
    out: &mut impl Write,
    pixels: &[u8],
    image: u32,
    (width, height): (u32, u32),
) -> io::Result<()> {
    let payload = payload::compress(pixels, width)?;
    // 3072 binary bytes become exactly 4096 base64 bytes. Encode each
    // protocol chunk directly instead of allocating a full-frame String and
    // then copying that String into the reusable presentation buffer.
    let mut chunks = payload.as_slice().chunks(3072).peekable();
    let mut encoded = [0; 4096];
    let mut first = true;
    while let Some(chunk) = chunks.next() {
        let more = u8::from(chunks.peek().is_some());
        if first {
            write!(
                out,
                "\x1b_Ga=t,f=24,s={width},v={height},o=z,t=d,i={image},q=2,m={more};"
            )?;
            first = false;
        } else {
            write!(out, "\x1b_Gm={more};")?;
        }
        let length = STANDARD
            .encode_slice(chunk, &mut encoded)
            .map_err(io::Error::other)?;
        out.write_all(&encoded[..length])?;
        out.write_all(b"\x1b\\")?;
    }
    Ok(())
}

/// Probe without displaying an image. Match the ID as well as the success word.
pub(super) fn supported(reply: &[u8]) -> bool {
    let expected = format!("\x1b_Gi={QUERY};OK\x1b\\");
    reply
        .windows(expected.len())
        .any(|part| part == expected.as_bytes())
}

#[cfg(test)]
#[path = "graphics_tests.rs"]
mod tests;

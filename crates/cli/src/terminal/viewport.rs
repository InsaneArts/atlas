//! Local, lossless zoom and pan. A projection carries the actual integer
//! source crop, so rendering and helper pointer mapping use the same pixels.
use super::graphics::Layout;
use atlas_core::buffer::Buffer;
use atlas_core::error::{Code, Error, Result};

const ZOOMS: [f64; 5] = [1.0, 1.5, 2.0, 3.0, 4.0];
const LABELS: [&str; 5] = ["Fit", "150%", "200%", "300%", "400%"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Change {
    Fit,
    ZoomIn,
    ZoomOut,
    PanLeft,
    PanRight,
    PanUp,
    PanDown,
}

/// Zoom is relative to Fit, not a claim about physical terminal pixel density.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Viewport {
    zoom: usize,
    center: (f64, f64),
    pub(super) generation: u64,
}

impl Default for Viewport {
    fn default() -> Self {
        Self {
            zoom: 0,
            center: (0.5, 0.5),
            generation: 0,
        }
    }
}

impl Viewport {
    pub(super) fn factor(self) -> f64 {
        ZOOMS[self.zoom]
    }

    pub(super) fn label(self) -> &'static str {
        LABELS[self.zoom]
    }

    pub(super) fn changed(self, change: Change) -> Self {
        let mut next = self;
        let step = 0.25 / ZOOMS[self.zoom];
        match change {
            Change::Fit => {
                next.zoom = 0;
                next.center = (0.5, 0.5);
            }
            Change::ZoomIn => next.zoom = (self.zoom + 1).min(ZOOMS.len() - 1),
            Change::ZoomOut => next.zoom = self.zoom.saturating_sub(1),
            Change::PanLeft => next.center.0 -= step,
            Change::PanRight => next.center.0 += step,
            Change::PanUp => next.center.1 -= step,
            Change::PanDown => next.center.1 += step,
        }
        let half = 0.5 / ZOOMS[next.zoom];
        next.center.0 = next.center.0.clamp(half, 1.0 - half);
        next.center.1 = next.center.1.clamp(half, 1.0 - half);
        if next.zoom == self.zoom && next.center == self.center {
            return self;
        }
        // Never change the view under a reused generation. Exhaustion cannot
        // occur during a practical session; refusing it is safer than wrapping.
        let Some(generation) = self.generation.checked_add(1) else {
            return self;
        };
        next.generation = generation;
        next
    }

    pub(super) fn projection(self, source: (u32, u32)) -> Projection {
        if source.0 == 0 || source.1 == 0 {
            return Projection {
                origin: (0, 0),
                size: (0, 0),
                source,
                generation: self.generation,
            };
        }
        let zoom = ZOOMS[self.zoom];
        let side = |source: u32| ((f64::from(source) / zoom).round() as u32).clamp(1, source);
        let size = (side(source.0), side(source.1));
        let origin = |center: f64, source: u32, size: u32| {
            (center * f64::from(source) - f64::from(size) / 2.0)
                .round()
                .clamp(0.0, f64::from(source - size)) as u32
        };
        Projection {
            origin: (
                origin(self.center.0, source.0, size.0),
                origin(self.center.1, source.1, size.1),
            ),
            size,
            source,
            generation: self.generation,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Projection {
    pub(super) origin: (u32, u32),
    pub(super) size: (u32, u32),
    pub(super) source: (u32, u32),
    pub(super) generation: u64,
}

impl Projection {
    pub(super) fn is_full(self) -> bool {
        self.valid() && self.origin == (0, 0) && self.size == self.source
    }

    /// Copy exact source rows without interpolation, scaling or color changes.
    /// The caller can retain the original frame allocation when is_full().
    pub(super) fn crop_rgb(self, rgb: &[u8]) -> Result<Buffer> {
        let invalid = || Error::new(Code::Capture, "Invalid terminal viewport pixels");
        if !self.valid() {
            return Err(invalid());
        }
        let length = |size: (u32, u32)| {
            usize::try_from(size.0)
                .ok()?
                .checked_mul(usize::try_from(size.1).ok()?)?
                .checked_mul(3)
                .filter(|length| *length <= isize::MAX as usize)
        };
        if length(self.source) != Some(rgb.len()) {
            return Err(invalid());
        }
        let capacity = length(self.size).ok_or_else(invalid)?;
        let mut cropped = Buffer::with_capacity(capacity)?;
        use std::io::Write;
        for row in self.origin.1..self.origin.1 + self.size.1 {
            // The complete source length and crop bounds were validated
            // before allocation. Every row lies within that checked buffer.
            let start = (row as usize * self.source.0 as usize + self.origin.0 as usize) * 3;
            let end = start + self.size.0 as usize * 3;
            cropped.write_all(&rgb[start..end])?;
        }
        Ok(cropped)
    }

    /// Map a visible cell center through this exact crop to full helper
    /// display points. The caller must use the projection actually presented,
    /// not a newer requested zoom or pan.
    pub(super) fn point(
        self,
        layout: Layout,
        column: u16,
        row: u16,
        display: (f64, f64),
    ) -> Option<(f64, f64)> {
        if !self.valid()
            || !display.0.is_finite()
            || !display.1.is_finite()
            || display.0 <= 0.0
            || display.1 <= 0.0
        {
            return None;
        }
        let local = layout.point(
            column,
            row,
            (f64::from(self.size.0), f64::from(self.size.1)),
        )?;
        Some((
            ((f64::from(self.origin.0) + local.0) / f64::from(self.source.0)) * display.0,
            ((f64::from(self.origin.1) + local.1) / f64::from(self.source.1)) * display.1,
        ))
    }

    fn valid(self) -> bool {
        self.source.0 > 0
            && self.source.1 > 0
            && self.size.0 > 0
            && self.size.1 > 0
            && self
                .origin
                .0
                .checked_add(self.size.0)
                .is_some_and(|right| right <= self.source.0)
            && self
                .origin
                .1
                .checked_add(self.size.1)
                .is_some_and(|bottom| bottom <= self.source.1)
    }
}

#[cfg(test)]
mod tests;

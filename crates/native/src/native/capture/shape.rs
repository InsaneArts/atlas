//! What part of a stream picture a caller gets, and at what size: the whole
//! display or an area of it, in the stream's own pixels, fitted to a limit.
use super::fit;
use objc2_core_foundation::CGRect;

/// A rectangle of a stream picture, in its pixels from the top left.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Area {
    pub(super) x: usize,
    pub(super) y: usize,
    pub(super) width: usize,
    pub(super) height: usize,
}

/// The part of a `pixels`-sized picture a caller gets, and the size it gets
/// it at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Shape {
    /// None for the whole picture.
    pub(super) area: Option<Area>,
    pub(super) width: usize,
    pub(super) height: usize,
}

impl Shape {
    /// The whole picture of `pixels`, at most `limit` on its longer side.
    pub(super) fn whole(pixels: (usize, usize), limit: Option<u32>) -> Self {
        let (width, height) = fit(pixels.0, pixels.1, limit);
        Self {
            area: None,
            width,
            height,
        }
    }

    /// `area`, in global display points, of a display whose bounds are
    /// `bounds` and whose picture is `pixels` across, at most `limit` on its
    /// longer side: none when it holds no whole pixel of the picture. An
    /// area reaching past the display is cut at its edge.
    pub(super) fn of(
        area: CGRect,
        bounds: CGRect,
        pixels: (usize, usize),
        limit: Option<u32>,
    ) -> Option<Self> {
        if bounds.size.width <= 0.0 || bounds.size.height <= 0.0 {
            return None;
        }
        let scale = pixels.0 as f64 / bounds.size.width;
        let edge = |points: f64, origin: f64, most: usize| {
            ((points - origin) * scale).round().clamp(0.0, most as f64) as usize
        };
        let left = edge(area.origin.x, bounds.origin.x, pixels.0);
        let top = edge(area.origin.y, bounds.origin.y, pixels.1);
        let right = edge(area.origin.x + area.size.width, bounds.origin.x, pixels.0);
        let bottom = edge(area.origin.y + area.size.height, bounds.origin.y, pixels.1);
        let area = Area {
            x: left,
            y: top,
            width: right.checked_sub(left).filter(|width| *width > 0)?,
            height: bottom.checked_sub(top).filter(|height| *height > 0)?,
        };
        let (width, height) = fit(area.width, area.height, limit);
        Some(Self {
            area: Some(area),
            width,
            height,
        })
    }

    /// Whether this is the whole of a `pixels`-sized picture at its own size.
    pub(super) fn is_whole(&self, pixels: (usize, usize)) -> bool {
        self.area.is_none() && (self.width, self.height) == pixels
    }
}

#[cfg(test)]
#[path = "shape_tests.rs"]
mod tests;

//! One screenshot of the helper's display, and how large a saved one is.
use crate::session::Epoch;
use std::{sync::Arc, time::Instant};

pub(crate) struct Frame {
    pub(crate) png: Arc<[u8]>,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) epoch: Epoch,
    pub(crate) at: Instant,
}

/// The longest side of an agent's screenshot, in pixels, unless it asks for
/// another size: enough to see layout, menu titles and controls at about
/// half the cost of the full display.
pub const SCREENSHOT_SIDE: u32 = 1456;

/// How large a saved screenshot is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Size {
    /// At most one pixel per display point and at most [`SCREENSHOT_SIDE`]
    /// pixels on its longer side, so a crop of a Retina display costs no more
    /// than the same crop at one pixel per point.
    Standard,
    /// At most this many pixels on its longer side, never enlarged.
    Longest(u32),
    /// Every pixel the display has.
    Full,
}

impl Size {
    /// The most pixels the longer side may have, if any limit.
    pub(crate) fn longest(self) -> Option<u32> {
        match self {
            Size::Standard => Some(SCREENSHOT_SIDE),
            Size::Longest(side) => Some(side),
            Size::Full => None,
        }
    }
}

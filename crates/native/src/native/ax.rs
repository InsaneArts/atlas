//! Accessibility elements of one application, read *inside* the helper login:
//! its windows and the menus it has open. Listings are read-only; bounded
//! target preparation may request focus. A global
//! messaging timeout and an overall deadline keep an unresponsive app from
//! hanging the probe; depth, count, children fetched and string lengths are
//! capped before anything is copied, and a long text is read only from its
//! start. An element's attributes are asked for in one request. A scan that
//! hit a limit or a communication failure reports itself incomplete.
use atlas_core::screen::elements::ABSENT;
use objc2_application_services::AXError;

mod batch;
mod bring;
mod ffi;
mod keyboard;
mod menus;
mod scan;
mod system;
mod text;
mod tree;

#[cfg(test)]
pub(crate) use atlas_core::screen::elements::{Element, Limits};
pub(crate) use atlas_core::screen::geometry::Bounds;
pub(super) use bring::{bring, check_application, described, target_application, title_bar};
pub(super) use ffi::act;
pub(super) use keyboard::{
    Asked, Focused, Judged, Keyboard, Status, Unread, Within, frontmost, keyboard, remaining,
};
pub(super) use system::dock_item;
pub(super) use system::focused_pid;
pub use system::{DOCK_TIMEOUT, dock};
pub use tree::elements;

/// AX errors meaning the element could not be read at all: the request could
/// not complete (timeout or unresponsive app), the element vanished, or the
/// API is disabled. Any of these makes the whole scan incomplete.
const INCOMPLETE: [AXError; 3] = [
    AXError::CannotComplete,
    AXError::InvalidUIElement,
    AXError::APIDisabled,
];

/// Whether `error` means the element simply lacks the attribute.
fn absent(error: AXError) -> bool {
    ABSENT.contains(&error.0)
}

#[cfg(test)]
mod tests;

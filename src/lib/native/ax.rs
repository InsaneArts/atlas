//! Accessibility elements of one application, read *inside* the helper login:
//! its windows and the menus it has open. Listings are read-only; bounded
//! target preparation may request focus. A global
//! messaging timeout and an overall deadline keep an unresponsive app from
//! hanging the probe; depth, count, children fetched and string lengths are
//! capped before anything is copied, and a long text is read only from its
//! start. An element's attributes are asked for in one request. A scan that
//! hit a limit or a communication failure reports itself incomplete.
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

pub use crate::screen::elements::{Element, Frame, Limits, Snapshot};
pub(super) use bring::{bring, check_application, target_application};
pub(super) use ffi::act;
pub(super) use keyboard::{Focused, Judged, Keyboard, Unread, Within, frontmost, keyboard};
pub(super) use system::dock_item;
pub(super) use system::focused_pid;
pub(crate) use system::{DOCK_TIMEOUT, dock};
pub use tree::elements;

/// AX errors meaning the element could not be read at all: the request could
/// not complete (timeout or unresponsive app), the element vanished, or the
/// API is disabled. Any of these makes the whole scan incomplete.
const INCOMPLETE: [AXError; 3] = [
    AXError::CannotComplete,
    AXError::InvalidUIElement,
    AXError::APIDisabled,
];

/// AX errors meaning the element simply lacks the attribute: no value,
/// unsupported, not implemented.
const ABSENT: [AXError; 3] = [
    AXError::NoValue,
    AXError::AttributeUnsupported,
    AXError::NotImplemented,
];

/// Whether an `"attribute:code"` error key says something went wrong,
/// rather than that the element simply lacks the attribute.
pub(crate) fn telling(key: &str) -> bool {
    let code = key
        .rsplit_once(':')
        .and_then(|(_, code)| code.parse::<i32>().ok());

    code.is_none_or(|code| !ABSENT.iter().any(|absent| absent.0 == code))
}

#[cfg(test)]
mod tests;

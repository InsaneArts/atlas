//! Reads of what the whole login shows rather than one app's tree: the
//! application with keyboard focus, and where the Dock takes clicks.
use super::{batch::Source, ffi, scan::Scan};
use crate::{
    error::{Error, Result},
    native::windows::{self, Dock},
};
use objc2_application_services::{AXError, AXUIElement};
use objc2_core_foundation::CFString;
use std::time::{Duration, Instant};

/// The process of the application with keyboard focus in this login, as
/// Accessibility reports it; None when no application has it. Waits at most
/// `timeout`. This sets the calling process's messaging timeout, which the
/// input service uses for nothing else.
pub fn focused_pid(timeout: Duration) -> Result<Option<i64>> {
    let unreadable = || Error::new("native", "The focused app could not be read");

    let system = ffi::system_wide().ok_or_else(unreadable)?;
    // The system-wide element sets this process's global messaging timeout,
    // which covers the read below. A read without it could wait on an
    // unresponsive app for far longer.
    if ffi::set_timeout(&system, timeout) != AXError::Success {
        return Err(unreadable());
    }

    let name = CFString::from_static_str("AXFocusedApplication");
    let focused = match ffi::copy(&system, &name) {
        Ok(focused) => focused.ok_or_else(unreadable)?,
        Err(AXError::NoValue) => return Ok(None),
        Err(_) => return Err(unreadable()),
    };
    let app = focused
        .downcast_ref::<AXUIElement>()
        .ok_or_else(unreadable)?;
    match ffi::owner(app) {
        Ok(pid) if pid > 0 => Ok(Some(pid.into())),
        _ => Err(unreadable()),
    }
}

/// The longest a read of where the Dock takes clicks may take.
pub const DOCK_TIMEOUT: Duration = Duration::from_millis(250);

/// Most children of the Dock's application element read; it has one list.
const DOCK_CHILDREN: usize = 16;

/// Where the Dock takes clicks: the frames of its lists of icons, read
/// from the process owning the Dock's window in `windows`, waiting at most
/// `timeout`. With no Dock window there is nothing of the Dock's to click.
/// A Dock that shows anything but lists of icons is [`Dock::Other`], and
/// one that cannot all be read in time is [`Dock::Unread`]; either covers
/// all of its window.
pub fn dock(windows: &[windows::Window], timeout: Duration) -> Dock {
    let Some(pid) = windows
        .iter()
        .find(|window| window.is_dock())
        .map(|window| window.pid)
    else {
        return Dock::Icons(Vec::new());
    };
    let Ok(pid) = libc::pid_t::try_from(pid) else {
        return Dock::Unread;
    };

    let Some(app) = ffi::application(pid) else {
        return Dock::Unread;
    };
    // This one element's messaging timeout bounds each read below; nothing
    // else in the process is affected.
    if ffi::set_timeout(&app, timeout) != AXError::Success {
        return Dock::Unread;
    }

    let mut scan = Scan::new(Instant::now() + timeout);
    let mut lists = Vec::new();
    let mut other = false;
    for child in scan.children(&app, "AXChildren", DOCK_CHILDREN) {
        let Some(role) = scan.text(Source::Element(&child), "AXRole") else {
            return Dock::Unread;
        };
        if role != "AXList" {
            // Mission Control's spaces bar and the like take clicks too.
            other = true;
            continue;
        }

        let Some(frame) = scan.frame(Source::Element(&child)) else {
            return Dock::Unread;
        };
        lists.push(windows::Bounds {
            x: frame.x,
            y: frame.y,
            width: frame.width,
            height: frame.height,
        });
    }

    if scan.incomplete {
        return Dock::Unread;
    }
    if other || lists.is_empty() {
        return Dock::Other;
    }

    Dock::Icons(lists)
}

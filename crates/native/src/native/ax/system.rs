//! Reads of what the whole login shows rather than one app's tree: the
//! application with keyboard focus, and where the Dock takes clicks.
use super::{Asked, Bounds, batch::Source, ffi, scan::Scan};
use crate::native::windows as native_windows;
use atlas_core::{
    error::{Code, Error, Result},
    screen::windows::{self, Dock},
};
use objc2_application_services::{AXError, AXUIElement};
use objc2_core_foundation::{CFRetained, CFString};
use std::time::{Duration, Instant};

/// The system-wide attribute naming the application with keyboard focus.
const FOCUSED_APPLICATION: &str = "AXFocusedApplication";

/// The process of the application with keyboard focus in this login, as
/// Accessibility reports it; None when no application has it. Waits at most
/// `timeout`. This sets the calling process's messaging timeout, which the
/// input service's element reads also set before their own reads.
pub(in crate::native) fn focused_pid(timeout: Duration) -> Result<Option<i64>> {
    let unreadable = || Error::new(Code::Native, "The focused app could not be read");
    let failed = |call, status| unreadable().with_source(Asked { call, status });

    let system = ffi::system_wide().ok_or_else(unreadable)?;
    // The system-wide element sets this process's global messaging timeout,
    // which covers the read below. A read without it could wait on an
    // unresponsive app for far longer.
    let bounded = ffi::set_timeout(&system, timeout);
    if bounded != AXError::Success {
        return Err(failed("AXUIElementSetMessagingTimeout", bounded));
    }

    let name = CFString::from_static_str(FOCUSED_APPLICATION);
    let focused = match ffi::copy(&system, &name) {
        Ok(focused) => focused.ok_or_else(unreadable)?,
        Err(AXError::NoValue) => return Ok(None),
        Err(status) => return Err(failed(FOCUSED_APPLICATION, status)),
    };
    let app = focused
        .downcast_ref::<AXUIElement>()
        .ok_or_else(unreadable)?;
    match ffi::owner(app) {
        Ok(pid) if pid > 0 => Ok(Some(pid.into())),
        Ok(_) => Err(unreadable()),
        Err(status) => Err(failed("AXUIElementGetPid", status)),
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
        lists.push(frame);
    }

    if scan.incomplete {
        return Dock::Unread;
    }
    if other || lists.is_empty() {
        return Dock::Other;
    }

    Dock::Icons(lists)
}

/// Find a pressable Dock item, or leave ordinary points to pointer input.
pub(in crate::native) fn dock_item(
    display: windows::Bounds,
    x: f64,
    y: f64,
) -> Result<Option<CFRetained<AXUIElement>>> {
    let windows = native_windows::windows()?;
    let screen = windows::Screen::new(display, dock(&windows, DOCK_TIMEOUT));
    let Some(window) = windows::top_at(&windows, x, y, &screen).filter(|w| w.is_dock()) else {
        return Ok(None);
    };
    let refused = || Error::new(Code::Input, "No pressable Dock item; no click was sent");
    if !matches!(screen.dock(), Dock::Icons(_)) {
        return Err(refused());
    }
    let app = libc::pid_t::try_from(window.pid)
        .ok()
        .and_then(ffi::application)
        .ok_or_else(refused)?;
    if ffi::set_timeout(&app, DOCK_TIMEOUT) != AXError::Success {
        return Err(refused());
    }
    let item = ffi::element_at(&app, x as f32, y as f32)
        .ok()
        .flatten()
        .ok_or_else(refused)?;
    if ffi::set_timeout(&item, DOCK_TIMEOUT) != AXError::Success {
        return Err(refused());
    }
    let mut scan = Scan::new(Instant::now() + DOCK_TIMEOUT);
    let subrole = scan.text(Source::Element(&item), "AXSubrole");
    let frame = scan.frame(Source::Element(&item));
    let press = CFString::from_static_str("AXPress");
    let supported = ffi::actions(&item).ok().flatten().is_some_and(|names| {
        names.len() <= 64
            && names
                .iter()
                .any(|name| name.downcast_ref::<CFString>() == Some(&*press))
    });
    if !scan.open() || !launchable(subrole.as_deref()) || !pressable_at(frame, supported, x, y) {
        return Err(refused());
    }
    Ok(Some(item))
}

fn launchable(subrole: Option<&str>) -> bool {
    matches!(
        subrole,
        Some(
            "AXApplicationDockItem"
                | "AXDocumentDockItem"
                | "AXFolderDockItem"
                | "AXMinimizedWindowDockItem"
                | "AXURLDockItem"
        )
    )
}

fn pressable_at(frame: Option<Bounds>, supported: bool, x: f64, y: f64) -> bool {
    supported && frame.is_some_and(|frame| frame.contains(x, y))
}

#[cfg(test)]
mod tests;

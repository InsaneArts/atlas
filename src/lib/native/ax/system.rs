//! Reads of what the whole login shows rather than one app's tree: the
//! application with keyboard focus, and where the Dock takes clicks.
use super::{batch::Source, ffi, scan::Scan};
use crate::{
    error::{Code, Error, Result},
    native::windows::{self, Dock},
};
use objc2_application_services::{AXError, AXUIElement};
use objc2_core_foundation::{CFRetained, CFString};
use std::time::{Duration, Instant};

/// The process of the application with keyboard focus in this login, as
/// Accessibility reports it; None when no application has it. Waits at most
/// `timeout`. This sets the calling process's messaging timeout, which the
/// input service uses for nothing else.
pub fn focused_pid(timeout: Duration) -> Result<Option<i64>> {
    let unreadable = || Error::new(Code::Native, "The focused app could not be read");

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

/// Find a pressable Dock item, or leave ordinary points to pointer input.
pub(crate) fn dock_item(
    display: windows::Bounds,
    x: f64,
    y: f64,
) -> Result<Option<CFRetained<AXUIElement>>> {
    let windows = windows::windows()?;
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

fn pressable_at(frame: Option<super::Frame>, supported: bool, x: f64, y: f64) -> bool {
    supported
        && frame.is_some_and(|f| x >= f.x && y >= f.y && x < f.x + f.width && y < f.y + f.height)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dock_gaps_and_separators_are_not_pressable() {
        let frame = || {
            Some(super::super::Frame {
                x: 1120.0,
                y: 920.0,
                width: 16.0,
                height: 40.0,
            })
        };
        assert!(pressable_at(frame(), true, 1128.0, 940.0));
        assert!(
            !pressable_at(frame(), true, 1138.0, 940.0),
            "gap beside an item"
        );
        assert!(
            !pressable_at(frame(), false, 1128.0, 940.0),
            "separator without AXPress"
        );
        assert!(!pressable_at(None, true, 1138.0, 940.0));
    }

    #[test]
    fn only_launchable_dock_subroles_are_pressable_even_with_axpress() {
        for (subrole, expected) in [
            (Some("AXApplicationDockItem"), true),
            (Some("AXDocumentDockItem"), true),
            (Some("AXFolderDockItem"), true),
            (Some("AXMinimizedWindowDockItem"), true),
            (Some("AXURLDockItem"), true),
            (Some("AXSeparatorDockItem"), false),
            (Some("AXTrashDockItem"), false),
            (Some("AXDockExtraDockItem"), false),
            (Some("AXUnknownDockItem"), false),
            (None, false),
        ] {
            let frame = Some(super::super::Frame {
                x: 1120.0,
                y: 920.0,
                width: 40.0,
                height: 40.0,
            });
            assert_eq!(
                launchable(subrole) && pressable_at(frame, true, 1138.0, 940.0),
                expected,
                "{subrole:?}"
            );
        }
    }
}

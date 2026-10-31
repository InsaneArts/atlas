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
    read_dock(windows, timeout).map_or(Dock::Unread, |(_, dock, _)| dock)
}

type List = (CFRetained<AXUIElement>, windows::Bounds);

fn read_dock(windows: &[windows::Window], timeout: Duration) -> Result<(Scan, Dock, Vec<List>)> {
    let mut scan = Scan::new(Instant::now() + timeout);
    let unread = || refused("The Dock could not be read");
    let Some(pid) = windows
        .iter()
        .find(|window| window.is_dock())
        .map(|window| window.pid)
    else {
        return Ok((scan, Dock::Icons(Vec::new()), Vec::new()));
    };
    let Ok(pid) = libc::pid_t::try_from(pid) else {
        return Err(unread());
    };

    let Some(app) = ffi::application(pid) else {
        return Err(unread());
    };
    // This one element's messaging timeout bounds each read below; nothing
    // else in the process is affected.
    if ffi::set_timeout(&app, timeout) != AXError::Success {
        return Err(unread());
    }

    let mut lists = Vec::new();
    let mut other = false;
    for child in scan.children(&app, "AXChildren", DOCK_CHILDREN) {
        let Some(role) = scan.text(Source::Element(&child), "AXRole") else {
            return Err(unread());
        };
        if role != "AXList" {
            // Mission Control's spaces bar and the like take clicks too.
            other = true;
            continue;
        }

        let Some(frame) = scan.frame(Source::Element(&child)) else {
            return Err(unread());
        };
        lists.push((
            child,
            windows::Bounds {
                x: frame.x,
                y: frame.y,
                width: frame.width,
                height: frame.height,
            },
        ));
    }

    if scan.incomplete {
        return Err(unread());
    }
    if other || lists.is_empty() {
        return Ok((scan, Dock::Other, lists));
    }

    let dock = Dock::Icons(lists.iter().map(|(_, frame)| frame.clone()).collect());
    Ok((scan, dock, lists))
}

/// Probe a pointer press, or press one Dock item. AX references stay on this
/// worker; the caller keeps the input engine locked until it joins.
pub(crate) fn dock_click(
    display: windows::Bounds,
    x: f64,
    y: f64,
    press: bool,
    guard: impl Fn() -> Result<()>,
) -> Result<bool> {
    guard()?;
    let windows = windows::windows()?;
    let (mut scan, dock, lists) = read_dock(&windows, DOCK_TIMEOUT)?;
    let screen = windows::Screen::new(display, dock);
    route_click(&windows, &screen, x, y, || {
        if !press {
            return Ok(());
        }
        if !matches!(screen.dock(), Dock::Icons(_)) {
            return Err(refused("The Dock is showing an overlay"));
        }
        let mut items = Vec::new();
        for (list, _) in lists {
            for item in scan.children(&list, "AXChildren", 256) {
                let frame = scan
                    .frame(Source::Element(&item))
                    .ok_or_else(|| refused("The Dock items could not be read"))?;
                items.push((
                    item,
                    windows::Bounds {
                        x: frame.x,
                        y: frame.y,
                        width: frame.width,
                        height: frame.height,
                    },
                ));
            }
        }
        if scan.incomplete || !scan.open() {
            return Err(refused("The Dock items could not be read"));
        }

        press_item(&items, x, y, |item| {
            let name = CFString::from_static_str("AXPress");
            let supported = ffi::actions(item).ok().flatten().is_some_and(|names| {
                names.len() <= 64
                    && names
                        .iter()
                        .any(|action| action.downcast_ref::<CFString>() == Some(&*name))
            });
            if !supported || !scan.open() {
                return Err(refused("That part of the Dock is not a pressable item"));
            }
            if ffi::set_timeout(item, DOCK_TIMEOUT) != AXError::Success {
                return Err(refused("The Dock item could not be prepared"));
            }
            guard()?;
            press_result(ffi::act(item, &name))
        })?;
        Ok(())
    })
}

fn route_click(
    windows: &[windows::Window],
    screen: &windows::Screen,
    x: f64,
    y: f64,
    press: impl FnOnce() -> Result<()>,
) -> Result<bool> {
    if !windows::top_at(windows, x, y, screen).is_some_and(windows::Window::is_dock) {
        return Ok(false);
    }
    press()?;
    Ok(true)
}

fn press_item<T>(
    items: &[(T, windows::Bounds)],
    x: f64,
    y: f64,
    press: impl FnOnce(&T) -> Result<()>,
) -> Result<()> {
    let mut matching = items.iter().filter(|(_, frame)| frame.contains(x, y));
    let item = matching
        .next()
        .filter(|_| matching.next().is_none())
        .ok_or_else(|| refused("There is no single Dock item at that point"))?;
    press(&item.0)
}

fn press_result(status: AXError) -> Result<()> {
    if status == AXError::Success {
        Ok(())
    } else {
        Err(Error::new(Code::Uncertain, "The Dock did not acknowledge the press").uncertain())
    }
}

fn refused(why: &str) -> Error {
    Error::new(Code::Input, format!("{why}; no click was sent"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bounds(x: f64, y: f64, width: f64, height: f64) -> windows::Bounds {
        windows::Bounds {
            x,
            y,
            width,
            height,
        }
    }

    #[test]
    fn a_dock_item_is_pressed_the_shelf_refuses_and_other_points_post() {
        let display = bounds(0.0, 0.0, 1000.0, 800.0);
        let listed = vec![windows::Window {
            number: 1,
            pid: 2,
            owner: Some("Dock".into()),
            title: None,
            layer: 20,
            alpha: 1.0,
            bounds: display.clone(),
        }];
        let screen = windows::Screen::new(
            display,
            Dock::Icons(vec![bounds(300.0, 740.0, 400.0, 50.0)]),
        );
        let items = [
            ("Finder", bounds(300.0, 740.0, 50.0, 50.0)),
            ("Notes", bounds(360.0, 740.0, 50.0, 50.0)),
        ];
        for (x, y, wanted) in [
            (380.0, 760.0, "Notes"),
            (355.0, 760.0, "refused"),
            (290.0, 760.0, "refused"),
            (100.0, 100.0, "posted"),
        ] {
            let mut pressed = None;
            let outcome = route_click(&listed, &screen, x, y, || {
                press_item(&items, x, y, |item| {
                    pressed = Some(*item);
                    Ok(())
                })
            });
            let actual = match outcome {
                Ok(true) => pressed.unwrap(),
                Ok(false) => {
                    assert_eq!(pressed, None);
                    "posted"
                }
                Err(error) => {
                    assert!(!error.uncertain);
                    assert_eq!(pressed, None);
                    "refused"
                }
            };
            assert_eq!(actual, wanted);
        }
    }

    #[test]
    fn a_failed_dock_press_is_uncertain_and_never_retried() {
        let items = [(1, bounds(0.0, 0.0, 50.0, 50.0))];
        for status in [
            AXError::CannotComplete,
            AXError::Failure,
            AXError::InvalidUIElement,
        ] {
            let mut calls = 0;
            let error = press_item(&items, 20.0, 20.0, |_| {
                calls += 1;
                press_result(status)
            })
            .unwrap_err();
            assert!(error.uncertain);
            assert_eq!(error.code, Code::Uncertain);
            assert_eq!(calls, 1);
        }
        assert!(press_result(AXError::Success).is_ok());
    }
}

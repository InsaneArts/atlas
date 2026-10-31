//! The depth-first read of one app's elements: where it starts (the app's
//! windows, its menu bar titles and the menus it has open), the walk down
//! from there, and what each element met on the way reports.
use super::{
    ABSENT, Element, Frame, Limits, Snapshot, Stop,
    batch::{Batch, Source},
    ffi,
    menus::{Drawn, skip_nested_menu},
    scan::Scan,
};
use crate::{
    error::{Code, Error, Result},
    native::{
        self,
        windows::{self, MENU_LAYER},
    },
};
use objc2_application_services::{AXError, AXUIElement};
use objc2_core_foundation::{CFRetained, CFString};
use std::{
    collections::HashSet,
    time::{Duration, Instant},
};

/// Most menu bar titles read for an app.
const MENU_BAR_ITEMS: usize = 64;

/// An element waiting to be read: how deep it sits, the frame of the
/// window or open menu it belongs to, and whether it is in an open menu.
struct Visit {
    element: CFRetained<AXUIElement>,
    depth: usize,
    window: Option<Frame>,
    menu: bool,
}

impl Visit {
    /// Where a walk starts: a window or menu read at the top, with its frame.
    fn root(element: CFRetained<AXUIElement>, frame: Option<Frame>, menu: bool) -> Self {
        Self {
            element,
            depth: 0,
            window: frame,
            menu,
        }
    }
}

/// Depth-first elements of `pid`'s windows, its menu bar titles when it is
/// the active app, then the menus it has open (a menu bar, status item or
/// context menu). Closed menus are left out: nothing in them is on screen.
pub fn elements(pid: i32, limits: Limits) -> Result<Snapshot> {
    // The system-wide element sets this process's global messaging timeout,
    // which covers every read below.
    if let Some(system) = ffi::system_wide() {
        ffi::set_timeout(&system, Duration::from_secs(1));
    }
    let app = ffi::application(pid)
        .ok_or_else(|| Error::new(Code::Elements, "No accessibility element for that process"))?;

    let mut scan = Scan::new(Instant::now() + limits.time);
    // Some browsers build their page tree when the application's role is read.
    let role = if scan.open() {
        ffi::copy(&app, &CFString::from_static_str("AXRole"))
            .map(|role| role.is_some_and(|role| role.downcast_ref::<CFString>().is_some()))
    } else {
        Ok(false)
    };
    let windows = scan.children(&app, "AXWindows", limits.count);
    let windows_error = scan.last_error.take();
    let windows = scan.shown(&app, windows);

    // An app that answers nothing at all, not even its role, is unreachable;
    // report why, not "empty".
    if let Err(error) = reachable(role, !windows.is_empty(), scan.last_error.or(windows_error)) {
        let exists = pid > 0
            && !matches!(
                nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid), None),
                Err(nix::errno::Errno::ESRCH)
            );

        return Err(unreachable(error, native::accessibility_trusted(), exists));
    }

    let mut roots: Vec<Visit> = windows
        .into_iter()
        .map(|window| {
            let frame = scan.frame(Source::Element(&window));
            Visit::root(window, frame, false)
        })
        .collect();
    // Elements hit through a click-through window, which the scan below
    // must reach; any it does not might hold an unseen match.
    let mut unvisited: Vec<CFRetained<AXUIElement>> = Vec::new();

    let listed = match windows::windows() {
        Ok(listed) => listed,
        Err(_) => {
            scan.stop(Stop::Unread);
            Vec::new()
        }
    };
    menu_bar(&mut scan, &app, &mut roots);
    open_menus(&mut scan, &app, pid, &mut roots, &mut unvisited, &listed);
    let found = walk(
        &mut scan,
        roots,
        limits,
        &mut unvisited,
        &listed,
        i64::from(pid),
    );

    if !unvisited.is_empty() {
        scan.stop(Stop::Unread);
    }

    // A request that started before the deadline may have ended after it.
    scan.open();

    Ok(Snapshot {
        elements: found,
        incomplete: scan.incomplete,
        stops: scan.stops,
        errors: scan.errors,
    })
}

/// The menu bar shows the active app's titles, which are on screen and
/// take clicks; the closed menus under them are left out of the walk. A
/// bar, or the flag that says whether it is this app's, that fails to read
/// might hide an unseen match.
fn menu_bar(scan: &mut Scan, app: &AXUIElement, roots: &mut Vec<Visit>) {
    scan.last_error = None;
    let frontmost = scan.flag(Source::Element(app), "AXFrontmost");
    if frontmost.is_none() && scan.last_error.is_some_and(|code| !ABSENT.contains(&code)) {
        scan.stop(Stop::Unread);
    }

    if frontmost != Some(true) {
        return;
    }

    scan.last_error = None;
    let bar = scan
        .attribute(Source::Element(app), "AXMenuBar")
        .and_then(|bar| bar.downcast::<AXUIElement>().ok());
    match bar {
        Some(bar) => {
            let frame = scan.frame(Source::Element(&bar));
            for item in scan.children(&bar, "AXChildren", MENU_BAR_ITEMS) {
                roots.push(Visit::root(item, frame.clone(), false));
            }
        }
        None => {
            if scan.last_error.is_some_and(|code| !ABSENT.contains(&code)) {
                scan.stop(Stop::Unread);
            }
        }
    }
}

/// An open menu is a window of its own at the menu layer. A window there
/// that cannot be read might hold an unseen match, so it leaves the scan
/// incomplete; one that shows nothing of its own adds the elements hit
/// through it to `unvisited`.
fn open_menus(
    scan: &mut Scan,
    app: &AXUIElement,
    pid: i32,
    roots: &mut Vec<Visit>,
    unvisited: &mut Vec<CFRetained<AXUIElement>>,
    listed: &[windows::Window],
) {
    let menus = listed.iter().filter(|window| {
        window.pid == i64::from(pid) && window.layer == MENU_LAYER && window.alpha > 0.0
    });
    for menu in menus {
        let bounds = Frame::from(&menu.bounds);

        // A window the app lists among its windows is read already.
        if roots.iter().any(|root| {
            root.window
                .as_ref()
                .is_some_and(|frame| frame.matches(&bounds))
        }) {
            continue;
        }

        let read: Vec<Frame> = roots
            .iter()
            .filter_map(|root| root.window.clone())
            .collect();
        match scan.drawn_at(app, &bounds, &read) {
            Some(Drawn::Menu(element)) => roots.push(Visit::root(element, Some(bounds), true)),
            Some(Drawn::Window(element)) => roots.push(Visit::root(element, Some(bounds), false)),
            Some(Drawn::Through(hits)) => unvisited.extend(hits),
            None => scan.stop(Stop::Unread),
        }
    }
}

/// Every element under `roots`, depth first, within `limits`. Each element
/// reached is struck from `unvisited`.
fn walk(
    scan: &mut Scan,
    roots: Vec<Visit>,
    limits: Limits,
    unvisited: &mut Vec<CFRetained<AXUIElement>>,
    listed: &[windows::Window],
    pid: i64,
) -> Vec<Element> {
    let open_menus: Vec<Frame> = roots
        .iter()
        .filter(|root| root.menu)
        .filter_map(|root| root.window.clone())
        .collect();
    let mut found = Vec::new();
    let mut stack: Vec<Visit> = roots.into_iter().rev().collect();
    // Each element is read once: an app can list an element under itself
    // (TextEdit lists its own application element as a child), and without
    // this the scan would walk that loop until its budget ran out.
    let mut seen: HashSet<CFRetained<AXUIElement>> = HashSet::new();

    while let Some(visit) = stack.pop() {
        if found.len() >= limits.count {
            scan.stop(Stop::Count);
            break;
        }

        if !scan.open() {
            break;
        }

        if !seen.insert(visit.element.clone()) {
            continue;
        }

        let item = &*visit.element;
        unvisited.retain(|hit| **hit != *item);
        scan.unreadable.clear();
        scan.truncated.clear();

        let values = scan.batch(item);
        let Some(element) = read(
            scan,
            &visit,
            values.as_ref(),
            found.len(),
            &open_menus,
            listed,
            pid,
        ) else {
            continue;
        };
        let window = element.window.clone();
        let bar_title = element.role.as_deref() == Some("AXMenuBarItem");
        found.push(element);

        // The menu under a menu bar title is closed, or read as a window of
        // its own above: nothing in it is on screen here.
        if bar_title {
            continue;
        }

        // An element that did not answer is not asked for its children.
        if scan.stalled(values.as_ref()) {
            continue;
        }

        if visit.depth + 1 >= limits.depth {
            if scan.has_children(item, "AXChildren") {
                scan.stop(Stop::Depth);
            }
            continue;
        }

        let budget = limits.count.saturating_sub(found.len() + stack.len());
        let children = scan.children(item, "AXChildren", budget);
        for child in children.into_iter().rev() {
            stack.push(Visit {
                element: child,
                depth: visit.depth + 1,
                window: window.clone(),
                menu: visit.menu,
            });
        }
    }

    found
}

/// What the element in `visit` reports, the `index`th found, its
/// attributes taken from `values` where the batch holds them. None for a
/// nested menu left out of the scan (see [`skip_nested_menu`]) or one whose
/// frame failed to read.
fn read(
    scan: &mut Scan,
    visit: &Visit,
    values: Option<&Batch>,
    index: usize,
    open_menus: &[Frame],
    listed: &[windows::Window],
    pid: i64,
) -> Option<Element> {
    let item = &*visit.element;
    let from = Source::of(item, values);
    let role = scan.text(from, "AXRole");
    scan.last_error = None;
    let frame = scan.frame(from);

    if visit.depth > 0 && role.as_deref() == Some("AXMenu") {
        // A frame that failed to read (rather than being absent) leaves
        // an open menu possible: it might hold an unseen match.
        if frame.is_none() && scan.last_error.is_some_and(|code| !ABSENT.contains(&code)) {
            scan.stop(Stop::Unread);
            return None;
        }

        if skip_nested_menu(frame.as_ref(), open_menus) {
            return None;
        }
    }

    let window = sheet_window(
        role.as_deref(),
        frame.as_ref(),
        visit.window.clone(),
        listed,
        pid,
    );
    let selectable = matches!(role.as_deref(), Some("AXRow" | "AXCell"));
    let title = scan.text(from, "AXTitle");
    let description = scan.text(from, "AXDescription");
    scan.last_error = None;
    let subrole = scan.text(from, "AXSubrole");
    let value = if scan.last_error.is_none_or(|error| ABSENT.contains(&error)) {
        scan.value(item, values, role.as_deref(), subrole.as_deref())
    } else {
        scan.unreadable.push("AXValue".to_owned());
        None
    };
    let mut element = Element {
        index,
        depth: visit.depth,
        window,
        role,
        title,
        description,
        value,
        subrole,
        identifier: scan.text(from, "AXIdentifier"),
        enabled: scan.flag(from, "AXEnabled"),
        focused: scan.flag(from, "AXFocused"),
        selected: selectable.then(|| scan.flag(from, "AXSelected")).flatten(),
        frame,
        unreadable: Vec::new(),
        truncated: Vec::new(),
        menu: visit.menu,
        mark: None,
    };
    if element.role.as_deref() == Some("AXMenuItem") {
        element.mark = scan
            .text(from, "AXMenuItemMarkChar")
            .filter(|mark| !mark.is_empty());
    }

    element.unreadable = std::mem::take(&mut scan.unreadable);
    element.truncated = std::mem::take(&mut scan.truncated);
    Some(element)
}

/// The window frame an element is judged visible against: a sheet's own
/// frame when CG lists it separately; otherwise its inherited parent frame,
/// when that frame uniquely identifies a listed window. No geometry guessing.
pub(super) fn sheet_window(
    role: Option<&str>,
    frame: Option<&Frame>,
    inherited: Option<Frame>,
    listed: &[windows::Window],
    pid: i64,
) -> Option<Frame> {
    match (role, frame) {
        (Some("AXSheet"), Some(frame)) => {
            match windows::described(listed, pid, frame, None, windows::Kind::Other) {
                windows::Listed::One(_) => Some(frame.clone()),
                windows::Listed::Several => None,
                windows::Listed::Missing => {
                    inherited.filter(|frame| windows::owning_window(listed, pid, frame).is_some())
                }
            }
        }
        _ => inherited,
    }
}

pub(super) fn reachable(
    role: std::result::Result<bool, AXError>,
    has_windows: bool,
    error: Option<AXError>,
) -> std::result::Result<(), Option<AXError>> {
    if has_windows || role == Ok(true) {
        Ok(())
    } else {
        Err(error.filter(|error| !ABSENT.contains(error)).or(role.err()))
    }
}

/// Why an app answered nothing, from the last AX error its reads returned,
/// whether this process may use Accessibility, and whether the process
/// exists. Only a missing permission is reported as one.
pub(super) fn unreachable(error: Option<AXError>, trusted: bool, exists: bool) -> Error {
    if !trusted || error == Some(AXError::APIDisabled) {
        return Error::new(
            Code::Permission,
            "The app's accessibility tree is unavailable; check Accessibility for the helper",
        );
    }

    if !exists || error == Some(AXError::InvalidUIElement) {
        return Error::new(Code::Elements, "No app is running with that process ID");
    }

    if error == Some(AXError::CannotComplete) {
        return Error::new(
            Code::Elements,
            "The app is not responding to Accessibility (it may be busy, or not an app with \
             windows); try again",
        );
    }

    match error {
        Some(error) => Error::new(
            Code::Elements,
            format!(
                "The app's accessibility tree could not be read (error {})",
                error.0
            ),
        ),
        None => Error::new(Code::Elements, "The app shows no accessibility tree"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        native::ax::batch::{BATCH, Slot},
        select::ElementFilter,
    };
    use objc2_core_foundation::{CFBoolean, CFNumber, CFString, Type};

    #[test]
    fn a_malformed_subrole_withholds_the_value_without_reaching_either_getter() {
        for characters in [0, 5000] {
            let batch = Batch {
                slots: BATCH
                    .iter()
                    .map(|name| match *name {
                        "AXRole" => Slot::Value(CFString::from_static_str("AXTextField").into()),
                        "AXSubrole" => Slot::Value(CFBoolean::new(true).retain().into()),
                        "AXNumberOfCharacters" => {
                            Slot::Value(CFNumber::new_isize(characters).into())
                        }
                        _ => Slot::Empty,
                    })
                    .collect(),
                stalled: false,
            };
            let visit = Visit::root(ffi::system_wide().unwrap(), None, false);
            let mut scan = Scan::new(Instant::now());
            let element = read(&mut scan, &visit, Some(&batch), 0, &[], &[], 812).unwrap();
            assert!(element.subrole.is_none());
            assert!(element.value.is_none());
            assert!(
                scan.stops.is_empty(),
                "either getter would record the expired deadline"
            );
            assert_eq!(element.unreadable, ["AXValue"]);
        }
    }

    #[test]
    fn a_withheld_static_text_value_remains_a_possible_duplicate_without_a_getter() {
        let batch = Batch {
            slots: BATCH
                .iter()
                .map(|name| match *name {
                    "AXRole" => Slot::Value(CFString::from_static_str("AXStaticText").into()),
                    "AXSubrole" => Slot::Failed(AXError::Failure),
                    _ => Slot::Empty,
                })
                .collect(),
            stalled: false,
        };
        let visit = Visit::root(ffi::system_wide().unwrap(), None, false);
        let mut scan = Scan::new(Instant::now());
        let hidden = read(&mut scan, &visit, Some(&batch), 0, &[], &[], 812).unwrap();
        assert!(hidden.value.is_none());
        assert!(
            scan.stops.is_empty(),
            "a getter would record the expired deadline"
        );
        assert!(!scan.errors.keys().any(|name| name.starts_with("AXValue:")));
        let visible = Element {
            index: 1,
            value: Some("Save".into()),
            unreadable: Vec::new(),
            ..hidden.clone()
        };
        let filter = ElementFilter {
            name: Some("Save".into()),
            ..Default::default()
        };
        assert!(filter.matches(&visible));
        assert!(!filter.matches(&hidden));
        assert_eq!(
            [hidden, visible]
                .iter()
                .filter(|element| filter.might_match(element))
                .count(),
            2
        );
    }
}

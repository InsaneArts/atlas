//! The depth-first read of one app's elements: where it starts (the app's
//! windows, its menu bar titles and the menus it has open), the walk down
//! from there, what each element met on the way reports, and the object
//! each one is with the element it was reached from.
use super::{
    absent,
    batch::{Batch, Source},
    ffi,
    menus::{Drawn, skip_nested_menu},
    scan::Scan,
};
use crate::native;
use atlas_core::{
    error::{Code, Error, Result},
    screen::{
        elements::{Element, Limits, Snapshot, Stop},
        geometry::Bounds,
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
/// window or open menu it belongs to, whether it is in an open menu, and
/// the index of the element it was reached from.
struct Visit {
    element: CFRetained<AXUIElement>,
    depth: usize,
    window: Option<Bounds>,
    menu: bool,
    parent: Option<usize>,
}

impl Visit {
    /// Where a walk starts: a window or menu read at the top, with its frame.
    fn root(element: CFRetained<AXUIElement>, frame: Option<Bounds>, menu: bool) -> Self {
        Self {
            element,
            depth: 0,
            window: frame,
            menu,
            parent: None,
        }
    }
}

/// The object an element of a snapshot is, the index of the element it was
/// reached from (none where the walk started), the identifying
/// attributes its read left unproven, and those whose getter failed.
pub(super) struct Traced {
    pub(super) object: CFRetained<AXUIElement>,
    pub(super) parent: Option<usize>,
    pub(super) unproven: Vec<&'static str>,
    pub(super) refused: Vec<&'static str>,
}

/// One element on a path read again, the identifying attributes that read
/// left unproven and those whose getter failed, and the object it was read
/// from: made on this thread from the app's own element, so it may be asked
/// and acted on here.
pub(super) struct Retraced {
    pub(super) element: Element,
    pub(super) unproven: Vec<&'static str>,
    pub(super) refused: Vec<&'static str>,
    pub(super) object: CFRetained<AXUIElement>,
}

/// One traversal's snapshot and, beside each of its elements by index, the
/// object it is.
pub(super) struct Observed {
    pub(super) snapshot: Snapshot,
    pub(super) objects: Vec<Traced>,
}

/// Depth-first elements of `pid`'s windows, its menu bar titles when it is
/// the active app, then the menus it has open (a menu bar, status item or
/// context menu). Closed menus are left out: nothing in them is on screen.
///
/// # Errors
///
/// Rejects an invalid process and returns accessibility-observation failures. Incomplete
/// scans retain their truncation reasons instead of claiming complete coverage.
pub fn elements(pid: i32, limits: Limits) -> Result<Snapshot> {
    observe(pid, limits).map(|observed| observed.snapshot)
}

/// [`elements`], with the object each element is.
pub(super) fn observe(pid: i32, limits: Limits) -> Result<Observed> {
    let app = application(pid)?;
    let mut scan = Scan::new(Instant::now() + limits.time);
    // Some browsers build their page tree when the application's role is read.
    let role = if scan.open() {
        scan.bound(&app);
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

    let listed = match native::windows::windows() {
        Ok(listed) => listed,
        Err(_) => {
            scan.stop(Stop::Unread);
            Vec::new()
        }
    };
    let (roots, mut unvisited) = starts(&mut scan, &app, pid, windows, &listed);
    let (found, objects) = walk(
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

    Ok(Observed {
        snapshot: Snapshot {
            elements: found,
            incomplete: scan.incomplete,
            stops: scan.stops,
            errors: scan.errors,
        },
        objects,
    })
}

/// The accessibility element of app `pid`.
fn application(pid: i32) -> Result<CFRetained<AXUIElement>> {
    ffi::application(pid)
        .ok_or_else(|| Error::new(Code::Elements, "No accessibility element for that process"))
}

/// Where a walk of app `pid` starts: its `windows`, its menu bar titles
/// when it is in front, and the menus it has open; and the elements hit
/// through a click-through window, which the walk must reach, since any it
/// does not might hold an unseen match.
fn starts(
    scan: &mut Scan,
    app: &AXUIElement,
    pid: i32,
    windows: Vec<CFRetained<AXUIElement>>,
    listed: &[windows::Window],
) -> (Vec<Visit>, Vec<CFRetained<AXUIElement>>) {
    let mut roots: Vec<Visit> = windows
        .into_iter()
        .map(|window| {
            let frame = scan.frame(Source::Element(&window));
            Visit::root(window, frame, false)
        })
        .collect();
    let mut unvisited = Vec::new();
    menu_bar(scan, app, &mut roots);
    open_menus(scan, app, pid, &mut roots, &mut unvisited, listed);
    (roots, unvisited)
}

/// The frames of the menus open among `roots`.
fn menu_frames(roots: &[Visit]) -> Vec<Bounds> {
    roots
        .iter()
        .filter(|root| root.menu)
        .filter_map(|root| root.window.clone())
        .collect()
}

/// The elements of app `pid` along `path`, the objects from where a walk
/// starts down to one it met, read again as a walk reads them within
/// `time`: the first proven to be where a walk starts now, and each other
/// proven to be among the children of the one before it now. None when a
/// step is not proven, whatever kept it from being.
///
/// # Errors
///
/// Returns failures to make the app's element or to list the windows.
pub(super) fn retrace(
    pid: i32,
    path: &[&AXUIElement],
    time: Duration,
) -> Result<Option<Vec<Retraced>>> {
    let app = application(pid)?;
    let mut scan = Scan::new(Instant::now() + time);
    let listed = native::windows::windows()?;
    let windows = scan.children(&app, "AXWindows", Limits::default().count);
    let windows = scan.shown(&app, windows);
    let (roots, _) = starts(&mut scan, &app, pid, windows, &listed);
    let open_menus = menu_frames(&roots);
    let Some((first, rest)) = path.split_first() else {
        return Ok(None);
    };
    let Some(mut visit) = roots.into_iter().find(|root| *root.element == **first) else {
        return Ok(None);
    };

    let mut found: Vec<Retraced> = Vec::new();
    for next in rest.iter().map(Some).chain([None]) {
        scan.unreadable.clear();
        scan.truncated.clear();
        scan.unproven.clear();
        scan.refused.clear();
        let values = scan.batch(&visit.element);
        let Some(element) = read(
            &mut scan,
            &visit,
            values.as_ref(),
            found.len(),
            &open_menus,
            &listed,
            i64::from(pid),
        ) else {
            return Ok(None);
        };
        let window = element.window.clone();
        found.push(Retraced {
            element,
            unproven: std::mem::take(&mut scan.unproven),
            refused: std::mem::take(&mut scan.refused),
            object: visit.element.clone(),
        });

        let Some(next) = next else { break };
        if scan.stalled(values.as_ref()) {
            return Ok(None);
        }
        let children = scan.children(&visit.element, "AXChildren", Limits::default().count);
        let Some(child) = children.into_iter().find(|child| **child == **next) else {
            return Ok(None);
        };
        visit = Visit {
            element: child,
            depth: visit.depth + 1,
            window,
            menu: visit.menu,
            parent: found.len().checked_sub(1),
        };
    }

    Ok(Some(found))
}

/// The menu bar shows the active app's titles, which are on screen and
/// take clicks; the closed menus under them are left out of the walk. A
/// bar, or the flag that says whether it is this app's, that fails to read
/// might hide an unseen match.
fn menu_bar(scan: &mut Scan, app: &AXUIElement, roots: &mut Vec<Visit>) {
    scan.last_error = None;
    let frontmost = scan.flag(Source::Element(app), "AXFrontmost");
    if frontmost.is_none() && scan.read_failed() {
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
        None if scan.read_failed() => scan.stop(Stop::Unread),
        None => {}
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
        let bounds = &menu.bounds;

        // A window the app lists among its windows is read already.
        if roots.iter().any(|root| {
            root.window
                .as_ref()
                .is_some_and(|frame| frame.matches(bounds))
        }) {
            continue;
        }

        let read: Vec<Bounds> = roots
            .iter()
            .filter_map(|root| root.window.clone())
            .collect();
        match scan.drawn_at(app, bounds, &read) {
            Some(Drawn::Menu(element)) => {
                roots.push(Visit::root(element, Some(bounds.clone()), true));
            }
            Some(Drawn::Window(element)) => {
                roots.push(Visit::root(element, Some(bounds.clone()), false));
            }
            Some(Drawn::Through(hits)) => unvisited.extend(hits),
            None => scan.stop(Stop::Unread),
        }
    }
}

/// Every element of app `pid` under `roots`, depth first, within `limits`,
/// and the object each is. Each element reached is struck from `unvisited`.
fn walk(
    scan: &mut Scan,
    roots: Vec<Visit>,
    limits: Limits,
    unvisited: &mut Vec<CFRetained<AXUIElement>>,
    listed: &[windows::Window],
    pid: i64,
) -> (Vec<Element>, Vec<Traced>) {
    let open_menus = menu_frames(&roots);
    let mut found = Vec::new();
    let mut objects = Vec::new();
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
        scan.unproven.clear();
        scan.refused.clear();

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
        let index = found.len();
        found.push(element);
        objects.push(Traced {
            object: visit.element.clone(),
            parent: visit.parent,
            unproven: std::mem::take(&mut scan.unproven),
            refused: std::mem::take(&mut scan.refused),
        });

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
                parent: Some(index),
            });
        }
    }

    (found, objects)
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
    open_menus: &[Bounds],
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
        if frame.is_none() && scan.read_failed() {
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
    let value = if scan.read_failed() {
        scan.unreadable.push("AXValue".to_owned());
        None
    } else {
        scan.value(item, values, role.as_deref(), subrole.as_deref())
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
    frame: Option<&Bounds>,
    inherited: Option<Bounds>,
    listed: &[windows::Window],
    pid: i64,
) -> Option<Bounds> {
    match (role, frame) {
        (Some("AXSheet"), Some(frame)) => {
            match windows::described(listed, pid, frame, None, None) {
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
        Err(error.filter(|error| !absent(*error)).or(role.err()))
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
#[path = "tree_tests.rs"]
mod tests;

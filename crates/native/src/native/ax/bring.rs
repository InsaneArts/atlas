//! Resolve one target and request focus under a single bounded input turn.
//! AX references and transient window titles stay on the calling worker.
//! Resolution, each write and final observation share one reader deadline;
//! retained elements identify the destination but never replace fresh admission.
use super::{
    Status, Unread, Within, absent, ffi,
    keyboard::{Ask, Live, PARENTS, REQUEST, Reader, UnknownNames, remaining},
};
use crate::native::focus::{CHECK_BUDGET, FIELDS, Need, Reason, refused};
use atlas_core::{
    actions::Expect,
    error::{Code, Error, Result},
    name::same_text,
    process::{Info, Instance},
    screen::{
        elements::Element,
        geometry::Bounds,
        select::{ElementFilter, NameMatch},
        windows::{self, Listed, Window},
    },
};
use objc2_application_services::AXError;
use objc2_core_foundation::{CFArray, CFString, CFType, Type};
use std::time::{Duration, Instant};

const WRITE_BUDGET: Duration = Duration::from_secs(1);
const SETTLE_BUDGET: Duration = Duration::from_millis(50);
const WINDOWS: usize = 64;
const ACTIONS: usize = 64;
const ELEMENTS: usize = 1500;
const DEPTH: usize = 14;

pub(super) trait Backend: Ask {
    fn pause(&mut self, duration: Duration) {
        std::thread::sleep(duration);
    }
    fn app(&mut self, pid: i64) -> Result<Self::Element>;
    fn windows(&mut self) -> Result<Vec<Window>>;
    /// A complete, typed list, or an error. Never a truncated prefix.
    fn list(
        &mut self,
        element: &Self::Element,
        name: &'static str,
        limit: usize,
        deadline: Instant,
    ) -> Result<Vec<Self::Element>>;
    fn supports(
        &mut self,
        element: &Self::Element,
        operation: Write,
        deadline: Instant,
    ) -> Result<bool>;
    fn write(&mut self, element: &Self::Element, operation: Write) -> AXError;
    /// The element of `app` Accessibility finds at the point; None when it
    /// finds none.
    fn hit(&mut self, app: &Self::Element, x: f64, y: f64) -> Result<Option<Self::Element>>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Write {
    Flag(&'static str),
    Raise,
}

impl Backend for Live {
    fn app(&mut self, pid: i64) -> Result<Self::Element> {
        let instance = application(pid)?;
        ffi::application(instance.pid).ok_or_else(|| refused(Reason::App))
    }

    fn windows(&mut self) -> Result<Vec<Window>> {
        crate::native::windows::windows()
    }

    fn list(
        &mut self,
        element: &Self::Element,
        name: &'static str,
        limit: usize,
        deadline: Instant,
    ) -> Result<Vec<Self::Element>> {
        let attribute = name;
        let name = CFString::from_static_str(attribute);
        let bound = || {
            Reader {
                ask: Live,
                deadline,
            }
            .bound(element, REQUEST)
        };
        bound()?;
        let count = match ffi::count(element, &name) {
            Ok(count) if count >= 0 && count as usize <= limit => count,
            Err(error) if absent(error) => return Ok(Vec::new()),
            Err(error) => return Err(Unread::Malformed.asking(attribute, error)),
            _ => return Err(Unread::Malformed.error()),
        };
        bound()?;
        if count == 0 {
            return Ok(Vec::new());
        }
        let values = ffi::copy_values(element, &name, count)
            .map_err(|error| Unread::Failed.asking(attribute, error))?
            .ok_or_else(|| Unread::Malformed.error())?;
        bound()?;
        if values.len() != count as usize
            || ffi::count(element, &name)
                .map_err(|error| Unread::Failed.asking(attribute, error))?
                != count
        {
            return Err(Unread::Failed.error());
        }
        let result = values
            .iter()
            .map(|value| {
                value
                    .downcast_ref::<objc2_application_services::AXUIElement>()
                    .map(Type::retain)
                    .ok_or_else(|| Unread::Malformed.error())
            })
            .collect();
        bound()?;
        result
    }

    fn supports(
        &mut self,
        element: &Self::Element,
        operation: Write,
        deadline: Instant,
    ) -> Result<bool> {
        match operation {
            Write::Flag(name) => match ffi::settable(element, &CFString::from_static_str(name)) {
                Ok(value) => Ok(value),
                Err(error) if absent(error) => Ok(false),
                Err(error) => Err(Unread::Failed.reading(error)),
            },
            Write::Raise => {
                let names = match ffi::actions(element) {
                    Ok(Some(names)) => names,
                    Ok(None) => return Ok(false),
                    Err(AXError::ActionUnsupported | AXError::NotImplemented) => return Ok(false),
                    Err(error) => return Err(Unread::Failed.reading(error)),
                };
                supports_raise(&names, deadline, Instant::now)
            }
        }
    }

    fn write(&mut self, element: &Self::Element, operation: Write) -> AXError {
        match operation {
            Write::Flag(name) => ffi::set_flag(element, &CFString::from_static_str(name), true),
            Write::Raise => ffi::act(element, &CFString::from_static_str("AXRaise")),
        }
    }

    fn hit(&mut self, app: &Self::Element, x: f64, y: f64) -> Result<Option<Self::Element>> {
        match ffi::element_at(app, x as f32, y as f32) {
            Ok(element) => Ok(element),
            Err(AXError::NoValue) => Ok(None),
            Err(status) => Err(Unread::Failed.asking("AXUIElementCopyElementAtPosition", status)),
        }
    }
}

/// The listed window of app `pid` that a drag pressing at `point` moves by
/// its own title, as Accessibility proves it before `deadline`; None when
/// anything else is there. Nothing is written.
pub(in crate::native) fn title_bar(
    pid: i64,
    point: (f64, f64),
    listed: &[Window],
    deadline: Instant,
) -> Result<Option<i64>> {
    let mut reader = Reader {
        ask: Live,
        deadline,
    };
    let app = reader.ask.app(pid)?;
    reader.title_bar(&app, pid, point, listed)
}

/// The band across the top of a window `frame` where its title bar is,
/// worked out from its `title` element: from the window's top edge down past
/// the title by as much again as the title sits below that edge. None when
/// the title is not inside the window.
fn title_band(frame: &Bounds, title: &Bounds) -> Option<Bounds> {
    let above = title.y - frame.y;
    let height = above * 2.0 + title.height;
    (above >= 0.0 && title.height > 0.0 && frame.covers(title)).then(|| Bounds {
        height: height.min(frame.height),
        ..frame.clone()
    })
}

fn supports_raise(
    names: &CFArray<CFType>,
    deadline: Instant,
    mut now: impl FnMut() -> Instant,
) -> Result<bool> {
    if names.len() > ACTIONS {
        return Err(Unread::Malformed.error());
    }
    let mut check = || remaining(deadline, now()).map(|_| ());
    let raise = CFString::from_static_str("AXRaise");
    let mut supported = false;
    for name in names.iter() {
        check()?;
        let name = name
            .downcast_ref::<CFString>()
            .ok_or_else(|| Unread::Malformed.error())?;
        supported |= name == &*raise;
    }
    check()?;
    Ok(supported)
}

/// Kernel process facts do not depend on an AppKit run loop delivering
/// application-launch notifications to this worker process.
fn application_with(
    pid: i64,
    uid: u32,
    describe: impl FnOnce(libc::pid_t) -> Result<Option<Info>>,
) -> Result<Option<Instance>> {
    let Some(pid) = libc::pid_t::try_from(pid).ok().filter(|pid| *pid > 0) else {
        return Ok(None);
    };
    Ok(describe(pid)?
        .filter(|info| info.instance.pid == pid && info.uid == uid && !info.zombie)
        .map(|info| info.instance))
}

/// A process that cannot be read is refused as gone, keeping why.
pub(in crate::native) fn application(pid: i64) -> Result<Instance> {
    application_with(
        pid,
        nix::unistd::geteuid().as_raw(),
        atlas_core::process::info,
    )
    .map_err(|cause| refused(Reason::App).with_source(cause))?
    .ok_or_else(|| refused(Reason::App))
}

pub(in crate::native) fn check_application(target: Option<Instance>) -> Result<()> {
    if let Some(target) = target {
        match application(target.pid.into()) {
            Ok(current) => same_application(target, Some(current))?,
            Err(cause) => {
                same_application(target, None).map_err(|refusal| refusal.with_source(cause))?;
            }
        }
    }
    Ok(())
}

pub(in crate::native) fn target_application(expect: &Expect) -> Result<Instance> {
    let target = match expect.instance() {
        Some(instance) => instance,
        None => application(expect.pid())?,
    };
    check_application(Some(target))?;
    Ok(target)
}

fn same_application(target: Instance, current: Option<Instance>) -> Result<()> {
    if current != Some(target) {
        return Err(refused(Reason::App));
    }
    Ok(())
}

/// The frame a window must have to be told from the window list.
fn frame(window: &Element) -> Result<&Bounds> {
    window
        .frame
        .as_ref()
        .ok_or_else(|| Unread::Malformed.error())
}

/// Which listed window of app `pid` an element with this `frame` is: the one
/// its frame, title and kind (by its subrole) leave.
pub(in crate::native) fn described(
    window: &Element,
    frame: &Bounds,
    pid: i64,
    listed: &[Window],
) -> Listed {
    windows::described(
        listed,
        pid,
        frame,
        window.title.as_deref(),
        window.subrole.as_deref(),
    )
}

/// Window `number` of app `pid` in `listed`.
fn named_window(listed: &[Window], pid: i64, number: i64) -> Result<&Window> {
    listed
        .iter()
        .find(|window| window.pid == pid && window.number == number)
        .ok_or_else(|| refused(Reason::Window))
}

/// The window that must hold a named field.
fn field_window<E>(window: Option<&E>) -> Result<&E> {
    window.ok_or_else(|| refused(Reason::Field))
}

fn named_field(name: &str) -> ElementFilter {
    ElementFilter {
        name: Some(name.to_owned()),
        ..Default::default()
    }
}

/// The one field `matches` leaves, unless it is disabled.
fn chosen<E>(matches: Vec<(E, Option<bool>)>) -> Result<E> {
    let mut matches = matches.into_iter();
    match (matches.next(), matches.next()) {
        (Some((_, Some(false))), None) => Err(refused(Reason::Disabled)),
        (Some((element, _)), None) => Ok(element),
        (None, _) => Err(refused(Reason::Field)),
        (Some(_), Some(_)) => Err(refused(Reason::Field)),
    }
}

struct WindowRef<E> {
    element: E,
    parent: Option<E>,
}

/// The parent `refs` records for `element`.
fn parent_of<'a, E: PartialEq>(refs: &'a [WindowRef<E>], element: &E) -> Option<&'a E> {
    refs.iter()
        .find(|entry| entry.element == *element)
        .and_then(|entry| entry.parent.as_ref())
}

/// What one admission pass judges each window against: the app's windows,
/// and the window selected, when the selection is explicit.
struct Scope<'a, E> {
    refs: Vec<WindowRef<E>>,
    window: Option<&'a E>,
    pid: i64,
    explicit: bool,
}

impl<A: Backend> Reader<A> {
    /// AppKit may expose a sheet only as an AXSheet child of its document.
    /// Both attributes are direct relationship evidence; geometry is not.
    fn sheets(&mut self, window: &A::Element, pid: i64) -> Result<Vec<A::Element>> {
        let mut sheets = self.list(window, "AXSheets", WINDOWS)?;
        for child in self.list(window, "AXChildren", ELEMENTS)? {
            self.owned(&child, pid)?;
            if self.text(&child, "AXRole")?.as_deref() == Some("AXSheet")
                && !sheets.contains(&child)
            {
                if sheets.len() >= WINDOWS {
                    return Err(Unread::Deep.error());
                }
                sheets.push(child);
            }
        }
        Ok(sheets)
    }

    /// Include attached sheets even when the app omits them from AXWindows.
    /// Keep their parent relationship, and reject conflicting relationships.
    fn window_refs(&mut self, app: &A::Element, pid: i64) -> Result<Vec<WindowRef<A::Element>>> {
        let mut refs = Vec::new();
        for element in self.list(app, "AXWindows", WINDOWS)? {
            self.owned(&element, pid)?;
            if self.text(&element, "AXRole")?.as_deref() == Some("AXHelpTag") {
                continue;
            }
            refs.push(WindowRef {
                element,
                parent: None,
            });
        }
        let mut index = 0;
        while index < refs.len() {
            let element = refs[index].element.clone();
            self.owned(&element, pid)?;
            for sheet in self.sheets(&element, pid)? {
                if sheet == element {
                    return Err(Unread::Malformed.error());
                }
                if let Some(existing) = refs.iter_mut().find(|entry| entry.element == sheet) {
                    if existing
                        .parent
                        .as_ref()
                        .is_some_and(|parent| *parent != element)
                    {
                        return Err(Unread::Alike.error());
                    }
                    existing.parent = Some(element.clone());
                } else {
                    if refs.len() >= WINDOWS {
                        return Err(Unread::Deep.error());
                    }
                    refs.push(WindowRef {
                        element: sheet,
                        parent: Some(element.clone()),
                    });
                }
            }
            index += 1;
        }
        for entry in &refs {
            let mut parent = entry.parent.as_ref();
            let mut seen = vec![&entry.element];
            while let Some(element) = parent {
                if seen.contains(&element) {
                    return Err(Unread::Malformed.error());
                }
                seen.push(element);
                parent = parent_of(&refs, element);
            }
        }
        Ok(refs)
    }

    fn list(
        &mut self,
        element: &A::Element,
        name: &'static str,
        limit: usize,
    ) -> Result<Vec<A::Element>> {
        self.left()?;
        let result = self.ask.list(element, name, limit, self.deadline)?;
        self.left()?;
        if result.len() > limit {
            return Err(Unread::Malformed.error());
        }
        Ok(result)
    }

    fn listed(&mut self) -> Result<Vec<Window>> {
        let listed = self.ask.windows()?;
        self.left()?;
        Ok(listed)
    }

    fn point_window(&mut self, pid: i64, x: f64, y: f64) -> Result<Option<i64>> {
        let listed = self.listed()?;
        if listed
            .iter()
            .any(|window| window.is_menu_bar() && window.bounds.contains(x, y))
        {
            return Ok(None);
        }
        let candidate = listed
            .iter()
            .find(|window| {
                window.pid == pid
                    && window.alpha > 0.0
                    && window.is_apps_or_menu()
                    && window.bounds.contains(x, y)
            })
            .ok_or_else(|| refused(Reason::Point))?;
        Ok((candidate.layer != windows::MENU_LAYER).then_some(candidate.number))
    }

    /// The listed window of app `pid` that a press at the point moves by its
    /// title bar: Accessibility finds there either the element its standard
    /// window names as its title, or that window itself with nothing deeper,
    /// and the point lies in the window's title band (see [`title_band`]).
    /// `listed` must show the window once. A window without a title
    /// element proves nothing; None when anything else is there.
    fn title_bar(
        &mut self,
        app: &A::Element,
        pid: i64,
        (x, y): (f64, f64),
        listed: &[Window],
    ) -> Result<Option<i64>> {
        self.owned(app, pid)?;
        self.bound(app, REQUEST)?;
        let hit = self.ask.hit(app, x, y)?;
        self.left()?;
        let Some(hit) = hit.filter(|hit| self.ask.owner(hit) == Some(pid)) else {
            return Ok(None);
        };
        let window = if self.text(&hit, "AXRole")?.as_deref() == Some("AXWindow") {
            hit.clone()
        } else {
            match self.reference(&hit, "AXWindow")? {
                Some(window) => window,
                None => return Ok(None),
            }
        };
        self.owned(&window, pid)?;
        let Some(title) = self.reference(&window, "AXTitleUIElement")? else {
            return Ok(None);
        };
        self.owned(&title, pid)?;
        if hit != title && hit != window {
            return Ok(None);
        }
        let shown = self.window(&window)?;
        if shown.role.as_deref() != Some("AXWindow")
            || shown.subrole.as_deref() != Some("AXStandardWindow")
        {
            return Ok(None);
        }
        let frame = frame(&shown)?;
        let Some(heading) = self.frame(&title)? else {
            return Ok(None);
        };
        let pressed = |area: &Bounds| area.contains(x, y);
        if !title_band(frame, &heading).is_some_and(|band| pressed(&band))
            || (hit == title && !pressed(&heading))
        {
            return Ok(None);
        }
        Ok(match described(&shown, frame, pid, listed) {
            Listed::One(number) => Some(number),
            Listed::Missing | Listed::Several => None,
        })
    }

    fn resolve(&mut self, app: &A::Element, pid: i64, number: i64) -> Result<A::Element> {
        let listed = self.listed()?;
        let requested = named_window(&listed, pid, number)?;
        let mut found = None;
        for WindowRef {
            element: candidate, ..
        } in self.window_refs(app, pid)?
        {
            self.owned(&candidate, pid)?;
            let window = self.window(&candidate)?;
            if !matches!(window.role.as_deref(), Some("AXWindow" | "AXSheet")) {
                return Err(Unread::Malformed.error());
            }
            let frame = frame(&window)?;
            if described(&window, frame, pid, std::slice::from_ref(requested)) == Listed::Missing {
                continue;
            }
            match described(&window, frame, pid, &listed) {
                Listed::One(n) if n == number => {
                    if found.is_some() {
                        return Err(Unread::Alike.error());
                    }
                    found = Some(candidate);
                }
                Listed::Several => return Err(Unread::Alike.error()),
                Listed::One(_) | Listed::Missing => {}
            }
        }
        let found = found.ok_or_else(|| Unread::Unlisted.error())?;
        self.identity(&found, pid, number)?;
        Ok(found)
    }

    fn identity(&mut self, element: &A::Element, pid: i64, number: i64) -> Result<()> {
        self.owned(element, pid)?;
        let window = self.window(element)?;
        let frame = frame(&window)?;
        let listed = self.listed()?;
        match described(&window, frame, pid, &listed) {
            Listed::One(current) if current == number => Ok(()),
            Listed::Several => Err(Unread::Alike.error()),
            Listed::One(_) | Listed::Missing => Err(Unread::Unlisted.error()),
        }
    }

    fn admit_window(
        &mut self,
        candidate: &A::Element,
        parent: Option<&A::Element>,
        scope: &Scope<'_, A::Element>,
    ) -> Result<()> {
        let (window, pid, explicit) = (scope.window, scope.pid, scope.explicit);
        self.owned(candidate, pid)?;
        let role = self.text(candidate, "AXRole")?;
        let subrole = self.text(candidate, "AXSubrole")?;
        let modal = self.flag(candidate, "AXModal")?;
        if role.as_deref() == Some("AXApplication") {
            return Err(Unread::Withheld.error());
        }
        if explicit && window == Some(candidate) {
            return Ok(());
        }
        if parent.is_some() || role.as_deref() == Some("AXSheet") {
            // Attached sheets have direct parent evidence. A separately listed
            // sheet must identify its parent through a bounded ancestry read.
            let parent = match parent {
                Some(parent) => parent.clone(),
                None => self.sheet_parent(candidate, pid)?,
            };
            self.owned(&parent, pid)?;
            if window.is_none() || window == Some(&parent) || window == Some(candidate) {
                return Err(refused(Reason::Sheet));
            }
            return Ok(());
        }
        if modal == Some(true) || subrole.as_deref() == Some("AXSystemDialog") {
            return Err(refused(Reason::Sheet));
        }
        if subrole.as_deref() == Some("AXDialog") && modal.is_none() {
            return Err(Unread::Failed.error());
        }
        Ok(())
    }

    /// Walk up from the focused element to the window that holds it.
    fn admit_focus_path(
        &mut self,
        mut element: A::Element,
        focused: Option<&A::Element>,
        scope: &Scope<'_, A::Element>,
    ) -> Result<()> {
        for _ in 0..PARENTS {
            self.owned(&element, scope.pid)?;
            if Some(&element) == scope.window {
                return Ok(());
            }
            let role = self.text(&element, "AXRole")?;
            if role.as_deref() == Some("AXApplication") {
                return Ok(());
            }
            let parent = parent_of(&scope.refs, &element);
            self.admit_window(&element, parent, scope)?;
            if parent.is_some() || matches!(role.as_deref(), Some("AXWindow" | "AXSheet")) {
                if role.as_deref() != Some("AXSheet")
                    && parent.is_none()
                    && scope.window == focused
                    && scope.window.is_some()
                {
                    return Err(Unread::Failed.error());
                }
                return Ok(());
            }
            element = self
                .reference(&element, "AXParent")?
                .ok_or_else(|| Unread::Failed.error())?;
        }
        Err(Unread::Deep.error())
    }

    fn sheet_parent(&mut self, sheet: &A::Element, pid: i64) -> Result<A::Element> {
        let mut element = sheet.clone();
        for _ in 0..PARENTS {
            element = self
                .reference(&element, "AXParent")?
                .ok_or_else(|| Unread::Failed.error())?;
            self.owned(&element, pid)?;
            match self.text(&element, "AXRole")?.as_deref() {
                Some("AXWindow" | "AXSheet") => return Ok(element),
                Some("AXApplication") => return Err(Unread::Failed.error()),
                _ => {}
            }
        }
        Err(Unread::Deep.error())
    }

    fn at_sheet(&mut self, sheet: &A::Element, (x, y): (f64, f64)) -> Result<()> {
        let frame = self
            .window(sheet)?
            .frame
            .ok_or_else(|| Unread::Malformed.error())?;
        if frame.contains(x, y) {
            Ok(())
        } else {
            Err(refused(Reason::Sheet))
        }
    }

    fn find_field(&mut self, window: &A::Element, pid: i64, name: &str) -> Result<A::Element> {
        let filter = named_field(name);
        let mut stack = vec![(window.clone(), 0)];
        let mut seen = Vec::new();
        let mut best = None;
        let mut matches = Vec::new();
        let mut identifiers = Vec::new();
        let mut unknown = UnknownNames::default();
        while let Some((element, depth)) = stack.pop() {
            if seen.len() >= ELEMENTS || depth > DEPTH || seen.contains(&element) {
                return Err(Unread::Deep.error());
            }
            seen.push(element.clone());
            self.owned(&element, pid)?;
            let role = self.text(&element, "AXRole")?;
            if element != *window && matches!(role.as_deref(), Some("AXWindow" | "AXSheet")) {
                continue;
            }
            if role.as_deref().is_some_and(|role| FIELDS.contains(&role)) {
                let (mut described, failed) = self.field_names(&element)?;
                unknown.labels |= failed.labels;
                unknown.identifier |= failed.identifier;
                // Identifiers name their own namespace and only match whole.
                // Labels cannot create or conceal an identifier duplicate.
                if described
                    .identifier
                    .take()
                    .is_some_and(|identifier| same_text(&identifier, name))
                {
                    identifiers.push((element.clone(), described.enabled));
                }
                if let Some(rank) = filter.match_of(&described)
                    && best.is_none_or(|best| rank >= best)
                {
                    if best != Some(rank) {
                        matches.clear();
                        best = Some(rank);
                    }
                    matches.push((element.clone(), described.enabled));
                }
            }
            for child in self.list(
                &element,
                "AXChildren",
                ELEMENTS.saturating_sub(seen.len() + stack.len()),
            )? {
                stack.push((child, depth + 1));
            }
        }
        if !identifiers.is_empty() {
            if unknown.identifier {
                return Err(Unread::Failed.error());
            }
            matches = identifiers;
        } else if (unknown.labels || unknown.identifier) && best.is_some() {
            return Err(refused(Reason::Field));
        } else if matches.is_empty() && unknown.identifier {
            return Err(Unread::Failed.error());
        }
        chosen(matches)
    }
}

struct Target<A: Backend> {
    reader: Reader<A>,
    app: A::Element,
    window: Option<A::Element>,
    field: Option<A::Element>,
    number: Option<i64>,
    /// Whether the window is raised: it was named or lies under the point,
    /// not only inferred as proof for a named field.
    raise: bool,
    changed: bool,
    point: Option<(f64, f64)>,
}

impl<A: Backend> Target<A> {
    /// Keep an inferred field window as proof, without adding a raise that
    /// an app-only request did not previously make.
    fn resolve(ask: A, expect: &Expect, need: Need, deadline: Instant) -> Result<Self> {
        let pid = expect.pid();
        let mut reader = Reader { ask, deadline };
        let app = reader.ask.app(pid)?;
        reader.left()?;
        reader.owned(&app, pid)?;
        if expect.field().is_some() {
            // Ignore the result because this read only wakes the browser's page tree.
            let _ = reader.text(&app, "AXRole");
        }
        let point = match need {
            Need::Point(x, y) | Need::Pointer(x, y) => Some((x, y)),
            Need::Drag { from, .. } => Some(from),
            Need::Keys | Need::Nothing => None,
        };
        let mut number = match (expect.window(), point) {
            (Some(number), _) => Some(number),
            (None, Some((x, y))) => reader.point_window(pid, x, y)?,
            (None, None) => None,
        };
        let raise = number.is_some();
        let window = match number {
            Some(number) => Some(reader.resolve(&app, pid, number)?),
            None => match reader.reference(&app, "AXFocusedWindow")? {
                Some(window) => Some(window),
                None => reader.reference(&app, "AXMainWindow")?,
            },
        };
        if number.is_none() && expect.field().is_some() {
            let window = field_window(window.as_ref())?;
            reader.owned(window, pid)?;
            let shown = reader.window(window)?;
            let listed = reader.ask.windows()?;
            number = Some(match described(&shown, frame(&shown)?, pid, &listed) {
                Listed::One(number) => number,
                Listed::Missing => return Err(Unread::Unlisted.error()),
                Listed::Several => return Err(Unread::Alike.error()),
            });
        }
        Ok(Self {
            reader,
            app,
            window,
            number,
            raise,
            point,
            field: None,
            changed: false,
        })
    }

    fn admit(&mut self, expect: &Expect) -> Result<()> {
        // Only an explicit selection grants the unrelated-document exemption.
        // An app-only command follows the current keyboard destination.
        let reader = &mut self.reader;
        let app = &self.app;
        let pid = expect.pid();
        let point = self.point;
        let explicit = expect.window().is_some() || point.is_some();
        let window = self.window.as_ref().filter(|_| explicit);
        let scope = Scope {
            refs: reader.window_refs(app, pid)?,
            window,
            pid,
            explicit,
        };
        let refs = &scope.refs;
        if let Some(window) = window {
            let attached = refs
                .iter()
                .any(|entry| entry.element == *window && entry.parent.is_some());
            if attached || reader.text(window, "AXRole")?.as_deref() == Some("AXSheet") {
                if !refs.iter().any(|entry| entry.element == *window) {
                    return Err(Unread::Unlisted.error());
                }
                if let Some(point) = point {
                    reader.at_sheet(window, point)?;
                }
            }
        }
        for entry in refs {
            reader.admit_window(&entry.element, entry.parent.as_ref(), &scope)?;
        }
        if let Some(window) = window
            && !refs.iter().any(|entry| entry.element == *window)
        {
            reader.admit_window(window, None, &scope)?;
            if !reader.sheets(window, pid)?.is_empty() {
                return Err(refused(Reason::Sheet));
            }
        }
        let focused = reader.reference(app, "AXFocusedWindow")?;
        let main = reader.reference(app, "AXMainWindow")?;
        for current in [focused.as_ref(), main.as_ref()].into_iter().flatten() {
            reader.admit_window(current, parent_of(refs, current), &scope)?;
            for sheet in reader.sheets(current, pid)? {
                reader.admit_window(&sheet, Some(current), &scope)?;
            }
        }
        if let Some(element) = reader.reference(app, "AXFocusedUIElement")? {
            reader.admit_focus_path(element, focused.as_ref(), &scope)?;
        }
        Ok(())
    }

    /// A window both named and pointed at must lie under the point.
    fn check_point(&mut self, expect: &Expect) -> Result<()> {
        if let (Some(number), Some((x, y))) = (expect.window(), self.point) {
            let listed = self.reader.listed()?;
            let window = named_window(&listed, expect.pid(), number)?;
            if !window.bounds.contains(x, y) {
                return Err(refused(Reason::Window));
            }
        }
        Ok(())
    }

    /// Whether the window and field already have focus, so nothing need be
    /// written; a named field found whole is kept as the target's.
    fn in_place(&mut self, expect: &Expect) -> Result<bool> {
        if self.point.is_some() && self.raise {
            let focused = self.reader.reference(&self.app, "AXFocusedWindow")?;
            if focused.as_ref() != self.window.as_ref() {
                return Ok(false);
            }
        }
        let Some(name) = expect.field() else {
            return Ok(true);
        };
        if let Some(focused) = self.reader.reference(&self.app, "AXFocusedUIElement")? {
            let field = self.reader.field(&focused)?;
            if named_field(name).match_of(&field) == Some(NameMatch::Exact) {
                self.field = Some(focused);
            }
        }
        Ok(self.field.is_some())
    }

    /// The focus requests that bring the destination forward, in order.
    fn writes(&self) -> Vec<(A::Element, Write)> {
        let mut writes = vec![(self.app.clone(), Write::Flag("AXFrontmost"))];
        if let Some(window) = self.window.as_ref().filter(|_| self.raise) {
            writes.extend([
                (window.clone(), Write::Raise),
                (window.clone(), Write::Flag("AXMain")),
                (window.clone(), Write::Flag("AXFocused")),
            ]);
        }
        if let Some(field) = &self.field {
            writes.push((field.clone(), Write::Flag("AXFocused")));
        }
        writes
    }

    /// Recheck admission and identity for each supported focus write.
    fn focus(&mut self, expect: &Expect, guard: &impl Fn() -> Result<()>) -> Result<()> {
        for (element, operation) in self.writes() {
            self.admit(expect)?;
            guard()?;
            self.reader.bound(&element, REQUEST)?;
            let supported = self
                .reader
                .ask
                .supports(&element, operation, self.reader.deadline)?;
            self.reader.left()?;
            if !supported {
                continue;
            }
            self.admit(expect)?;
            if self.field.as_ref() == Some(&element) {
                if self.reader.field(&element)?.enabled == Some(false) {
                    return Err(refused(Reason::Disabled));
                }
                if self.reader.within(element.clone(), self.window.as_ref())? != Within::Window {
                    return Err(Unread::Failed.error());
                }
            }
            if let (Some(window), Some(number)) = (&self.window, self.number) {
                self.reader.identity(window, expect.pid(), number)?;
            }
            guard()?;
            self.reader.bound(&element, WRITE_BUDGET)?;
            let status = self.reader.ask.write(&element, operation);
            if status == AXError::CannotComplete {
                return Err(Error::new(
                    Code::Uncertain,
                    "The app did not acknowledge the focus request",
                )
                .uncertain()
                .with_source(Status(status)));
            }
            self.changed |= status == AXError::Success;
            guard()?;
            if status != AXError::Success
                && !(self.point.is_some()
                    && self.window.as_ref() == Some(&element)
                    && operation == Write::Flag("AXFocused"))
                && !matches!(
                    status,
                    AXError::AttributeUnsupported
                        | AXError::ActionUnsupported
                        | AXError::NotImplemented
                )
            {
                if self.field.as_ref() == Some(&element) {
                    return Err(refused(Reason::Field).with_source(Status(status)));
                }
                return Err(Unread::Failed.reading(status));
            }
            self.reader.left()?;
        }
        guard()?;
        Ok(())
    }

    fn verify(
        &mut self,
        deadline: Instant,
        verify: &mut impl FnMut(Option<i64>, Instant) -> Result<Option<&'static str>>,
    ) -> Result<Option<&'static str>> {
        let role = verify(self.number, deadline)?;
        if let Some(field) = &self.field {
            let focused = self.reader.reference(&self.app, "AXFocusedUIElement")?;
            // The retained AX elements' safe equality uses CFEqual. A child
            // editor is deliberately refused: ancestry alone is not identity.
            if focused.as_ref() != Some(field) {
                return Err(refused(Reason::Field));
            }
        }
        Ok(role)
    }

    /// Admission and proof, each checked against the other: admission is
    /// another observation whose scan may outlive the earlier proof.
    fn proven(
        &mut self,
        expect: &Expect,
        deadline: Instant,
        verify: &mut impl FnMut(Option<i64>, Instant) -> Result<Option<&'static str>>,
    ) -> Result<Option<&'static str>> {
        self.admit(expect)?;
        self.verify(deadline, verify)?;
        self.admit(expect)?;
        self.verify(deadline, verify)
    }
}

/// Resolve and admit the exact destination before any focus write.
fn prepare<A: Backend>(
    ask: A,
    expect: &Expect,
    need: Need,
    deadline: Instant,
    guard: &impl Fn() -> Result<()>,
    ready: &mut impl FnMut(Option<i64>) -> bool,
) -> Result<Target<A>> {
    guard()?;
    let mut target = Target::resolve(ask, expect, need, deadline)?;
    target.admit(expect)?;
    guard()?;
    target.check_point(expect)?;
    if ready(target.number) && target.in_place(expect)? {
        return Ok(target);
    }
    target.reader.left()?;
    if let Some(name) = expect.field() {
        let window = field_window(target.window.as_ref())?;
        target.field = Some(target.reader.find_field(window, expect.pid(), name)?);
    }
    target.focus(expect, guard)?;
    Ok(target)
}

pub(in crate::native) fn bring(
    expect: &Expect,
    need: Need,
    deadline: Instant,
    guard: &impl Fn() -> Result<()>,
    mut verify: impl FnMut(Option<i64>, Instant) -> Result<Option<&'static str>>,
) -> Result<Option<&'static str>> {
    let preparation = deadline
        .checked_sub(SETTLE_BUDGET)
        .ok_or_else(|| Unread::Slow.error())?;
    let target = prepare(Live, expect, need, preparation, guard, &mut |number| {
        verify(number, preparation).is_ok()
    })?;
    settle(target, expect, deadline, guard, verify)
}

fn settle<A: Backend>(
    mut target: Target<A>,
    expect: &Expect,
    deadline: Instant,
    guard: &impl Fn() -> Result<()>,
    mut verify: impl FnMut(Option<i64>, Instant) -> Result<Option<&'static str>>,
) -> Result<Option<&'static str>> {
    let mut last_refusal = None;
    loop {
        guard()?;
        let checking = deadline.min(target.reader.ask.now() + CHECK_BUDGET);
        target.reader.deadline = checking;
        let result = target.proven(expect, checking, &mut verify);
        guard()?;
        let result = result.and_then(|role| target.reader.left().map(|_| role));
        target.reader.deadline = deadline;
        let error = match result {
            Ok(role) => return Ok(role),
            Err(error) if slow(&error) => {
                if !target.changed || target.reader.left().is_err() {
                    return Err(last_refusal.unwrap_or(error));
                }
                last_refusal.take().unwrap_or(error)
            }
            Err(error) => error,
        };
        if !target.changed || !settling(&error) || target.reader.left().is_err() {
            return Err(error);
        }
        // A refused destination never authorizes input. Keep its actual reason
        // if waiting for a focus request to take effect exhausts the deadline.
        target.reader.ask.pause(
            Duration::from_millis(10)
                .min(deadline.saturating_duration_since(target.reader.ask.now())),
        );
        if target.reader.ask.now() >= deadline || Instant::now() >= deadline {
            guard()?;
            return Err(error);
        }
        last_refusal = Some(error);
    }
}

fn slow(error: &Error) -> bool {
    Unread::of(error) == Some(Unread::Slow)
}

/// Whether a refusal may only mean a focus request has yet to take effect.
fn settling(error: &Error) -> bool {
    slow(error)
        || (error.code == Code::FocusChanged && Reason::of(error).is_some_and(Reason::may_settle))
}

#[cfg(test)]
mod tests;

//! Resolve one target and request focus under a single bounded input turn.
//! AX references and transient window titles stay on the calling worker.
use super::{
    ABSENT, Unread, Within, ffi,
    keyboard::{Ask, Live, Reader},
};
use crate::{
    actions::Expect,
    error::{Code, Error, Result},
    native::{
        focus::{CHECK_BUDGET, FIELDS, Need},
        windows::{self, Kind, Listed, Window},
    },
    process::{Info, Instance},
    select::{ElementFilter, NameMatch},
    ui::text::same_text,
};
use objc2_application_services::AXError;
use objc2_core_foundation::{CFArray, CFString, CFType, Type};
use std::time::{Duration, Instant};

const REQUEST: Duration = Duration::from_millis(250);
const WRITE_BUDGET: Duration = Duration::from_secs(1);
const SETTLE_BUDGET: Duration = Duration::from_millis(50);
const WINDOWS: usize = 64;
const ACTIONS: usize = 64;
const ELEMENTS: usize = 1500;
const DEPTH: usize = 14;

pub(super) trait Backend: Ask {
    fn app(&mut self, pid: i64) -> Option<Self::Element>;
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
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Write {
    Flag(&'static str),
    Raise,
}

impl Backend for Live {
    fn app(&mut self, pid: i64) -> Option<Self::Element> {
        application(pid)
            .ok()
            .and_then(|instance| ffi::application(instance.pid))
    }

    fn windows(&mut self) -> Result<Vec<Window>> {
        windows::windows()
    }

    fn list(
        &mut self,
        element: &Self::Element,
        name: &'static str,
        limit: usize,
        deadline: Instant,
    ) -> Result<Vec<Self::Element>> {
        let name = CFString::from_static_str(name);
        let bound = || {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Err(Unread::Slow.error());
            }
            if ffi::set_timeout(element, left.min(REQUEST)) != AXError::Success {
                return Err(Unread::Failed.error());
            }
            Ok(())
        };
        bound()?;
        let count = match ffi::count(element, &name) {
            Ok(count) if count >= 0 && count as usize <= limit => count,
            Err(error) if ABSENT.contains(&error) => return Ok(Vec::new()),
            _ => return Err(Unread::Malformed.error()),
        };
        bound()?;
        if count == 0 {
            return Ok(Vec::new());
        }
        let values = ffi::copy_values(element, &name, count)
            .map_err(|_| Unread::Failed.error())?
            .ok_or_else(|| Unread::Malformed.error())?;
        bound()?;
        if values.len() != count as usize || ffi::count(element, &name) != Ok(count) {
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
                Err(error) if ABSENT.contains(&error) => Ok(false),
                Err(_) => Err(Unread::Failed.error()),
            },
            Write::Raise => {
                let names = match ffi::actions(element) {
                    Ok(Some(names)) => names,
                    Ok(None) => return Ok(false),
                    Err(AXError::ActionUnsupported | AXError::NotImplemented) => return Ok(false),
                    Err(_) => return Err(Unread::Failed.error()),
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
}

fn supports_raise(
    names: &CFArray<CFType>,
    deadline: Instant,
    mut now: impl FnMut() -> Instant,
) -> Result<bool> {
    if names.len() > ACTIONS {
        return Err(Unread::Malformed.error());
    }
    let mut check = || {
        if now() >= deadline {
            return Err(Unread::Slow.error());
        }
        Ok(())
    };
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
    describe: impl FnOnce(libc::pid_t) -> Option<Info>,
) -> Option<Instance> {
    let pid = libc::pid_t::try_from(pid).ok().filter(|pid| *pid > 0)?;
    let info = describe(pid)?;
    (info.instance.pid == pid && info.uid == uid && !info.zombie).then_some(info.instance)
}

pub(crate) fn application(pid: i64) -> Result<Instance> {
    application_with(pid, nix::unistd::geteuid().as_raw(), |pid| {
        crate::process::info(pid).ok().flatten()
    })
    .ok_or_else(|| refused("app", "The target app is no longer running"))
}

pub(crate) fn check_application(target: Option<Instance>) -> Result<()> {
    if let Some(target) = target {
        same_application(target, application(target.pid.into()).ok())?;
    }
    Ok(())
}

pub(crate) fn target_application(expect: &Expect) -> Result<Instance> {
    let target = match expect.instance() {
        Some(instance) => instance,
        None => application(expect.pid())?,
    };
    check_application(Some(target))?;
    Ok(target)
}

fn same_application(target: Instance, current: Option<Instance>) -> Result<()> {
    if current != Some(target) {
        return Err(refused(
            "app",
            "The target app instance is no longer running",
        ));
    }
    Ok(())
}

fn refused(reason: &str, message: &str) -> Error {
    Error::new(
        Code::FocusChanged,
        format!("{message}, so this action and the rest were not sent"),
    )
    .with("reason", reason)
}

struct WindowRef<E> {
    element: E,
    parent: Option<E>,
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
        let mut refs: Vec<_> = self
            .list(app, "AXWindows", WINDOWS)?
            .into_iter()
            .map(|element| WindowRef {
                element,
                parent: None,
            })
            .collect();
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
                parent = refs
                    .iter()
                    .find(|entry| entry.element == *element)
                    .and_then(|entry| entry.parent.as_ref());
            }
        }
        Ok(refs)
    }

    fn bound(&mut self, element: &A::Element, budget: Duration) -> Result<()> {
        let left = self.left()?;
        if !self.ask.bound(element, left.min(budget)) {
            return Err(Unread::Failed.error());
        }
        Ok(())
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

    fn owned(&mut self, element: &A::Element, pid: i64) -> Result<()> {
        if self.ask.owner(element) != Some(pid) {
            return Err(Unread::Failed.error());
        }
        Ok(())
    }

    fn resolve(&mut self, app: &A::Element, pid: i64, number: i64) -> Result<A::Element> {
        let listed = self.ask.windows()?;
        self.left()?;
        let requested = listed
            .iter()
            .find(|window| window.pid == pid && window.number == number)
            .ok_or_else(|| refused("window", "The named window is not on screen"))?;
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
            let frame = window.frame.ok_or_else(|| Unread::Malformed.error())?;
            if windows::described(
                std::slice::from_ref(requested),
                pid,
                &frame,
                window.title.as_deref(),
                Kind::of(window.subrole.as_deref()),
            ) == Listed::Missing
            {
                continue;
            }
            match windows::described(
                &listed,
                pid,
                &frame,
                window.title.as_deref(),
                Kind::of(window.subrole.as_deref()),
            ) {
                Listed::One(n) if n == number => {
                    if found.is_some() {
                        return Err(Unread::Alike.error());
                    }
                    found = Some(candidate);
                }
                Listed::Several => return Err(Unread::Alike.error()),
                _ => {}
            }
        }
        let found = found.ok_or_else(|| Unread::Unlisted.error())?;
        self.identity(&found, pid, number)?;
        Ok(found)
    }

    fn identity(&mut self, element: &A::Element, pid: i64, number: i64) -> Result<()> {
        self.owned(element, pid)?;
        let window = self.window(element)?;
        let frame = window.frame.ok_or_else(|| Unread::Malformed.error())?;
        let listed = self.ask.windows()?;
        self.left()?;
        match windows::described(
            &listed,
            pid,
            &frame,
            window.title.as_deref(),
            Kind::of(window.subrole.as_deref()),
        ) {
            Listed::One(current) if current == number => Ok(()),
            Listed::Several => Err(Unread::Alike.error()),
            _ => Err(Unread::Unlisted.error()),
        }
    }

    fn admit_window(
        &mut self,
        candidate: &A::Element,
        parent: Option<&A::Element>,
        window: Option<&A::Element>,
        pid: i64,
        explicit: bool,
    ) -> Result<()> {
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
                return Err(refused("sheet", "A sheet is open over the target window"));
            }
            return Ok(());
        }
        if modal == Some(true) || subrole.as_deref() == Some("AXSystemDialog") {
            return Err(refused("sheet", "A modal dialog blocks the target window"));
        }
        if subrole.as_deref() == Some("AXDialog") && modal.is_none() {
            return Err(Unread::Failed.error());
        }
        Ok(())
    }

    fn sheet_parent(&mut self, sheet: &A::Element, pid: i64) -> Result<A::Element> {
        let mut element = sheet.clone();
        for _ in 0..256 {
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

    fn blocking(
        &mut self,
        app: &A::Element,
        window: Option<&A::Element>,
        pid: i64,
        explicit: bool,
        point: Option<(f64, f64)>,
    ) -> Result<()> {
        // Only an explicit selection grants the unrelated-document exemption.
        // An app-only command follows the current keyboard destination.
        let window = window.filter(|_| explicit);
        let refs = self.window_refs(app, pid)?;
        if let Some(window) = window {
            let attached = refs
                .iter()
                .any(|entry| entry.element == *window && entry.parent.is_some());
            if attached || self.text(window, "AXRole")?.as_deref() == Some("AXSheet") {
                if !refs.iter().any(|entry| entry.element == *window) {
                    return Err(Unread::Unlisted.error());
                }
                if let Some(point) = point {
                    self.at_sheet(window, point)?;
                }
            }
        }
        for entry in &refs {
            self.admit_window(&entry.element, entry.parent.as_ref(), window, pid, explicit)?;
        }
        if let Some(window) = window
            && !refs.iter().any(|entry| entry.element == *window)
        {
            self.admit_window(window, None, Some(window), pid, explicit)?;
            if !self.sheets(window, pid)?.is_empty() {
                return Err(refused("sheet", "A sheet is open over the target window"));
            }
        }
        let focused = self.reference(app, "AXFocusedWindow")?;
        let main = self.reference(app, "AXMainWindow")?;
        for current in [focused.as_ref(), main.as_ref()].into_iter().flatten() {
            let parent = refs
                .iter()
                .find(|entry| entry.element == *current)
                .and_then(|entry| entry.parent.as_ref());
            self.admit_window(current, parent, window, pid, explicit)?;
            for sheet in self.sheets(current, pid)? {
                self.admit_window(&sheet, Some(current), window, pid, explicit)?;
            }
        }
        if let Some(mut element) = self.reference(app, "AXFocusedUIElement")? {
            for depth in 0..256 {
                self.owned(&element, pid)?;
                if Some(&element) == window {
                    break;
                }
                let role = self.text(&element, "AXRole")?;
                if role.as_deref() == Some("AXApplication") {
                    break;
                }
                let parent = refs
                    .iter()
                    .find(|entry| entry.element == element)
                    .and_then(|entry| entry.parent.as_ref());
                self.admit_window(&element, parent, window, pid, explicit)?;
                if parent.is_some() || matches!(role.as_deref(), Some("AXWindow" | "AXSheet")) {
                    if role.as_deref() != Some("AXSheet")
                        && parent.is_none()
                        && window == focused.as_ref()
                        && window.is_some()
                    {
                        return Err(Unread::Failed.error());
                    }
                    break;
                }
                element = self
                    .reference(&element, "AXParent")?
                    .ok_or_else(|| Unread::Failed.error())?;
                if depth == 255 {
                    return Err(Unread::Deep.error());
                }
            }
        }
        Ok(())
    }

    fn at_sheet(&mut self, sheet: &A::Element, (x, y): (f64, f64)) -> Result<()> {
        let frame = self
            .window(sheet)?
            .frame
            .ok_or_else(|| Unread::Malformed.error())?;
        if x >= frame.x && y >= frame.y && x < frame.x + frame.width && y < frame.y + frame.height {
            Ok(())
        } else {
            Err(refused("sheet", "The point is outside the target sheet"))
        }
    }

    fn find_field(&mut self, window: &A::Element, pid: i64, name: &str) -> Result<A::Element> {
        let filter = ElementFilter {
            name: Some(name.to_owned()),
            ..Default::default()
        };
        let mut stack = vec![(window.clone(), 0)];
        let mut seen = Vec::new();
        let mut best = None;
        let mut matches = Vec::new();
        let mut identifiers = Vec::new();
        let mut failed_names = false;
        let mut failed_identifier = false;
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
                failed_names |= failed.contains(&"AXTitle") || failed.contains(&"AXDescription");
                failed_identifier |= failed.contains(&"AXIdentifier");
                // Identifiers name their own namespace and only match whole.
                // Labels cannot create or conceal an identifier duplicate.
                if described
                    .identifier
                    .take()
                    .is_some_and(|identifier| same_text(&identifier, name))
                {
                    identifiers.push((element.clone(), described.enabled));
                }
                if let Some(rank) = filter.match_of(&described) {
                    if best.is_none_or(|best| rank > best) {
                        best = Some(rank);
                        matches.clear();
                    }
                    if best == Some(rank) {
                        matches.push((element.clone(), described.enabled));
                    }
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
            if failed_identifier {
                return Err(Unread::Failed.error());
            }
            matches = identifiers;
        } else if (failed_names || failed_identifier) && best.is_some() {
            return Err(refused(
                "field",
                "The fields cannot be told apart because a name could not be read; a whole-name match is required",
            ));
        } else if matches.is_empty() && failed_identifier {
            return Err(Unread::Failed.error());
        }
        match matches.as_slice() {
            [(_, Some(false))] => Err(refused("disabled", "That field is disabled")),
            [(element, _)] => Ok(element.clone()),
            [] => Err(refused(
                "field",
                "No field in the target window has that name",
            )),
            _ => Err(refused(
                "field",
                "The named field could not be selected uniquely",
            )),
        }
    }
}

struct Target<A: Backend> {
    reader: Reader<A>,
    app: A::Element,
    window: Option<A::Element>,
    field: Option<A::Element>,
    number: Option<i64>,
    changed: bool,
    point: Option<(f64, f64)>,
}

impl<A: Backend> Target<A> {
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
                return Err(refused(
                    "field",
                    "The selected field did not receive keyboard focus",
                ));
            }
        }
        Ok(role)
    }
}

/// Admit the target even when it is already ready. Each write is guarded
/// independently; the callback never holds ownership across remote AX calls.
fn prepare<A: Backend>(
    ask: A,
    expect: &Expect,
    need: Need,
    deadline: Instant,
    guard: &impl Fn() -> Result<()>,
    ready: &mut impl FnMut(Option<i64>) -> bool,
) -> Result<Target<A>> {
    guard()?;
    let mut reader = Reader { ask, deadline };
    let app = reader
        .ask
        .app(expect.pid())
        .ok_or_else(|| refused("app", "The target app is no longer running"))?;
    reader.left()?;
    reader.owned(&app, expect.pid())?;
    if expect.field().is_some() {
        reader.text(&app, "AXRole")?;
    }
    let point = match need {
        Need::Point(x, y) => Some((x, y)),
        Need::Drag { from, .. } => Some(from),
        _ => None,
    };
    let mut menu = false;
    let mut number = if let Some(number) = expect.window() {
        Some(number)
    } else if let Some((x, y)) = point {
        let listed = reader.ask.windows()?;
        reader.left()?;
        menu = listed
            .iter()
            .any(|window| window.is_menu_bar() && window.bounds.contains(x, y));
        if menu {
            None
        } else {
            let candidate = listed
                .iter()
                .find(|window| {
                    window.pid == expect.pid()
                        && window.alpha > 0.0
                        && window.is_apps_or_menu()
                        && window.bounds.contains(x, y)
                })
                .ok_or_else(|| refused("point", "No window of the app is at that point"))?;
            menu = candidate.layer == windows::MENU_LAYER;
            if menu { None } else { Some(candidate.number) }
        }
    } else {
        None
    };
    let window = match number {
        Some(number) => Some(reader.resolve(&app, expect.pid(), number)?),
        None => {
            let focused = reader.reference(&app, "AXFocusedWindow")?;
            match focused {
                Some(window) => Some(window),
                None => reader.reference(&app, "AXMainWindow")?,
            }
        }
    };
    let raise = number.is_some() && !menu;
    if number.is_none() && expect.field().is_some() {
        let window = window
            .as_ref()
            .ok_or_else(|| refused("field", "No window of the app holds the named field"))?;
        reader.owned(window, expect.pid())?;
        let described = reader.window(window)?;
        let listed = reader.ask.windows()?;
        let frame = described.frame.ok_or_else(|| Unread::Malformed.error())?;
        number = Some(
            match windows::described(
                &listed,
                expect.pid(),
                &frame,
                described.title.as_deref(),
                Kind::of(described.subrole.as_deref()),
            ) {
                Listed::One(number) => number,
                Listed::Missing => return Err(Unread::Unlisted.error()),
                Listed::Several => return Err(Unread::Alike.error()),
            },
        );
    }
    reader.blocking(
        &app,
        window.as_ref(),
        expect.pid(),
        expect.window().is_some() || point.is_some(),
        point,
    )?;
    guard()?;
    if let (Some(number), Some((x, y))) = (expect.window(), point) {
        let listed = reader.ask.windows()?;
        reader.left()?;
        let window = listed
            .iter()
            .find(|window| window.pid == expect.pid() && window.number == number)
            .ok_or_else(|| refused("window", "The named window is not on screen"))?;
        if !window.bounds.contains(x, y) {
            return Err(refused(
                "window",
                &format!("Window {number} is not at that point"),
            ));
        }
    }
    if ready(number)
        && (point.is_none()
            || !raise
            || reader.reference(&app, "AXFocusedWindow")?.as_ref() == window.as_ref())
    {
        let mut field = None;
        if let Some(name) = expect.field()
            && let Some(focused) = reader.reference(&app, "AXFocusedUIElement")?
        {
            let described = reader.field(&focused)?;
            let filter = ElementFilter {
                name: Some(name.to_owned()),
                ..Default::default()
            };
            if filter.match_of(&described) == Some(NameMatch::Exact) {
                field = Some(focused);
            }
        }
        if expect.field().is_none() || field.is_some() {
            return Ok(Target {
                reader,
                app,
                window,
                field,
                number,
                changed: false,
                point,
            });
        }
    }
    reader.left()?;
    let field = if let Some(name) = expect.field() {
        let window = window
            .as_ref()
            .ok_or_else(|| refused("field", "No window of the app holds the named field"))?;
        Some(reader.find_field(window, expect.pid(), name)?)
    } else {
        None
    };
    let mut writes = vec![(app.clone(), Write::Flag("AXFrontmost"))];
    if raise {
        let window = window.as_ref().ok_or_else(|| Unread::Unlisted.error())?;
        writes.extend([
            (window.clone(), Write::Raise),
            (window.clone(), Write::Flag("AXMain")),
            (window.clone(), Write::Flag("AXFocused")),
        ]);
    }
    if let Some(field) = &field {
        writes.push((field.clone(), Write::Flag("AXFocused")));
    }
    let mut changed = false;
    for (element, operation) in writes {
        reader.blocking(
            &app,
            window.as_ref(),
            expect.pid(),
            expect.window().is_some() || point.is_some(),
            point,
        )?;
        guard()?;
        reader.bound(&element, REQUEST)?;
        let supported = reader.ask.supports(&element, operation, reader.deadline)?;
        reader.left()?;
        if !supported {
            continue;
        }
        reader.blocking(
            &app,
            window.as_ref(),
            expect.pid(),
            expect.window().is_some() || point.is_some(),
            point,
        )?;
        if field.as_ref() == Some(&element) {
            let described = Reader::field(&mut reader, &element)?;
            if described.enabled == Some(false) {
                return Err(refused("disabled", "That field is disabled"));
            }
            if reader.within(element.clone(), window.as_ref())? != Within::Window {
                return Err(Unread::Failed.error());
            }
        }
        if let (Some(window), Some(number)) = (&window, number) {
            reader.identity(window, expect.pid(), number)?;
        }
        guard()?;
        reader.bound(&element, WRITE_BUDGET)?;
        let status = reader.ask.write(&element, operation);
        if status == AXError::CannotComplete {
            return Err(Error::new(
                Code::Uncertain,
                "The app did not acknowledge the focus request",
            )
            .uncertain());
        }
        changed |= status == AXError::Success;
        guard()?;
        if status != AXError::Success
            && !(point.is_some()
                && window.as_ref() == Some(&element)
                && operation == Write::Flag("AXFocused"))
            && !matches!(
                status,
                AXError::AttributeUnsupported
                    | AXError::ActionUnsupported
                    | AXError::NotImplemented
            )
        {
            if field.as_ref() == Some(&element) {
                return Err(refused(
                    "field",
                    "The named field did not accept keyboard focus",
                ));
            }
            return Err(Unread::Failed.error());
        }
        reader.left()?;
    }
    reader.blocking(
        &app,
        window.as_ref(),
        expect.pid(),
        expect.window().is_some() || point.is_some(),
        point,
    )?;
    guard()?;
    Ok(Target {
        reader,
        app,
        window,
        field,
        number,
        changed,
        point,
    })
}

pub(crate) fn bring(
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
        let result = (|| {
            target.reader.blocking(
                &target.app,
                target.window.as_ref(),
                expect.pid(),
                expect.window().is_some() || target.point.is_some(),
                target.point,
            )?;
            let mut result = target.verify(checking, &mut verify);
            if result.is_ok() {
                target.reader.blocking(
                    &target.app,
                    target.window.as_ref(),
                    expect.pid(),
                    expect.window().is_some() || target.point.is_some(),
                    target.point,
                )?;
                // Admission is another observation. Its scan may outlive the
                // earlier proof, so verify the destination again before returning.
                result = target.verify(checking, &mut verify);
            }
            result
        })();
        guard()?;
        let result = result.and_then(|role| target.reader.left().map(|_| role));
        target.reader.deadline = deadline;
        let error = match result {
            Ok(role) => return Ok(role),
            Err(error) if error.details.get("unread").is_some_and(|why| why == "slow") => {
                return Err(last_refusal.unwrap_or(error));
            }
            Err(error) => error,
        };
        if !target.changed
            || error.code != Code::FocusChanged
            || !matches!(
                error.details.get("reason").and_then(|value| value.as_str()),
                Some("app" | "window" | "point" | "field")
            )
            || target.reader.left().is_err()
        {
            return Err(error);
        }
        // A refused destination never authorizes input. Keep its actual reason
        // if waiting for a focus request to take effect exhausts the deadline.
        last_refusal = Some(error);
        std::thread::sleep(
            Duration::from_millis(10)
                .min(deadline.saturating_duration_since(target.reader.ask.now())),
        );
        if target.reader.ask.now() >= deadline || Instant::now() >= deadline {
            guard()?;
            return Err(last_refusal.unwrap());
        }
    }
}

#[cfg(test)]
mod tests;

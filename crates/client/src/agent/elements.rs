//! Accessibility elements of an app's windows: looked at by the helper's
//! input service, which keeps each look, listed compactly or in full under
//! the look's name, and clicked or acted on through Accessibility by that
//! look and an element's index, or clicked by name or role only when
//! exactly one visible element matches. Screenshots find their app the
//! same way.
use super::{
    input::click,
    target::{Target, target_expect},
};
use crate::client::{Client, Run, Seen, Verified, WINDOW_LOOKUP, require_looks};
use atlas_core::{
    actions::{Action, Batch, Button},
    desktop::Kind,
    error::{Code, Error, Result},
    protocol::{Crop, Part, Region},
    screen::{
        elements::{
            Element, Look, Reference, Stop, click_point, drawn, element_check, stale, telling, why,
        },
        geometry::Bounds,
        select::{self, Choice, ElementFilter, closeness, names, shown_name},
        windows::Window,
    },
    transport::run_output,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{collections::BTreeMap, time::Instant};

/// Most accessibility failures a scan's diagnostics name.
const SCAN_ERRORS: usize = 8;

/// Longest failure key a scan's diagnostics name, in bytes.
const SCAN_ERROR_KEY: usize = 64;

/// Most elements a refused name lists as what it may have meant.
const CANDIDATES: usize = 5;

/// How a listing names each element.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Form {
    /// Its role, name, value and state, where a click lands, its size and
    /// its window.
    Compact,
    /// Its role, name, value and state alone.
    Values,
    /// Every attribute the scan read, as it read it.
    Full,
}

/// Which elements of an app a listing shows, and how.
#[derive(Debug, Clone)]
pub struct Asked {
    /// Only elements with this role.
    pub role: Option<String>,
    /// Only elements with any of these names; every element when empty.
    pub names: Vec<String>,
    /// Only elements of this window, by its number.
    pub window: Option<i64>,
    /// Only what a web page shows: the elements inside its web area.
    pub page: bool,
    /// At most this many elements.
    pub max: usize,
    pub form: Form,
}

impl Asked {
    /// Every element, compactly, at most `max` of them.
    #[cfg(test)]
    pub(super) fn every(max: usize) -> Self {
        Self {
            role: None,
            names: Vec::new(),
            window: None,
            page: false,
            max,
            form: Form::Compact,
        }
    }

    /// One filter for each name asked for, each with the role asked for;
    /// one with no name when none is.
    fn filters(&self) -> Vec<ElementFilter> {
        let filter = |name: Option<&String>| ElementFilter {
            role: self.role.clone(),
            name: name.cloned(),
        };
        if self.names.is_empty() {
            return vec![filter(None)];
        }
        self.names.iter().map(Some).map(filter).collect()
    }
}

/// One scan of an app, as the probe or the input service reported it.
pub(super) struct Scanned {
    /// The look the input service keeps it as; none from the probe.
    pub(super) look: Option<Look>,
    pub(super) elements: Vec<Element>,
    pub(super) incomplete: bool,
    /// Whether the reply said, as a flag, if the scan was complete.
    pub(super) coverage_told: bool,
    /// Why it is incomplete, each reason once.
    pub(super) reasons: Vec<Stop>,
    /// Accessibility failures other than a missing attribute, by count.
    pub(super) errors: BTreeMap<String, usize>,
}

impl Scanned {
    /// The probe's or the input service's reply, read. An older probe
    /// names no reasons, and the probe no look.
    pub(super) fn from_reply(mut value: Value) -> Result<Self> {
        if value["ok"] != true {
            return Err(Error::new(
                Code::Elements,
                value["error"]
                    .as_str()
                    .unwrap_or("The accessibility probe failed")
                    .to_owned(),
            ));
        }

        let look = match &value["look"] {
            Value::Null => None,
            look => Some(look.as_str().and_then(Look::parse).ok_or_else(|| {
                Error::new(Code::Elements, "The input service named a malformed look")
            })?),
        };
        let elements = serde_json::from_value(value["elements"].take()).map_err(|cause| {
            Error::new(Code::Elements, "Malformed accessibility elements").with_source(cause)
        })?;

        let mut reasons: Vec<Stop> = value["reasons"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .filter_map(Stop::parse)
            .collect();
        reasons.sort_unstable();
        reasons.dedup();

        let mut errors: Vec<(String, usize)> = value["errors"]
            .as_object()
            .into_iter()
            .flatten()
            .filter(|(key, _)| key.len() <= SCAN_ERROR_KEY && telling(key))
            .filter_map(|(key, count)| Some((key.clone(), usize::try_from(count.as_u64()?).ok()?)))
            .collect();
        errors.sort_by(|(a, first), (b, second)| second.cmp(first).then_with(|| a.cmp(b)));
        errors.truncate(SCAN_ERRORS);

        let coverage = value["truncated"].as_bool();
        Ok(Self {
            look,
            elements,
            incomplete: coverage == Some(true),
            coverage_told: coverage.is_some(),
            reasons,
            errors: errors.into_iter().collect(),
        })
    }

    /// Why the scan stopped early and what failed in it, for programs:
    /// None when there is nothing to say.
    pub(super) fn diagnostics(&self) -> Option<Value> {
        let mut said = serde_json::Map::new();
        if !self.reasons.is_empty() {
            said.insert("reasons".into(), json!(self.reasons));
        }
        if !self.errors.is_empty() {
            said.insert("errors".into(), json!(self.errors));
        }

        (!said.is_empty()).then_some(Value::Object(said))
    }

    /// A named click refused because the scan was incomplete.
    pub(super) fn refusal(&self) -> Error {
        let why = why(self.reasons.iter().copied())
            .map(|why| format!(" ({why})"))
            .unwrap_or_default();
        let mut error = Error::new(
            Code::Elements,
            format!(
                "The accessibility scan was incomplete{why}, so a unique match cannot be \
                 established"
            ),
        );
        if let Some(diagnostics) = self.diagnostics() {
            error = error.with("scan", diagnostics);
        }

        let next = if self.withheld() {
            WITHHELD_NEXT
        } else {
            "Try again. If it stays incomplete, click the point from a screenshot instead."
        };
        error.with("next", json!(next))
    }

    /// This scan with only the elements of on-screen window `number`, one of
    /// app `pid`'s; the scan's other facts stay as they were.
    pub(super) fn in_window(mut self, shown: &[Window], pid: i64, number: i64) -> Result<Self> {
        if !shown
            .iter()
            .any(|window| window.number == number && window.pid == pid)
        {
            return Err(Error::new(
                Code::NoWindow,
                format!(
                    "No window {number} of that app is on screen: --window takes a number from \
                     atlas windows or a listing's windows"
                ),
            ));
        }
        // Each element is told by its own window frame: one that could be
        // window `number` or another of the app's cannot be told apart.
        let doubtful = self.elements.iter().any(|element| {
            element.window.as_ref().is_some_and(|frame| {
                let there = at_frame(shown, pid, frame);
                there.len() > 1 && there.contains(&number)
            })
        });
        if doubtful {
            return Err(Error::new(
                Code::Elements,
                format!(
                    "Another window of the app has window {number}'s frame, so its elements \
                     cannot be told apart: move one of them, or list them all"
                ),
            ));
        }
        self.elements.retain(|element| {
            element
                .window
                .as_ref()
                .is_some_and(|frame| at_frame(shown, pid, frame) == [number])
        });
        Ok(self)
    }

    /// This scan with only what its web pages show: the elements inside
    /// each web area, which the depth-first scan lists right after it, deeper
    /// than it. Refused when the scan found no web area.
    pub(super) fn in_page(mut self) -> Result<Self> {
        let mut inside: Option<usize> = None;
        let mut found = false;
        self.elements.retain(|element| {
            if let Some(depth) = inside {
                if element.depth > depth {
                    return true;
                }
                inside = None;
            }
            if element.role.as_deref() == Some(WEB_AREA) {
                inside = Some(element.depth);
                found = true;
            }
            false
        });
        if found {
            return Ok(self);
        }

        // An app that withheld its windows needs the screen unlocked, not
        // time to load.
        let (said, next) = if self.withheld() {
            (
                "The app withheld its windows, so no web page could be read",
                WITHHELD_NEXT,
            )
        } else if self.incomplete {
            (
                "The accessibility scan stopped before it reached a web page",
                "Look again once the page has loaded, or list without --page",
            )
        } else {
            (
                "No web page is open in that app's windows",
                "Look again once the page has loaded, or list without --page",
            )
        };
        let mut error = Error::new(Code::Elements, said).with("next", json!(next));
        if let Some(diagnostics) = self.diagnostics().filter(|_| self.incomplete) {
            error = error.with("scan", diagnostics);
        }
        Err(error)
    }

    /// Whether the app handed back itself in place of its windows.
    fn withheld(&self) -> bool {
        self.reasons.contains(&Stop::Withheld)
    }
}

/// The role of the area that holds a web page's own elements.
const WEB_AREA: &str = "AXWebArea";

/// What to do about an app that withheld its windows from the scan.
pub(super) const WITHHELD_NEXT: &str = "Ask the person to unlock the Mac: apps hide their windows from \
                             Accessibility while its screen is locked. Then look again.";

/// The accessibility elements of process `pid`, scanned in the helper
/// `verified` describes, which the caller verified just before.
pub(super) async fn app_elements(
    client: &Client,
    verified: &Verified,
    pid: i64,
) -> Result<Scanned> {
    scanned(&probe(client, Run::inspect().pinned(verified), pid).await?)
}

/// [`app_elements`], the helper stopping the scan after `timeout`; None
/// when it did, before the scan said anything.
pub(super) async fn app_elements_within(
    client: &Client,
    verified: &Verified,
    pid: i64,
    timeout: std::time::Duration,
) -> Result<Option<Scanned>> {
    let reply = probe(client, Run::inspect().within(timeout).pinned(verified), pid).await?;
    if reply["timedOut"] == true {
        return Ok(None);
    }
    scanned(&reply).map(Some)
}

/// Run the daemon's probe over process `pid` as `run` says.
async fn probe(client: &Client, run: Run<'_>, pid: i64) -> Result<Value> {
    match client.desktop().kind() {
        Kind::User(login) => probe_in(client, login, run, pid).await,
    }
}

/// [`probe`] in the second login `login`, which runs the shared Atlas
/// command from this disk.
async fn probe_in(
    client: &Client,
    _login: &atlas_core::user::User,
    run: Run<'_>,
    pid: i64,
) -> Result<Value> {
    let probe = client.config.identity_probe.to_string_lossy().into_owned();
    let argv = vec![probe, "elements-info".into(), pid.to_string()];
    client.run(argv, run).await
}

/// The scan a probe's run `reply` printed.
fn scanned(reply: &Value) -> Result<Scanned> {
    let missing = || Error::new(Code::Elements, "The accessibility probe returned no result");
    let bytes = run_output(reply, 8 << 20).map_err(|cause| missing().with_source(cause))?;
    let value = serde_json::from_slice(&bytes).map_err(|cause| missing().with_source(cause))?;

    Scanned::from_reply(value)
}

/// Accessibility elements of the target app's windows that `asked` is for,
/// in its form, under the name of the look a click names one by, with their
/// indexes; only those of its window, or of its web pages, when it says so.
/// Each window a compact listing names carries its number when the window
/// list tells it. The helper's input service reads them and keeps the look;
/// listing them grants nothing.
///
/// # Errors
///
/// Returns identity, target-resolution, transport or accessibility-read failures,
/// refuses a window number that is not one of the app's on screen, a page asked
/// for where the scan found none, and an input service that cannot keep looks.
/// An incomplete window observation cannot establish remembered routing evidence.
pub async fn elements(client: &Client, target: &Target, asked: &Asked) -> Result<Value> {
    let window = asked.window;
    let verified = client.verify().await?;
    let expect = target_expect(client, &verified, target).await?;
    let (scanned, seen) = client
        .look_seeing(&verified, window.is_some(), async {
            let scanned = Scanned::from_reply(client.look_at(&verified, &expect).await?)?;
            match scanned.look {
                Some(_) => Ok(scanned),
                None => Err(Error::new(
                    Code::Elements,
                    "The input service named no look",
                )),
            }
        })
        .await?;

    let pid = expect.pid();
    let shown = match seen {
        Seen::Stayed(shown) => Some(shown),
        Seen::Moved | Seen::Unread => None,
    };
    let scanned = match (window, &shown) {
        (Some(number), Some(shown)) => scanned.in_window(shown, pid, number)?,
        (Some(_), None) => {
            return Err(Error::new(
                Code::Elements,
                "The app's windows moved while it was read, so its elements cannot be told by \
                 window: look again",
            ));
        }
        (None, _) => scanned,
    };
    let scanned = if asked.page {
        scanned.in_page()?
    } else {
        scanned
    };
    Ok(listing_in(pid, &scanned, asked, shown.as_deref()))
}

/// The numbers of every window of app `pid` at `frame`, at any layer.
fn at_frame(shown: &[Window], pid: i64, frame: &Bounds) -> Vec<i64> {
    shown
        .iter()
        .filter(|window| window.pid == pid && window.bounds.matches(frame))
        .map(|window| window.number)
        .collect()
}

/// The number of the one window of app `pid` at `frame`, at any layer;
/// none when no window, or more than one, is there.
fn alone_at(shown: &[Window], pid: i64, frame: &Bounds) -> Option<i64> {
    match at_frame(shown, pid, frame).as_slice() {
        [number] => Some(*number),
        _ => None,
    }
}

/// Click the element `reference` names, as the look that listed it saw it.
/// The input service finds that very object again before anything is
/// checked or sent, then clicks it where it is now under every check a
/// named click has. With `target`, the look must be of that app.
///
/// # Errors
///
/// Refuses a malformed reference, and one whose look is gone or of another
/// app or display, or whose element changed or is disabled, with nothing
/// sent and the advice to look again. Identity, ownership, focus and
/// delivery errors propagate; an uncertain outcome must be recovered before
/// another mutation.
pub async fn click_reference(
    client: &Client,
    target: Option<&Target>,
    reference: &str,
    which: Button,
    count: u8,
    modifiers: &[String],
) -> Result<Value> {
    let action = json!({
        "type": "reference",
        "ref": reference,
        "button": which,
        "count": count,
        "modifiers": modifiers,
    });
    crate::asking::after_asking(client, async || referenced(client, target, &action).await).await
}

/// Do `action` to the element its `ref` names through Accessibility alone,
/// one action as `atlas input` takes it (`{"type": "press", "ref": …}`).
/// The input service finds that very object again and proves it twice,
/// the second time just before the one request that acts; no pointer or
/// key event is sent and nothing is asked to take focus. With `target`,
/// the look must be of that app. The reply says only how many actions were
/// performed, how, and whether the element reads as asked afterwards.
///
/// # Errors
///
/// Refuses, with nothing sent, a malformed ref, one whose look is gone or
/// of another app or display, an element that changed, is disabled, does
/// not support the action or is a password field given a value, and an
/// input service that cannot act on elements. Identity and ownership
/// errors propagate; an action the app did not acknowledge is uncertain and
/// must be recovered, never repeated.
pub async fn element(client: &Client, target: Option<&Target>, action: &Value) -> Result<Value> {
    // Control is taken again for each attempt, so none is held while the
    // person is asked.
    crate::asking::after_asking(client, async || element_once(client, target, action).await).await
}

/// [`element`] once: a refusal to ask the person comes back as it is.
async fn element_once(client: &Client, target: Option<&Target>, action: &Value) -> Result<Value> {
    // An action that does not decode is refused as its batch is, below.
    if Action::deserialize(action).is_ok_and(|action| action.element_action().is_none()) {
        return Err(Error::new(
            Code::Input,
            "Only an action on an element is done this way; nothing was sent",
        ));
    }
    let reply = referenced(client, target, action).await?;
    if reply["ok"] != true {
        return Ok(reply);
    }
    let mut said = json!({
        "ok": true,
        "performed": reply["performed"],
        "mechanism": reply["mechanism"],
        "verification": reply["verification"],
    });
    if let Some(warnings) = reply.get("warnings") {
        said["warnings"] = warnings.clone();
    }
    Ok(said)
}

/// Send `action`, which names an element by the `LOOK.INDEX` in its `ref`
/// (and for a pick, its `option`), as a batch of its own under one ownership and the
/// epoch verified first. A ref that is not one is refused before anything
/// is asked of the helper.
async fn referenced(client: &Client, target: Option<&Target>, action: &Value) -> Result<Value> {
    let malformed = ["ref", "option"].iter().any(|key| {
        action
            .get(key)
            .is_some_and(|text| text.as_str().and_then(Reference::parse).is_none())
    });
    if malformed {
        return Err(stale("That is not a ref from atlas elements").with("performed", json!(0)));
    }

    let ownership = client.controller.acquire_beside_commands()?;
    let verified = client.verify().await?;
    require_looks(&verified)?;
    let check = match target {
        Some(target) => Some(target_expect(client, &verified, target).await?),
        None => None,
    };

    let actions = json!([action]);
    client
        .input_owned(
            &ownership,
            Batch::decode(&actions),
            Some((&verified.session().epoch, Instant::now())),
            check,
        )
        .await
}

/// Save a screenshot of the whole display, of `target`'s windows, of
/// `region`, of window number `window` alone, or of `target`'s part of
/// `region`; or name the earlier file when nothing changed (unless
/// `new_file`). The app is found as `elements` finds it; a window or an
/// app's part of a region is cropped by the helper as its windows are when
/// it captures, naming what covers it.
///
/// # Errors
///
/// Rejects a window number that is not on screen (or not the app's), a
/// region that shows none of the app's windows, an older helper that
/// cannot crop a part, unavailable targets, invalid geometry, and capture
/// or local-file failures. A read failure does not imply that input was
/// sent.
pub async fn screenshot(
    client: &Client,
    target: Option<&Target>,
    region: Option<Region>,
    window: Option<i64>,
    size: crate::client::Size,
    new_file: bool,
) -> Result<Value> {
    // An app is found in the helper verified first, and captured only if
    // the helper is still that one: its pid means nothing in another.
    let verified = match (target, window) {
        (None, None) => None,
        _ => Some(client.verify().await?),
    };
    let pid = match (target, &verified) {
        (Some(target), Some(verified)) => {
            Some(target_expect(client, verified, target).await?.pid())
        }
        _ => None,
    };
    let crop = match (pid, region, window) {
        (None, region, None) => region.map(Crop::Region),
        (Some(pid), None, None) => Some(Crop::App { pid }),
        (Some(pid), Some(within), None) => Some(Crop::Part {
            pid,
            part: Part::Within(within),
        }),
        (pid, _, Some(number)) => {
            // A window named alone is its owner's.
            let pid = match (pid, &verified) {
                (Some(pid), _) => pid,
                (None, Some(verified)) => client
                    .windows_in(Some(verified), false, WINDOW_LOOKUP)
                    .await?
                    .windows
                    .iter()
                    .find(|shown| shown.number == number)
                    .map(|shown| shown.pid)
                    .ok_or_else(|| no_window(number))?,
                (None, None) => return Err(no_window(number)),
            };
            Some(Crop::Part {
                pid,
                part: Part::Window(number),
            })
        }
    };

    let captured = match &verified {
        Some(verified) => {
            client
                .capture_expecting(
                    Some(&verified.session().epoch),
                    size,
                    crop.as_ref(),
                    new_file,
                )
                .await
        }
        None => client.capture(size, crop.as_ref(), new_file).await,
    };
    captured.map_err(|error| match (target, window, region) {
        (_, Some(number), _) if error.code == Code::NoWindow => no_window(number),
        (Some(Target::Name(name)), None, Some(_)) if error.code == Code::NoWindow => Error::new(
            Code::NoWindow,
            format!("That region shows none of {name}'s windows: check them with atlas windows"),
        ),
        (Some(Target::Name(name)), None, None) if error.code == Code::NoWindow => Error::new(
            Code::NoWindow,
            format!(
                "{name} has no window on screen: open one, or take a full screenshot to see \
                     its menu bar"
            ),
        ),
        _ => error,
    })
}

/// The refusal of window `number`, which is not on screen.
fn no_window(number: i64) -> Error {
    Error::new(
        Code::NoWindow,
        format!(
            "No window {number} of that app is on screen: --window takes a number from atlas \
             windows or a listing's windows"
        ),
    )
}

/// The elements of one scan that match `filter`, as `elements` lists them:
/// each by its role, name, value, click point and size, its window listed
/// once, or with every attribute the scan read when `full`, under the
/// look's name when the scan is a kept look. A name that matches nothing
/// lists the closest names.
#[cfg(test)]
pub(super) fn listing(
    pid: i64,
    scanned: &Scanned,
    filter: &ElementFilter,
    max: usize,
    full: bool,
) -> Value {
    let asked = Asked {
        role: filter.role.clone(),
        names: filter.name.iter().cloned().collect(),
        form: if full { Form::Full } else { Form::Compact },
        ..Asked::every(max)
    };
    listing_in(pid, scanned, &asked, None)
}

/// The listing of `scanned` that `asked` is for. Each window a compact
/// listing names carries its number when the windows `on_screen` have
/// exactly one of app `pid`'s at its frame, told by the very frame the scan
/// gave it.
pub(super) fn listing_in(
    pid: i64,
    scanned: &Scanned,
    asked: &Asked,
    on_screen: Option<&[Window]>,
) -> Value {
    let filters = asked.filters();
    let narrowed = asked.role.is_some() || !asked.names.is_empty();
    let matching: Vec<&Element> = scanned
        .elements
        .iter()
        .filter(|element| filters.iter().any(|filter| filter.matches(element)))
        .filter(|element| asked.form == Form::Full || narrowed || !plain_group(element))
        .collect();
    let shown = &matching[..asked.max.min(matching.len())];

    let mut listed = json!({
        "ok": true,
        "pid": pid,
        "scanned": scanned.elements.len(),
        "scanIncomplete": scanned.incomplete,
        "matching": matching.len(),
        "more": matching.len() - shown.len(),
    });
    if let Some(look) = scanned.look {
        listed["look"] = json!(look.to_string());
    }

    match asked.form {
        Form::Full => listed["elements"] = json!(shown),
        Form::Values => listed["elements"] = shown.iter().copied().map(stated).collect(),
        Form::Compact => {
            let mut windows: Vec<&Bounds> = Vec::new();
            listed["elements"] = shown
                .iter()
                .map(|element| compact(element, &mut windows))
                .collect();
            listed["windows"] = numbered(pid, shown, &windows, on_screen);
        }
    }

    // What failed in a complete scan (an app's own getter answering
    // failure, say) changes nothing an agent does.
    if let Some(diagnostics) = scanned.diagnostics().filter(|_| scanned.incomplete) {
        listed["scan"] = diagnostics;
    }
    if scanned.withheld() {
        listed["next"] = json!(WITHHELD_NEXT);
    }

    // Each name that matched nothing, with what it may have meant.
    let unmatched: Vec<&ElementFilter> = filters
        .iter()
        .filter(|filter| filter.name.is_some())
        .filter(|filter| !matching.iter().any(|element| filter.matches(element)))
        .collect();
    let in_play = select::in_play(&scanned.elements);
    let mut close: Vec<Value> = Vec::new();
    for filter in &unmatched {
        for candidate in candidates(&in_play, &[], filter) {
            if !close.contains(&candidate) {
                close.push(candidate);
            }
        }
    }
    if !close.is_empty() {
        listed["candidates"] = json!(close);
    }
    if filters.len() > 1 && !unmatched.is_empty() {
        listed["unmatched"] = unmatched
            .iter()
            .filter_map(|filter| filter.name.as_deref())
            .collect();
    }

    listed
}

/// The windows a compact listing named, in order. A listed window is named
/// by number only when every element listed in it is told, by its own
/// frame, to be in that one window of app `pid` `on_screen`. Each element is
/// in the first listed window its frame matches, as `compact` placed it.
fn numbered(
    pid: i64,
    shown: &[&Element],
    windows: &[&Bounds],
    on_screen: Option<&[Window]>,
) -> Value {
    windows
        .iter()
        .enumerate()
        .map(|(entry, frame)| {
            let number = on_screen.and_then(|on_screen| {
                let mut told = shown
                    .iter()
                    .filter_map(|element| element.window.as_ref())
                    .filter(|frame| {
                        windows.iter().position(|seen| seen.matches(frame)) == Some(entry)
                    })
                    .map(|frame| alone_at(on_screen, pid, frame));
                let first = told.next()??;
                told.all(|number| number == Some(first)).then_some(first)
            });
            let mut listed = json!({
                "x": whole(frame.x),
                "y": whole(frame.y),
                "width": whole(frame.width),
                "height": whole(frame.height),
            });
            if let Some(number) = number {
                listed["number"] = json!(number);
            }
            listed
        })
        .collect()
}

/// Whether `element` is a group with nothing to tell, which the default
/// listing leaves out.
fn plain_group(element: &Element) -> bool {
    element.role.as_deref() == Some("AXGroup")
        && shown_name(element).is_none()
        && element.value.as_deref().is_none_or(str::is_empty)
        && element.enabled != Some(false)
        && element.focused != Some(true)
        && element.selected != Some(true)
        && !element.menu
        && element.mark.is_none()
        && element.unreadable.is_empty()
        && element.truncated.is_empty()
}

/// An element as the default listing names it, with its window's place in
/// `windows` (added when new).
pub(super) fn compact<'a>(element: &'a Element, windows: &mut Vec<&'a Bounds>) -> Value {
    let mut listed = stated(element);
    if let Some(frame) = &element.frame {
        let (x, y) = click_point(element.role.as_deref(), frame);
        listed["at"] = json!([whole(x), whole(y)]);
        listed["size"] = json!([whole(frame.width), whole(frame.height)]);
    }
    if let Some(window) = &element.window {
        let place = windows
            .iter()
            .position(|seen| seen.matches(window))
            .unwrap_or_else(|| {
                windows.push(window);
                windows.len() - 1
            });
        listed["window"] = json!(place);
    }

    listed
}

/// An element as a listing of values names it: its index, role, name,
/// value and state, and what of it could not be read in full.
fn stated(element: &Element) -> Value {
    let mut listed = element_identity(element);
    listed["index"] = json!(element.index);
    if let Some(value) = element
        .value
        .as_deref()
        .filter(|value| !value.is_empty() && Some(*value) != listed["name"].as_str())
    {
        listed["value"] = json!(value);
    }

    let flags = [
        ("disabled", element.enabled == Some(false)),
        ("focused", element.focused == Some(true)),
        ("selected", element.selected == Some(true)),
        ("menu", element.menu),
    ];
    for (flag, set) in flags {
        if set {
            listed[flag] = json!(true);
        }
    }

    if let Some(mark) = &element.mark {
        listed["mark"] = json!(mark);
    }
    if !element.truncated.is_empty() {
        listed["truncated"] = json!(element.truncated);
    }
    if !element.unreadable.is_empty() {
        listed["unreadable"] = json!(element.unreadable);
    }

    listed
}

/// The one element a click by name or role is for. While a menu is open
/// only its items count (a click outside it just closes the menu). A whole
/// name wins over the first part of a longer label, judged over every match
/// before the one chosen must have a frame. Refused when no visible element
/// or several match, or when another element whose name could not be read
/// might match too.
pub(super) fn choose<'a>(elements: &'a [Element], filter: &ElementFilter) -> Result<&'a Element> {
    let element = match select::choose(elements, filter) {
        Choice::One(element) => element,
        Choice::NotOne { in_play, matching } => return Err(not_one(&in_play, &matching, filter)),
        Choice::Doubtful(_) => {
            return Err(Error::new(
                Code::Elements,
                "Another element's name could not be read whole, so this match may not be unique",
            ));
        }
    };

    if element.enabled == Some(false) {
        return Err(Error::new(
            Code::Input,
            "The selected control is disabled; no click was sent",
        )
        .with("next", json!(atlas_core::error::advice::DISABLED_CONTROL)));
    }

    Ok(element)
}

/// The refusal when `matching`, of the `elements` in play, is not one
/// element: what to do, and the candidates when there are any.
fn not_one(elements: &[&Element], matching: &[&Element], filter: &ElementFilter) -> Error {
    // What is in play is either an open menu's items or no menu item at all.
    let menu_open = elements.first().is_some_and(|element| element.menu);
    let candidates = candidates(elements, matching, filter);
    let closest = if candidates.is_empty() {
        ""
    } else {
        " The closest names are in candidates."
    };
    let error = Error::new(
        Code::Elements,
        match matching.len() {
            0 if menu_open => format!(
                "No item of the open menu matches. Run atlas elements --app NAME to see it, \
                 or press escape to close the menu.{closest}"
            ),
            0 => format!(
                "No visible element matches. Run atlas elements --app NAME to see what is \
                 there.{closest}"
            ),
            count => format!(
                "{count} elements match. Run atlas elements with the same --name to \
                 tell them apart, then add --role, or click the one you mean at its point."
            ),
        },
    );

    if candidates.is_empty() {
        error
    } else {
        error.with("candidates", json!(candidates))
    }
}

/// Up to five elements a refused name may have meant, each with its role,
/// name and click point: on several matches, those matches; on none, the
/// visible elements whose names are closest to the name asked for.
fn candidates(seen: &[&Element], matching: &[&Element], filter: &ElementFilter) -> Vec<Value> {
    if !matching.is_empty() {
        return matching
            .iter()
            .filter_map(|element| Some(clicked(element, point_of(element)?)))
            .take(CANDIDATES)
            .collect();
    }

    let Some(name) = filter.name.as_deref() else {
        return Vec::new();
    };
    let query: String = name.chars().flat_map(char::to_lowercase).collect();

    // Each element's closest name: by how it is close, then how far off.
    let mut close: Vec<((u8, usize, usize), &Element, &str)> = seen
        .iter()
        .filter(|element| element.frame.as_ref().is_some_and(drawn))
        .filter_map(|element| {
            names(element)
                .filter_map(|text| Some((closeness(&query, text)?, text)))
                .min()
                .map(|((tier, distance), text)| ((tier, distance, element.index), *element, text))
        })
        .collect();
    close.sort_by_key(|(rank, ..)| *rank);

    let mut listed: Vec<Value> = Vec::new();
    for (_, element, text) in close {
        let Some(point) = point_of(element) else {
            continue;
        };
        let mut candidate = clicked(element, point);
        candidate["name"] = json!(text);

        if !listed.contains(&candidate) {
            listed.push(candidate);
        }
        if listed.len() == CANDIDATES {
            break;
        }
    }

    listed
}

/// Click exactly one matching element at its click point. Uniqueness needs a
/// complete scan; the point must be uncovered in the element's own window in
/// a window list read after the scan; and the click is posted under one
/// ownership and the epoch observed at the start.
///
/// # Errors
/// Refuses incomplete or ambiguous scans and definitely disabled controls
/// before input. Identity, ownership, focus and delivery errors propagate;
/// an uncertain outcome must be recovered before another mutation.
pub async fn click_element(
    client: &Client,
    target: &Target,
    filter: &ElementFilter,
    which: Button,
    count: u8,
    modifiers: &[String],
) -> Result<Value> {
    crate::asking::after_asking(client, async || {
        click_element_once(client, target, filter, which, count, modifiers).await
    })
    .await
}

/// [`click_element`] once: a refusal to ask the person comes back as it is.
async fn click_element_once(
    client: &Client,
    target: &Target,
    filter: &ElementFilter,
    which: Button,
    count: u8,
    modifiers: &[String],
) -> Result<Value> {
    if filter.role.is_none() && filter.name.is_none() {
        return Err(Error::new(
            Code::Input,
            "Select the element with --name, --role or both",
        ));
    }

    let ownership = client.controller.acquire_beside_commands()?;
    let verified = client.verify().await?;
    let expect = target_expect(client, &verified, target).await?;
    let pid = expect.pid();
    let scanned = app_elements(client, &verified, pid).await?;
    if scanned.incomplete {
        return Err(scanned.refusal());
    }

    let element = choose(&scanned.elements, filter)?;
    let (x, y) = point_of(element).unwrap_or_default();

    // Resolve the element's owning window from a fresh list. Visibility is
    // checked after native preparation, under the same lock as the click.
    let windows = client.windows_in(None, true, WINDOW_LOOKUP).await?.windows;
    let check = element_check(element, &windows, expect, (x, y))?;
    client.picture_found(element);

    // The name it was found by is kept only as a control's caption: any
    // other element's name may be a document's text, a file or a title.
    let uncaptioned;
    let client = if crate::doing::captioned(element.role.as_deref()) {
        client
    } else {
        uncaptioned = client.clone().unlabelled();
        &uncaptioned
    };

    let actions = click(x, y, which, count, modifiers);
    let mut result = client
        .input_owned(
            &ownership,
            Batch::decode(&actions),
            Some((&verified.session().epoch, Instant::now())),
            Some(check),
        )
        .await?;
    result["element"] = clicked(element, (x, y));

    Ok(result)
}

/// Where a click on `element` lands, when it has a frame.
fn point_of(element: &Element) -> Option<(f64, f64)> {
    let frame = element.frame.as_ref()?;
    Some(click_point(element.role.as_deref(), frame))
}

/// A coordinate or length in whole points, as replies give them.
fn whole(points: f64) -> i64 {
    points.round() as i64
}

/// A clicked element as its reply names it: role, name and the point clicked.
pub(super) fn clicked(element: &Element, (x, y): (f64, f64)) -> Value {
    let mut named = element_identity(element);
    named["at"] = json!([whole(x), whole(y)]);
    named
}

/// What a reply calls an element: its role and name, each when it has one.
fn element_identity(element: &Element) -> Value {
    let mut named = json!({});
    if let Some(role) = &element.role {
        named["role"] = json!(role);
    }
    if let Some(name) = shown_name(element) {
        named["name"] = json!(name);
    }

    named
}

#[cfg(test)]
#[path = "elements_tests.rs"]
mod tests;

//! Accessibility elements of an app's windows: scanned in the helper,
//! listed compactly or in full, and clicked by name or role only when
//! exactly one visible element matches. Screenshots find their app the
//! same way.
use super::{
    input::click,
    target::{Target, target_expect, target_pid},
    windows::pointer_view,
};
use crate::{
    actions::{Button, Expect},
    client::{Check, Client, Run, Verified},
    daemon::protocol::{Crop, Region},
    error::{Code, Error, Result},
    native::{
        ax::{self, Element, Frame, Stop},
        windows::{Bounds, Dock, Shown, Window, hit_windows, owning_window},
    },
    select::{ElementFilter, NameMatch, closeness, names, shown_name},
    transport::run_output,
};
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// Most accessibility failures a scan's diagnostics name.
const SCAN_ERRORS: usize = 8;

/// Longest failure key a scan's diagnostics name, in bytes.
const SCAN_ERROR_KEY: usize = 64;

/// Most elements a refused name lists as what it may have meant.
const CANDIDATES: usize = 5;

/// One scan of an app, as the probe reported it.
pub(super) struct Scanned {
    pub(super) windows: Vec<Window>,
    pub(super) dock: Dock,
    pub(super) elements: Vec<Element>,
    pub(super) incomplete: bool,
    /// Why it is incomplete, each reason once.
    pub(super) reasons: Vec<Stop>,
    /// Accessibility failures other than a missing attribute, by count.
    pub(super) errors: BTreeMap<String, usize>,
}

impl Scanned {
    /// The probe's reply, read. An older probe names no reasons.
    pub(super) fn from_probe(mut value: Value) -> Result<Self> {
        if value["ok"] != true {
            return Err(Error::new(
                Code::Elements,
                value["error"]
                    .as_str()
                    .unwrap_or("The accessibility probe failed")
                    .to_owned(),
            ));
        }

        let elements = serde_json::from_value(value["elements"].take())
            .map_err(|_| Error::new(Code::Elements, "Malformed accessibility elements"))?;

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
            .filter(|(key, _)| key.len() <= SCAN_ERROR_KEY && ax::telling(key))
            .filter_map(|(key, count)| Some((key.clone(), usize::try_from(count.as_u64()?).ok()?)))
            .collect();
        errors.sort_by(|(a, first), (b, second)| second.cmp(first).then_with(|| a.cmp(b)));
        errors.truncate(SCAN_ERRORS);

        Ok(Self {
            windows: serde_json::from_value(value["windows"].take()).unwrap_or_default(),
            dock: serde_json::from_value(value["dock"].take()).unwrap_or_default(),
            elements,
            incomplete: value["truncated"] == true,
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
        let why = ax::why(self.reasons.iter().copied())
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
        error.with("next", next)
    }

    /// Whether the app handed back itself in place of its windows.
    fn withheld(&self) -> bool {
        self.reasons.contains(&Stop::Withheld)
    }
}

/// What to do about an app that withheld its windows from the scan.
pub(super) const WITHHELD_NEXT: &str = "Ask the person to unlock the Mac: apps hide their windows from \
                             Accessibility while its screen is locked. Then look again.";

/// The accessibility elements of process `pid`, scanned in the helper
/// `verified` describes, which the caller verified just before.
async fn app_elements(client: &Client, verified: &Verified, pid: i64) -> Result<Scanned> {
    let probe = client.config.identity_probe.to_string_lossy().into_owned();
    let argv = vec![probe, "elements-info".into(), pid.to_string()];
    let reply = client.run(argv, Run::inspect().pinned(verified)).await?;
    let value = run_output(&reply, 8 << 20)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .ok_or_else(|| Error::new(Code::Elements, "The accessibility probe returned no result"))?;

    Scanned::from_probe(value)
}

/// Accessibility elements of the target app's windows that match `filter`,
/// at most `max` of them, listed compactly unless `full`.
pub async fn elements(
    client: &Client,
    target: &Target,
    filter: &ElementFilter,
    max: usize,
    full: bool,
) -> Result<Value> {
    let verified = client.verify().await?;
    let pid = target_pid(client, &verified, target).await?;
    let scanned = app_elements(client, &verified, pid).await?;
    let display = &verified.session().epoch.display;
    client.remember_windows(&hit_windows(
        &scanned.windows,
        &Bounds::display(display.width, display.height),
        &scanned.dock,
    ))?;

    Ok(listing(pid, &scanned, filter, max, full))
}

/// Save a screenshot of the whole display, of `target`'s windows, or of
/// `region`, or name the earlier file when nothing changed (unless
/// `new_file`). The app is found as `elements` finds it.
pub async fn screenshot(
    client: &Client,
    target: Option<&Target>,
    region: Option<Region>,
    size: crate::client::Size,
    new_file: bool,
) -> Result<Value> {
    // An app is found in the helper verified first, and captured only if
    // the helper is still that one: its pid means nothing in another.
    let (crop, verified) = match (target, region) {
        (Some(_), Some(_)) => {
            return Err(Error::new(
                Code::Usage,
                "Give an app or a region to capture, not both",
            ));
        }
        (Some(target), None) => {
            let verified = client.verify().await?;
            let pid = target_pid(client, &verified, target).await?;
            (Some(Crop::App { pid }), Some(verified))
        }
        (None, region) => (region.map(Crop::Region), None),
    };

    let captured = match &verified {
        Some(verified) => {
            client
                .capture_in(verified, size, crop.as_ref(), new_file)
                .await
        }
        None => client.capture(size, crop.as_ref(), new_file).await,
    };
    captured.map_err(|error| match target {
        Some(Target::Name(name)) if error.code == Code::NoWindow => Error::new(
            Code::NoWindow,
            format!(
                "{name} has no window on screen: open one, or take a full screenshot to see \
                     its menu bar"
            ),
        ),
        _ => error,
    })
}

/// The elements of one scan that match `filter`, as `elements` lists them:
/// each by its role, name, value, click point and size, its window listed
/// once, or with every attribute the scan read when `full`. A name that
/// matches nothing lists the closest names.
pub(super) fn listing(
    pid: i64,
    scanned: &Scanned,
    filter: &ElementFilter,
    max: usize,
    full: bool,
) -> Value {
    let matching: Vec<&Element> = scanned
        .elements
        .iter()
        .filter(|element| filter.matches(element))
        .collect();
    let shown = &matching[..max.min(matching.len())];

    let mut listed = json!({
        "ok": true,
        "pid": pid,
        "scanned": scanned.elements.len(),
        "scanIncomplete": scanned.incomplete,
        "matching": matching.len(),
        "more": matching.len() - shown.len(),
    });

    if full {
        listed["elements"] = json!(shown);
    } else {
        let mut windows: Vec<&Frame> = Vec::new();
        listed["elements"] = shown
            .iter()
            .map(|element| compact(element, &mut windows))
            .collect();
        listed["windows"] = windows
            .iter()
            .map(|frame| {
                json!({
                    "x": whole(frame.x),
                    "y": whole(frame.y),
                    "width": whole(frame.width),
                    "height": whole(frame.height),
                })
            })
            .collect();
    }

    // What failed in a complete scan (an app's own getter answering
    // failure, say) changes nothing an agent does.
    if let Some(diagnostics) = scanned.diagnostics().filter(|_| scanned.incomplete) {
        listed["scan"] = diagnostics;
    }
    if scanned.withheld() {
        listed["next"] = json!(WITHHELD_NEXT);
    }

    if filter.name.is_some() && matching.is_empty() {
        let candidates = candidates(&in_play(&scanned.elements), &[], filter);
        if !candidates.is_empty() {
            listed["candidates"] = json!(candidates);
        }
    }

    listed
}

/// An element as the default listing names it, with its window's place in
/// `windows` (added when new).
fn compact<'a>(element: &'a Element, windows: &mut Vec<&'a Frame>) -> Value {
    let mut listed = json!({"index": element.index});
    let name = shown_name(element);

    if let Some(role) = &element.role {
        listed["role"] = json!(role);
    }
    if let Some(name) = name {
        listed["name"] = json!(name);
    }
    if let Some(value) = element
        .value
        .as_deref()
        .filter(|value| !value.is_empty() && Some(*value) != name)
    {
        listed["value"] = json!(value);
    }
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

/// The menu bar where it shows the point, which is the Window Server's
/// window rather than the app's.
pub(super) fn menu_bar_window(windows: &[Window], x: f64, y: f64) -> Option<i64> {
    windows
        .iter()
        .find(|window| window.is_menu_bar() && window.alpha > 0.0 && window.bounds.contains(x, y))
        .map(|window| window.number)
}

/// Whether a frame has any area to click: a collapsed element is not on
/// screen, whatever lies under its point.
fn drawn(frame: &Frame) -> bool {
    frame.width > 0.0 && frame.height > 0.0
}

/// The one element a click by name or role is for. While a menu is open
/// only its items count (a click outside it just closes the menu). A whole
/// name wins over the first part of a longer label, judged over every match
/// before the one chosen must have a frame. Refused when no visible element
/// or several match, or when another element whose name could not be read
/// might match too.
pub(super) fn choose<'a>(elements: &'a [Element], filter: &ElementFilter) -> Result<&'a Element> {
    let elements = in_play(elements);
    // What is in play is either an open menu's items or no menu item at all.
    let menu_open = elements.first().is_some_and(|element| element.menu);

    let matches: Vec<(&Element, NameMatch)> = elements
        .iter()
        .filter_map(|element| Some((*element, filter.match_of(element)?)))
        .collect();
    let best = matches.iter().map(|(_, kind)| *kind).max();
    let matching: Vec<&Element> = matches
        .iter()
        .filter(|(element, kind)| Some(*kind) == best && element.frame.as_ref().is_some_and(drawn))
        .map(|(element, _)| *element)
        .collect();

    // A menu bar title gives way to a window element of the same name
    // (a Help button beside a Help menu); --role AXMenuBarItem asks for it.
    let in_windows: Vec<&Element> = matching
        .iter()
        .copied()
        .filter(|element| element.role.as_deref() != Some("AXMenuBarItem"))
        .collect();
    let matching = if filter.role.is_none() && in_windows.len() == 1 {
        in_windows
    } else {
        matching
    };

    let [element] = matching.as_slice() else {
        let candidates = candidates(&elements, &matching, filter);
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

        return Err(if candidates.is_empty() {
            error
        } else {
            error.with("candidates", candidates)
        });
    };

    // An element whose selector attributes could not be read may be a
    // second, unseen match, unless it is collapsed and could never be chosen.
    let doubtful = elements.iter().any(|other| {
        other.index != element.index
            && (!other.unreadable.is_empty() || !other.truncated.is_empty())
            && other.frame.as_ref().is_none_or(drawn)
            && filter.might_match(other)
    });
    if doubtful {
        return Err(Error::new(
            Code::Elements,
            "Another element's name could not be read whole, so this match may not be unique",
        ));
    }

    Ok(element)
}

/// The elements a name is judged against: while a menu is open, only its
/// items (a click outside it just closes the menu).
fn in_play(elements: &[Element]) -> Vec<&Element> {
    let menu_open = elements.iter().any(|element| element.menu);
    elements
        .iter()
        .filter(|element| element.menu || !menu_open)
        .collect()
}

/// Up to five elements a refused name may have meant, each with its role,
/// name and click point: on several matches, those matches; on none, the
/// visible elements whose names are closest to the name asked for.
fn candidates(seen: &[&Element], matching: &[&Element], filter: &ElementFilter) -> Vec<Value> {
    let at = |element: &Element| {
        element
            .frame
            .as_ref()
            .map(|frame| click_point(element.role.as_deref(), frame))
    };

    if !matching.is_empty() {
        return matching
            .iter()
            .filter_map(|element| Some(clicked(element, at(element)?)))
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
        let Some(point) = at(element) else {
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
pub async fn click_element(
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
    let epoch = verified.session().epoch.clone();
    if scanned.incomplete {
        return Err(scanned.refusal());
    }

    let element = choose(&scanned.elements, filter)?;
    let (x, y) = element
        .frame
        .as_ref()
        .map(|frame| click_point(element.role.as_deref(), frame))
        .unwrap_or_default();

    // Resolve the element's owning window from a fresh list. Visibility is
    // checked after native preparation, under the same lock as the click.
    let Shown { windows, .. } = pointer_view(client).await?;
    let check = element_check(element, &windows, expect, (x, y))?;

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
        .input_owned(&ownership, &actions, &epoch, Some(check))
        .await?;
    result["element"] = clicked(element, (x, y));

    Ok(result)
}

/// The exact target selected by the scan, before native preparation.
pub(super) fn element_check(
    element: &Element,
    windows: &[Window],
    expect: Expect,
    (x, y): (f64, f64),
) -> Result<Check> {
    let pid = expect.pid();
    let expect = if element.role.as_deref() == Some("AXMenuBarItem") {
        menu_bar_window(windows, x, y)
            .ok_or_else(|| Error::new(Code::Elements, "The menu bar is no longer on screen"))?;
        expect
    } else {
        let number = element
            .window
            .as_ref()
            .and_then(|frame| owning_window(windows, pid, frame))
            .ok_or_else(|| {
                Error::new(
                    Code::Elements,
                    "The element's window could not be selected uniquely",
                )
            })?;
        let menu = element.menu
            || windows.iter().any(|window| {
                window.number == number && window.layer == crate::native::windows::MENU_LAYER
            });
        if menu {
            expect
        } else {
            expect.in_window(number)?
        }
    };

    Ok(Check::Required(expect))
}

/// Where a click on an element lands: the middle of its frame, or for a
/// checkbox or radio button with a label beside it (a frame much wider than
/// tall) the middle of its box, at the leading end of a frame that may also
/// span a long empty stretch after a short label. A tab drawn as a radio
/// button is clicked in its middle.
pub(super) fn click_point(role: Option<&str>, frame: &Frame) -> (f64, f64) {
    match role {
        Some("AXCheckBox" | "AXRadioButton") if frame.width > 2.0 * frame.height => (
            (frame.x + frame.height / 2.0).round(),
            (frame.y + frame.height / 2.0).round(),
        ),
        _ => frame.center(),
    }
}

/// A coordinate or length in whole points, as replies give them.
fn whole(points: f64) -> i64 {
    points.round() as i64
}

/// A clicked element as its reply names it: role, name and the point clicked.
pub(super) fn clicked(element: &Element, (x, y): (f64, f64)) -> Value {
    let mut named = json!({"at": [whole(x), whole(y)]});
    if let Some(role) = &element.role {
        named["role"] = json!(role);
    }
    if let Some(name) = shown_name(element) {
        named["name"] = json!(name);
    }

    named
}

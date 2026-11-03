//! What a scan reports: the elements it read with their frames, the
//! snapshot that holds them, the bounds a scan keeps to, where a click on
//! one of them lands and which window it must reach, and the references
//! that name one element of one look.
use super::{
    geometry::Bounds,
    windows::{MENU_LAYER, Window, owning_window},
};
use crate::{
    actions::Expect,
    error::{Code, Error, Result},
};
use serde::{Deserialize, Deserializer, Serialize};
use std::{collections::BTreeSet, fmt, time::Duration};

/// The AX status codes meaning an element simply lacks an attribute: no
/// value, unsupported, not implemented.
pub const ABSENT: [i32; 3] = [-25212, -25205, -25208];

/// Whether an `"attribute:code"` error key says something went wrong,
/// rather than that the element simply lacks the attribute.
pub fn telling(key: &str) -> bool {
    let code = key
        .rsplit_once(':')
        .and_then(|(_, code)| code.parse::<i32>().ok());

    code.is_none_or(|code| !ABSENT.contains(&code))
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Element {
    /// Position in depth-first order, stable within one snapshot.
    pub index: usize,
    pub depth: usize,
    /// Frame of the top-level window (or open menu) this element belongs to.
    /// Like the attributes below, it is left out when the scan did not read it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window: Option<Bounds>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subrole: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identifier: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub focused: Option<bool>,
    /// Whether a row or cell is selected; not read for other elements.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frame: Option<Bounds>,
    /// Selector attributes (role, title, description, value, identifier)
    /// that could not be read, so this element might match a selector unseen.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unreadable: Vec<String>,
    /// Text attributes reported only in part (their first 200 UTF-16 units,
    /// cut between characters); the element holds more than is shown.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub truncated: Vec<String>,
    /// In a menu the app has open (its menu bar, a status item or a
    /// context menu), where a click goes while the menu is open.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub menu: bool,
    /// A menu item's check mark (such as "✓"), when it shows one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mark: Option<String>,
}

/// One traversal's result.
#[derive(Debug, Clone, Serialize)]
pub struct Snapshot {
    pub elements: Vec<Element>,
    /// A limit, the deadline or an unreadable element cut the scan short, so
    /// absence from it proves nothing.
    pub incomplete: bool,
    /// Why it is incomplete.
    pub stops: BTreeSet<Stop>,
    /// "attribute:AX error code" -> count, for diagnosing incomplete scans.
    pub errors: std::collections::BTreeMap<String, usize>,
}

/// Bounds on one traversal.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub depth: usize,
    pub count: usize,
    pub time: Duration,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            depth: 14,
            count: 1500,
            time: Duration::from_secs(4),
        }
    }
}

/// Why a scan is incomplete.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Stop {
    /// Some elements are nested deeper than the scan reads.
    Depth,
    /// The app has more elements than the scan reads.
    Count,
    /// The scan ran out of time.
    Time,
    /// Part of the app could not be read.
    Unread,
    /// The app handed back itself in place of its windows.
    Withheld,
}

impl Stop {
    /// The reason a reply names by `word`, if it is one of these.
    pub fn parse(word: &str) -> Option<Stop> {
        Self::deserialize(serde_json::Value::from(word)).ok()
    }

    /// Why the scan stopped, in plain words.
    pub(crate) fn said(self) -> &'static str {
        match self {
            Stop::Time => "it ran out of time",
            Stop::Count => "the app has more elements than Atlas reads",
            Stop::Depth => "some elements are nested too deep",
            Stop::Unread => "part of the app could not be read",
            Stop::Withheld => {
                "the app withheld its windows, as macOS does while the Mac's screen is locked"
            }
        }
    }
}

/// Why a scan stopped, in plain words joined into one phrase; None when
/// there is no reason.
pub fn why(stops: impl IntoIterator<Item = Stop>) -> Option<String> {
    let said: Vec<&str> = stops.into_iter().map(Stop::said).collect();

    match said.as_slice() {
        [] => None,
        [one] => Some((*one).to_owned()),
        [rest @ .., last] => Some(format!("{} and {last}", rest.join(", "))),
    }
}

/// How many incarnations two letters tell apart.
pub const INCARNATIONS: u16 = 26 * 26;

/// One look the input service keeps: the service process that took it, by
/// its ID and an incarnation that tells it from another process the ID was
/// given before, and which of that process's looks it is, counted from 0
/// and never reused. Written as the process ID, two letters for the
/// incarnation, then the count in letters (`a` to `z`, then `aa`), such as
/// `8123qfc`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Look {
    pub service: u32,
    /// Below [`INCARNATIONS`].
    pub incarnation: u16,
    pub sequence: u64,
}

impl Look {
    /// The look `word` names, if it is written exactly as a listing writes it.
    pub fn parse(word: &str) -> Option<Self> {
        let (service, letters) = word.split_at(word.find(|c: char| !c.is_ascii_digit())?);
        if !letters.bytes().all(|byte| byte.is_ascii_lowercase()) {
            return None;
        }
        let (incarnation, count) = letters.split_at_checked(2)?;
        let sequence = (!count.is_empty())
            .then(|| {
                count.bytes().try_fold(0u128, |number, byte| {
                    number
                        .checked_mul(26)?
                        .checked_add(u128::from(byte - b'a') + 1)
                })
            })
            .flatten()?;
        Some(Self {
            service: decimal(service).filter(|service| *service > 0)?,
            incarnation: incarnation
                .bytes()
                .fold(0, |number, byte| number * 26 + u16::from(byte - b'a')),
            sequence: u64::try_from(sequence - 1).ok()?,
        })
    }

    /// The look after this one in the same service; none once they run out.
    pub fn after(self) -> Option<Self> {
        Some(Self {
            sequence: self.sequence.checked_add(1)?,
            ..self
        })
    }
}

impl fmt::Display for Look {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut letters = Vec::new();
        let mut number = u128::from(self.sequence) + 1;
        while number > 0 {
            number -= 1;
            letters.push(char::from(b'a' + (number % 26) as u8));
            number /= 26;
        }
        let incarnation = self.incarnation % INCARNATIONS;
        write!(
            f,
            "{}{}{}",
            self.service,
            char::from(b'a' + (incarnation / 26) as u8),
            char::from(b'a' + (incarnation % 26) as u8),
        )?;
        letters
            .iter()
            .rev()
            .try_for_each(|letter| fmt::Write::write_char(f, *letter))
    }
}

/// A number written in decimal digits as a listing writes one: no sign, no
/// leading zero.
fn decimal<T: std::str::FromStr>(digits: &str) -> Option<T> {
    let written = !digits.is_empty()
        && digits.bytes().all(|byte| byte.is_ascii_digit())
        && (digits == "0" || !digits.starts_with('0'));
    written.then(|| digits.parse().ok()).flatten()
}

/// One element of one look, as `atlas elements` names it: the look's name
/// followed by the element's index in it, such as `8123qfc12`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Reference {
    pub look: Look,
    pub index: usize,
}

impl Reference {
    /// The reference `text` is, if it is one exactly as a listing gives it.
    pub fn parse(text: &str) -> Option<Self> {
        let letters = text.find(|c: char| !c.is_ascii_digit())?;
        let index = letters + text[letters..].find(|c: char| c.is_ascii_digit())?;
        let (look, index) = text.split_at(index);
        Some(Self {
            look: Look::parse(look)?,
            index: decimal(index)?,
        })
    }
}

impl fmt::Display for Reference {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", self.look, self.index)
    }
}

impl Serialize for Reference {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Reference {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Self::parse(&text).ok_or_else(|| serde::de::Error::custom("not an element reference"))
    }
}

/// What an agent does about a reference that names nothing it can click.
pub const LOOK_AGAIN: &str = "Run atlas elements again.";

/// The refusal of a reference that is malformed, no longer kept, or whose
/// element changed: `why`, and to look again. Nothing was sent.
pub fn stale(why: &str) -> Error {
    Error::new(Code::Elements, why).with("next", serde_json::json!(LOOK_AGAIN))
}

/// Whether a frame has any area to click: a collapsed element is not on
/// screen, whatever lies under its point.
pub fn drawn(frame: &Bounds) -> bool {
    frame.width > 0.0 && frame.height > 0.0
}

/// Where a click on an element lands: the middle of its frame, or for a
/// checkbox or radio button with a label beside it (a frame much wider than
/// tall) the middle of its box, at the leading end of a frame that may also
/// span a long empty stretch after a short label. A tab drawn as a radio
/// button is clicked in its middle.
pub fn click_point(role: Option<&str>, frame: &Bounds) -> (f64, f64) {
    match role {
        Some("AXCheckBox" | "AXRadioButton") if frame.width > 2.0 * frame.height => (
            (frame.x + frame.height / 2.0).round(),
            (frame.y + frame.height / 2.0).round(),
        ),
        _ => frame.center(),
    }
}

/// The menu bar where it shows the point, which is the Window Server's
/// window rather than the app's.
fn menu_bar_window(windows: &[Window], x: f64, y: f64) -> Option<i64> {
    windows
        .iter()
        .find(|window| window.is_menu_bar() && window.alpha > 0.0 && window.bounds.contains(x, y))
        .map(|window| window.number)
}

/// What a click on `element` at `(x, y)` must reach, given the app `expect`
/// names and a window list read after the scan: the element's own window,
/// or the app alone for a menu bar title or an open menu's item.
///
/// # Errors
///
/// Refuses an element whose menu bar is gone or whose window cannot be
/// told uniquely from `windows`.
pub fn element_check(
    element: &Element,
    windows: &[Window],
    expect: Expect,
    (x, y): (f64, f64),
) -> Result<Expect> {
    let pid = expect.pid();
    if element.role.as_deref() == Some("AXMenuBarItem") {
        menu_bar_window(windows, x, y)
            .ok_or_else(|| Error::new(Code::Elements, "The menu bar is no longer on screen"))?;
        return Ok(expect);
    }

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
    if element.menu
        || windows
            .iter()
            .any(|window| window.number == number && window.layer == MENU_LAYER)
    {
        Ok(expect)
    } else {
        expect.in_window(number)
    }
}

#[cfg(test)]
mod tests;

//! What a scan reports: the elements it read with their frames, the
//! snapshot that holds them, and the bounds a scan keeps to.
pub use super::geometry::Frame;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, time::Duration};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Element {
    /// Position in depth-first order, stable within one snapshot.
    pub index: usize,
    pub depth: usize,
    /// Frame of the top-level window (or open menu) this element belongs to.
    /// Like the attributes below, it is left out when the scan did not read it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window: Option<Frame>,
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
    pub frame: Option<Frame>,
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

/// Why a scan is incomplete.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
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
    pub(crate) const ALL: [Stop; 5] = [
        Stop::Depth,
        Stop::Count,
        Stop::Time,
        Stop::Unread,
        Stop::Withheld,
    ];

    /// The word a reply names it by.
    pub(crate) fn word(self) -> &'static str {
        match self {
            Stop::Depth => "depth",
            Stop::Count => "count",
            Stop::Time => "time",
            Stop::Unread => "unread",
            Stop::Withheld => "withheld",
        }
    }

    /// The reason a reply names by `word`, if it is one of these.
    pub(crate) fn parse(word: &str) -> Option<Stop> {
        Stop::ALL.into_iter().find(|stop| stop.word() == word)
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
pub(crate) fn why(stops: impl IntoIterator<Item = Stop>) -> Option<String> {
    let said: Vec<&str> = stops.into_iter().map(Stop::said).collect();

    match said.as_slice() {
        [] => None,
        [one] => Some((*one).to_owned()),
        [rest @ .., last] => Some(format!("{} and {last}", rest.join(", "))),
    }
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

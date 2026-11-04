//! Whether one scan of an app's elements shows the state `atlas wait`
//! waits for: an element there, gone, or the one element a named click
//! would pick enabled, on screen or holding a value. Only what was read
//! counts: a scan that could not see everything never proves absence or
//! that one element is the only match, and
//! an attribute it did not read, or read only in part, never proves a state.
use crate::screen::{
    elements::{Element, drawn},
    geometry::Bounds,
    select::{Choice, ElementFilter, choose},
};
use serde::Serialize;

/// The state an element wait waits for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum State {
    /// At least one element matches.
    Exists,
    /// No element matches, as far as a complete scan can tell.
    Gone,
    /// The one element a named click would pick says it is enabled.
    Enabled,
    /// That element is drawn inside its window and the display. It may
    /// still be covered: a click checks that when it is sent.
    Visible,
    /// That element's value is exactly this text.
    Value(String),
}

impl State {
    /// What a wait that timed out says it waited for in vain.
    pub fn missed(&self) -> &'static str {
        match self {
            Self::Exists => "The element did not appear in time",
            Self::Gone => "The element did not go in time",
            Self::Enabled => "The element was not enabled in time",
            Self::Visible => "The element was not on screen in time",
            Self::Value(_) => "The element did not hold the value in time",
        }
    }

    /// The state's word, as replies name it.
    pub fn word(&self) -> &'static str {
        match self {
            Self::Exists => "exists",
            Self::Gone => "gone",
            Self::Enabled => "enabled",
            Self::Visible => "visible",
            Self::Value(_) => "value",
        }
    }
}

/// What one scan shows of a wait's state.
#[derive(Debug, PartialEq, Eq)]
pub enum Judged {
    /// It holds, about the element at this index when there is one.
    Met(Option<usize>),
    /// It does not hold yet, for this reason.
    Unmet(Why),
}

/// Why a scan does not show the state, as a timed-out wait says.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Why {
    /// No element matches.
    None,
    /// More than one drawn element matches equally well.
    Several,
    /// One matches, but another whose name was not read whole might too.
    Doubtful,
    /// An element still matches.
    Present,
    /// The scan stopped early, so what it missed might match.
    Incomplete,
    /// An element whose name was not read whole might still match.
    Unread,
    /// The element says it is disabled.
    Disabled,
    /// The element did not say whether it is enabled.
    UnknownEnabled,
    /// The element is not drawn inside its window and the display.
    Hidden,
    /// The element holds another value, or none.
    OtherValue,
    /// The element's value was read only in part.
    CutValue,
    /// The element's value could not be read.
    UnreadValue,
}

impl Why {
    /// What a timed-out wait says the last look saw.
    pub fn says(self) -> &'static str {
        match self {
            Self::None => "no element matched",
            Self::Several => "more than one element matched",
            Self::Doubtful => "another element's name could not be read whole",
            Self::Present => "a matching element was still there",
            Self::Incomplete => "the scan was incomplete",
            Self::Unread => "an element's name could not be read whole",
            Self::Disabled => "the element was disabled",
            Self::UnknownEnabled => "the element did not say whether it was enabled",
            Self::Hidden => "the element was not on screen",
            Self::OtherValue => "the element held another value",
            Self::CutValue => "the element's value was too long to compare",
            Self::UnreadValue => "the element's value could not be read",
        }
    }
}

/// Whether `elements`, one scan of an app that was `incomplete` when it
/// stopped early, shows `state` for what `filter` picks, on `display`.
pub fn judge(
    elements: &[Element],
    incomplete: bool,
    filter: &ElementFilter,
    state: &State,
    display: &Bounds,
) -> Judged {
    match state {
        State::Exists => exists(elements, filter),
        State::Gone => gone(elements, incomplete, filter),
        State::Enabled | State::Visible | State::Value(_) => {
            // What an incomplete scan missed might be a second match.
            if incomplete {
                return Judged::Unmet(Why::Incomplete);
            }
            let element = match choose(elements, filter) {
                Choice::One(element) => element,
                Choice::NotOne { matching, .. } if matching.is_empty() => {
                    return Judged::Unmet(Why::None);
                }
                Choice::NotOne { .. } => return Judged::Unmet(Why::Several),
                Choice::Doubtful(_) => return Judged::Unmet(Why::Doubtful),
            };
            match holds(element, state, display) {
                Ok(()) => Judged::Met(Some(element.index)),
                Err(why) => Judged::Unmet(why),
            }
        }
    }
}

/// At least one element matches; the one a named click would pick when
/// there is one.
fn exists(elements: &[Element], filter: &ElementFilter) -> Judged {
    if !elements.iter().any(|element| filter.matches(element)) {
        return Judged::Unmet(Why::None);
    }
    match choose(elements, filter) {
        Choice::One(element) => Judged::Met(Some(element.index)),
        Choice::NotOne { .. } | Choice::Doubtful(_) => Judged::Met(None),
    }
}

/// A complete scan in which nothing matches, and nothing might.
fn gone(elements: &[Element], incomplete: bool, filter: &ElementFilter) -> Judged {
    if elements.iter().any(|element| filter.matches(element)) {
        return Judged::Unmet(Why::Present);
    }
    if incomplete {
        return Judged::Unmet(Why::Incomplete);
    }
    if elements.iter().any(|element| filter.might_match(element)) {
        return Judged::Unmet(Why::Unread);
    }
    Judged::Met(None)
}

/// Whether the chosen `element` holds `state`.
fn holds(element: &Element, state: &State, display: &Bounds) -> Result<(), Why> {
    match state {
        State::Enabled => match element.enabled {
            Some(true) => Ok(()),
            Some(false) => Err(Why::Disabled),
            None => Err(Why::UnknownEnabled),
        },
        State::Visible => {
            let shown = element.frame.as_ref().is_some_and(|frame| {
                drawn(frame)
                    && frame.intersects(display)
                    && element
                        .window
                        .as_ref()
                        .is_some_and(|window| frame.intersects(window))
            });
            if shown { Ok(()) } else { Err(Why::Hidden) }
        }
        State::Value(text) => {
            let said = |list: &[String]| list.iter().any(|attribute| attribute == "AXValue");
            if said(&element.unreadable) {
                return Err(Why::UnreadValue);
            }
            if said(&element.truncated) {
                return Err(Why::CutValue);
            }
            if element.value.as_deref() == Some(text.as_str()) {
                Ok(())
            } else {
                Err(Why::OtherValue)
            }
        }
        State::Exists | State::Gone => Ok(()),
    }
}

#[cfg(test)]
mod tests;

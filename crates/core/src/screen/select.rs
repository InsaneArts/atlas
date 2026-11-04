//! Which accessibility elements a name or role picks out, as `atlas
//! elements --name` matches them: shared by the commands that list and click
//! elements and by the helper's check of the focused field. A name that
//! picks out nothing is judged by how close it comes to the names there are.
use crate::{
    name::same_text,
    screen::elements::{Element, drawn},
};

/// Which accessibility elements a command is about.
#[derive(Debug, Default, Clone)]
pub struct ElementFilter {
    /// AX role, for example `AXButton`, compared case-insensitively.
    pub role: Option<String>,
    /// Exact title, label text, description or identifier, compared
    /// case-insensitively. A label that joins several parts ("QA Eject,
    /// Option-Space, 6 Slices") is also named by its first part.
    pub name: Option<String>,
}

/// Whether role `own` is the role `asked` for: the same, whatever its
/// case, or the same once `asked` is given the `AX` its name leaves out
/// (`checkbox` for `AXCheckBox`).
fn same_role(own: &str, asked: &str) -> bool {
    own.eq_ignore_ascii_case(asked)
        || own
            .get(..2)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("AX"))
            && own[2..].eq_ignore_ascii_case(asked)
}

/// How an element's name matched a `--name`: only as the first part of a
/// label that joins several, or whole. A whole match is the better one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum NameMatch {
    Leading,
    Exact,
}

/// How `text` matches `name`: whole, or as the first of its ", " parts.
fn name_match(text: &str, name: &str) -> Option<NameMatch> {
    if same_text(text, name) {
        return Some(NameMatch::Exact);
    }

    let first = text.split_once(", ").map(|(first, _)| first)?;
    same_text(first, name).then_some(NameMatch::Leading)
}

impl ElementFilter {
    /// How `element` matches, treating the attributes in `unknown` as
    /// matching (they could not be read). Of an attribute shown only in
    /// part, only its whole first part is judged. An identifier is never a
    /// label, so it only matches whole.
    fn compare(&self, element: &Element, unknown: &dyn Fn(&str) -> bool) -> Option<NameMatch> {
        let cut = |attribute: &str| element.truncated.iter().any(|a| a == attribute);
        let role = self.role.as_deref().is_none_or(|role| {
            unknown("AXRole")
                || element
                    .role
                    .as_deref()
                    .is_some_and(|own| same_role(own, role))
        });
        if !role {
            return None;
        }

        let Some(name) = self.name.as_deref() else {
            return Some(NameMatch::Exact);
        };

        let text = |value: Option<&str>, attribute: &str| {
            if cut(attribute) {
                return cut_match(value, name, unknown(attribute));
            }
            if unknown(attribute) {
                return Some(NameMatch::Exact);
            }
            value.and_then(|text| name_match(text, name))
        };

        let identifier = if cut("AXIdentifier") {
            // What is shown of a cut identifier can still rule a name out.
            (unknown("AXIdentifier")
                && element
                    .identifier
                    .as_deref()
                    .is_some_and(|text| could_begin(text, name)))
            .then_some(NameMatch::Exact)
        } else if unknown("AXIdentifier") {
            Some(NameMatch::Exact)
        } else {
            element
                .identifier
                .as_deref()
                .filter(|identifier| same_text(identifier, name))
                .map(|_| NameMatch::Exact)
        };

        let label = if !cut("AXValue") && unknown("AXValue") {
            // A static text whose value failed to read may be the label.
            (element.role.as_deref() == Some("AXStaticText") || unknown("AXRole"))
                .then_some(NameMatch::Exact)
        } else {
            text(label(element), "AXValue")
        };

        [
            text(element.title.as_deref(), "AXTitle"),
            text(element.description.as_deref(), "AXDescription"),
            identifier,
            label,
        ]
        .into_iter()
        .flatten()
        .max()
    }

    /// How `element` matches, if it does.
    pub fn match_of(&self, element: &Element) -> Option<NameMatch> {
        self.compare(element, &|_| false)
    }

    pub fn matches(&self, element: &Element) -> bool {
        self.match_of(element).is_some()
    }

    /// Whether `element` could match once attributes it failed to read, or
    /// read only in part, are treated as unknown.
    pub fn might_match(&self, element: &Element) -> bool {
        let unknown = |attribute: &str| {
            element.unreadable.iter().any(|a| a == attribute)
                || element.truncated.iter().any(|a| a == attribute)
        };

        self.compare(element, &unknown).is_some()
    }
}

/// Which one element a name or role picks out of a scan, as a named click
/// picks it.
#[derive(Debug)]
pub enum Choice<'a> {
    /// The one drawn element that matches best.
    One(&'a Element),
    /// Not exactly one: the elements in play and those of them that match
    /// best and are drawn.
    NotOne {
        in_play: Vec<&'a Element>,
        matching: Vec<&'a Element>,
    },
    /// One matches, but another whose name could not be read whole might
    /// match too.
    Doubtful(&'a Element),
}

/// The element `filter` picks out of `elements`: of those in play (only an
/// open menu's items while one is open), the drawn ones matching best; a
/// menu bar title gives way to one window element of the same name unless
/// a role is asked for. One is chosen only when no other drawn element
/// whose name could not be read whole might match as well.
pub fn choose<'a>(elements: &'a [Element], filter: &ElementFilter) -> Choice<'a> {
    let in_play = in_play(elements);
    let matches: Vec<(&Element, NameMatch)> = in_play
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
        return Choice::NotOne { in_play, matching };
    };

    // An element whose selector attributes could not be read may be a
    // second, unseen match, unless it is collapsed and could never be chosen.
    let doubtful = in_play.iter().any(|other| {
        other.index != element.index
            && (!other.unreadable.is_empty() || !other.truncated.is_empty())
            && other.frame.as_ref().is_none_or(drawn)
            && filter.might_match(other)
    });
    if doubtful {
        Choice::Doubtful(element)
    } else {
        Choice::One(element)
    }
}

/// The elements a name is judged against: while a menu is open, only its
/// items (a click outside it just closes the menu).
pub fn in_play(elements: &[Element]) -> Vec<&Element> {
    let menu_open = elements.iter().any(|element| element.menu);
    elements
        .iter()
        .filter(|element| element.menu || !menu_open)
        .collect()
}

/// The text of a plain label (SwiftUI and AppKit put it in the value of a
/// static text), which names it like a title. A text field's value is typed
/// content, never a name.
fn label(element: &Element) -> Option<&str> {
    (element.role.as_deref() == Some("AXStaticText"))
        .then_some(element.value.as_deref())
        .flatten()
}

/// Every name an element has: its title, description, label text and
/// identifier, in that order.
pub fn names(element: &Element) -> impl Iterator<Item = &str> {
    [
        element.title.as_deref(),
        element.description.as_deref(),
        label(element),
        element.identifier.as_deref(),
    ]
    .into_iter()
    .flatten()
}

/// What an element is called: the first of its names that is not empty.
pub fn shown_name(element: &Element) -> Option<&str> {
    names(element).find(|name| !name.is_empty())
}

/// How text shown only in part matches `name`. Its first ", " part is
/// whole, so it is judged like any other; the rest proves nothing, unless
/// `unknown` lets a possible match count.
fn cut_match(shown: Option<&str>, name: &str, unknown: bool) -> Option<NameMatch> {
    let shown = shown?;
    if let Some((first, _)) = shown.split_once(", ")
        && same_text(first, name)
    {
        return Some(NameMatch::Leading);
    }

    (unknown && could_begin(shown, name)).then_some(NameMatch::Exact)
}

/// Whether `text`, which is only the start of a longer value, could begin
/// a value named `name`: the part shown must begin the name.
fn could_begin(text: &str, name: &str) -> bool {
    let mut name = name.chars().flat_map(char::to_lowercase);
    text.chars()
        .flat_map(char::to_lowercase)
        .all(|c| name.next() == Some(c))
}

/// How close `text` is to `query` (already lowercased), if at all: the same
/// text under another role (0), one holding the other (1), or a few
/// characters off (2), with how far off it is.
pub fn closeness(query: &str, text: &str) -> Option<(u8, usize)> {
    let text: String = text.chars().flat_map(char::to_lowercase).collect();
    let (wanted, length) = (query.chars().count(), text.chars().count());
    let apart = wanted.abs_diff(length);

    if text.is_empty() {
        return None;
    }

    if text == query {
        return Some((0, 0));
    }

    if wanted.min(length) >= 2 && (text.contains(query) || query.contains(text.as_str())) {
        return Some((1, apart));
    }

    // A short query is a few characters off most short names ("OK" from
    // "No" and "Go"), so only a longer one is matched that way.
    if wanted < 4 {
        return None;
    }

    let limit = (wanted / 3).max(1);
    if apart > limit {
        return None;
    }

    let distance = edits(query, &text);
    (distance <= limit).then_some((2, distance))
}

/// How many characters must be added, removed or changed to turn one text
/// into the other.
fn edits(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();

    for (i, x) in a.chars().enumerate() {
        let mut diagonal = row[0];
        row[0] = i + 1;
        for (j, y) in b.iter().enumerate() {
            let above = row[j + 1];
            row[j + 1] = (above + 1)
                .min(row[j] + 1)
                .min(diagonal + usize::from(x != *y));
            diagonal = above;
        }
    }

    row[b.len()]
}

#[cfg(test)]
mod tests;

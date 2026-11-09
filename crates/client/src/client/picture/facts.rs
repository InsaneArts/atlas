//! What Atlas.app needs to mark an action's picture, kept beside it: what
//! the command's first input did and where, read from its request, and how
//! the command went, read from each of its rows. Only Atlas.app reads it;
//! the activity log an agent reads stays as it is.
use crate::{
    activity_log::{Failure, Row},
    before::Typed,
    doing::{captioned, chords},
};
use atlas_core::{
    actions::Action,
    error::Code,
    screen::{elements::Element, geometry::Bounds, select::shown_name},
    session::InputEpoch,
};
use serde::Deserialize;
use serde_json::{Map, Value, json};

/// A password field's role, as the input service reports it.
const SECURE: &str = "AXSecureTextField";

/// What the input `actions` send does, and where: its kind, the points it
/// acts at in display points, and the display of `epoch` those points are on.
pub(in crate::client) fn of_input(actions: &[Action], epoch: &InputEpoch) -> Map<String, Value> {
    let point = |x: &f64, y: &f64| json!({"x": x, "y": y});
    let fields = match actions.first() {
        None => return Map::new(),
        Some(
            Action::Click {
                x,
                y,
                button,
                count,
                ..
            }
            | Action::Pointer {
                x,
                y,
                button,
                count,
                ..
            },
        ) => json!({"action": "click", "point": point(x, y), "clicks": count, "button": button}),
        // A click on a referenced element: where is the element's own,
        // which the input service's reply says.
        Some(Action::Reference { button, count, .. }) => {
            json!({"action": "click", "clicks": count, "button": button})
        }
        Some(Action::Move { x, y }) => json!({"action": "move", "point": point(x, y)}),
        Some(Action::Scroll { x, y, .. }) => {
            // A batch of scrolls at one point reads as one scroll by all of them.
            let (dx, dy) = actions
                .iter()
                .fold((0_i64, 0_i64), |(dx, dy), action| match action {
                    Action::Scroll { dx: x, dy: y, .. } => (dx + i64::from(*x), dy + i64::from(*y)),
                    _ => (dx, dy),
                });
            json!({"action": "scroll", "point": point(x, y), "scroll": {"dx": dx, "dy": dy}})
        }
        Some(Action::Drag {
            from_x,
            from_y,
            to_x,
            to_y,
            button,
        }) => json!({"action": "drag", "point": point(from_x, from_y), "to": point(to_x, to_y),
            "button": button}),
        Some(Action::Type { .. }) => {
            let text: String = actions
                .iter()
                .filter_map(|action| match action {
                    Action::Type { text } => Some(text.as_str()),
                    _ => None,
                })
                .collect();
            json!({"action": "type", "text": text})
        }
        // Its keys come from its row's label once it ends, which names only
        // keys that type no text: a letter pressed may be a password's.
        Some(Action::Key { .. }) => json!({"action": "key"}),
        Some(
            Action::Press { .. }
            | Action::SetValue { .. }
            | Action::Check { .. }
            | Action::Uncheck { .. }
            | Action::Pick { .. }
            | Action::Expand { .. }
            | Action::Collapse { .. }
            | Action::ScrollIntoView { .. },
        ) => json!({"action": "press"}),
        // A pause first does nothing to mark.
        Some(Action::Wait { .. }) => json!({}),
    };
    let mut facts = Map::new();
    if let Value::Object(fields) = fields {
        facts.extend(fields);
    }
    facts.insert(
        "display".into(),
        json!({"width": epoch.width, "height": epoch.height}),
    );
    facts
}

/// What a command found to act on before its first input: `element`, when
/// it has a frame. A window's title is never kept: it may be a document's
/// name.
pub(in crate::client) fn found(element: &Element) -> Map<String, Value> {
    let mut found = Map::new();
    if let Some(frame) = &element.frame {
        found.insert(
            "element".into(),
            reached(frame, element.role.as_deref(), shown_name(element)),
        );
    }
    found
}

/// An element an input acts on: its `frame` in display points, its `role`,
/// and its `name` only for a control named by its caption, as any other
/// element's name may be a document's text.
fn reached(frame: &Bounds, role: Option<&str>, name: Option<&str>) -> Value {
    let mut element = json!({"frame": frame});
    if let Some(role) = role {
        element["role"] = json!(role);
    }
    if let Some(name) = name.filter(|_| captioned(role)) {
        element["name"] = json!(name);
    }
    element
}

/// The element the input service says the input reached, in `reply`, when
/// it says where.
fn replied(reply: &Value) -> Option<Value> {
    let element = &reply["element"];
    let frame = Bounds::deserialize(&element["frame"]).ok()?;
    Some(reached(
        &frame,
        element["role"].as_str(),
        element["name"].as_str(),
    ))
}

/// The facts of a command's picture once `row` says how the command went:
/// `input`'s facts with its outcome and why, the control or field it named,
/// the element `reply` says it reached when the command found none itself,
/// its keys as keycaps, when its picture was taken and when it ended (now),
/// in milliseconds since 1970, and what it `typed`. Typed text is kept
/// whole, or its first `limit` characters, with how many there were, only
/// when the helper found every input that typed it going into the same
/// field, no password field, and none of it may have moved on from that
/// field: for a password field, one it could not tell, or text that may
/// have left it, neither is kept.
pub(in crate::client) fn finished(
    mut facts: Map<String, Value>,
    row: &Row,
    reply: &Value,
    typed: Typed,
    limit: Option<u32>,
    taken: u128,
) -> Value {
    facts.insert("outcome".into(), json!(outcome(row)));
    if let Some(reason) = crate::overview::reason(row) {
        facts.insert("reason".into(), json!(reason));
    }
    if let Some(label) = &row.label {
        facts.insert("label".into(), json!(label));
        if row.verb.as_deref() == Some("key") {
            facts.insert("keys".into(), json!(chords(label)));
        }
    }
    if !facts.contains_key("element")
        && let Some(element) = replied(reply)
    {
        facts.insert("element".into(), element);
    }
    facts.insert(
        "at".into(),
        json!(u64::try_from(taken / 1_000_000).unwrap_or(u64::MAX)),
    );
    facts.insert("endedAt".into(), json!(atlas_core::time::now_ms()));

    if facts.get("action") == Some(&json!("type")) {
        let batch = facts
            .remove("text")
            .and_then(|text| text.as_str().map(str::to_owned));
        // Text is kept only for a field the helper said is no password, and
        // only while none of it may have moved on: a field it could not tell,
        // or the next one, may be one.
        let role = typed_into(&typed.into);
        if role == Some(SECURE) {
            facts.insert("secure".into(), json!(true));
        } else if let Some(text) = typed
            .text
            .or(batch)
            .filter(|text| role.is_some() && !may_leave_its_field(text))
        {
            let length = text.chars().count();
            let kept: String = match limit {
                Some(limit) => text.chars().take(limit as usize).collect(),
                None => text,
            };
            facts.insert("text".into(), json!(kept));
            facts.insert("length".into(), json!(length));
        }
    }
    Value::Object(facts)
}

/// Whether typing `text` may move the keyboard focus out of the field the
/// helper found before it was sent, perhaps into a password field: a tab, a
/// return or another control character, or a line break, may.
fn may_leave_its_field(text: &str) -> bool {
    text.chars()
        .any(|character| character.is_control() || matches!(character, '\u{2028}' | '\u{2029}'))
}

/// The field a command's text went into, from the role the helper found
/// for each of its inputs that typed: a password field when any was one,
/// and otherwise the role every one of them had alike. None when any had
/// none, or they disagree.
fn typed_into(into: &[Option<String>]) -> Option<&str> {
    if into.iter().any(|role| role.as_deref() == Some(SECURE)) {
        return Some(SECURE);
    }
    let first = into.first()?.as_deref();
    into.iter()
        .all(|role| role.as_deref() == first)
        .then_some(first)?
}

/// How a command went, as Atlas.app marks it: it worked, nothing was sent
/// (another app was in front before any of it), its outcome is unknown, or
/// it failed.
fn outcome(row: &Row) -> &'static str {
    match &row.code {
        _ if row.worked() => "worked",
        Some(Failure::Code(Code::FocusChanged)) if row.partial != Some(true) => "notSent",
        Some(Failure::Code(Code::Uncertain)) => "uncertain",
        _ => "failed",
    }
}

#[cfg(test)]
#[path = "facts_tests.rs"]
mod tests;

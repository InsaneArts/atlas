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
    error::Code,
    screen::{elements::Element, geometry::Bounds, select::shown_name},
};
use serde::Deserialize;
use serde_json::{Map, Value, json};

/// A password field's role, as the input service reports it.
const SECURE: &str = "AXSecureTextField";

/// What the input `request` sends does, and where: its kind, the points it
/// acts at in display points, and the display those points are on.
pub(in crate::client) fn of_input(request: &Value) -> Map<String, Value> {
    let actions = request["actions"].as_array().map_or(&[][..], Vec::as_slice);
    let mut facts = Map::new();
    let point = |action: &Value, x: &str, y: &str| json!({"x": action[x], "y": action[y]});
    let Some(first) = actions.first() else {
        return facts;
    };
    let (kind, fields) = match first["type"].as_str().unwrap_or_default() {
        "click" | "pointer" => (
            "click",
            json!({"point": point(first, "x", "y"), "clicks": first["count"],
                "button": first["button"]}),
        ),
        // A click on a referenced element: where is the element's own,
        // which the input service's reply says.
        "reference" => (
            "click",
            json!({"clicks": first["count"], "button": first["button"]}),
        ),
        "move" => ("move", json!({"point": point(first, "x", "y")})),
        "scroll" => {
            // A batch of scrolls at one point reads as one scroll by all of them.
            let sum = |axis: &str| {
                actions
                    .iter()
                    .filter_map(|action| action[axis].as_i64())
                    .sum::<i64>()
            };
            (
                "scroll",
                json!({"point": point(first, "x", "y"),
                    "scroll": {"dx": sum("dx"), "dy": sum("dy")}}),
            )
        }
        "drag" => (
            "drag",
            json!({"point": point(first, "fromX", "fromY"), "to": point(first, "toX", "toY"),
                "button": first["button"]}),
        ),
        "type" => {
            let text: String = actions
                .iter()
                .filter_map(|action| action["text"].as_str())
                .collect();
            ("type", json!({"text": text}))
        }
        // Its keys come from its row's label once it ends, which names only
        // keys that type no text: a letter pressed may be a password's.
        "key" => ("key", json!({})),
        _ => ("press", json!({})),
    };
    facts.insert("action".into(), json!(kind));
    if let Value::Object(fields) = fields {
        facts.extend(fields.into_iter().filter(|(_, value)| !value.is_null()));
    }
    let epoch = &request["epoch"];
    facts.insert(
        "display".into(),
        json!({"width": epoch["width"], "height": epoch["height"]}),
    );
    facts
}

/// What a command found to act on before its first input: `element`, when
/// it has a frame, and the title of the `window` it is in, when it has one.
pub(in crate::client) fn found(element: &Element, window: Option<&str>) -> Map<String, Value> {
    let mut found = Map::new();
    if let Some(frame) = &element.frame {
        found.insert(
            "element".into(),
            reached(frame, element.role.as_deref(), shown_name(element)),
        );
    }
    if let Some(title) = window.filter(|title| !title.is_empty()) {
        found.insert("window".into(), json!(title));
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
/// field, no password field: for a password field, or one it could not
/// tell, neither is kept.
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
        // Text is kept only for a field the helper said is no password; one
        // it could not tell may be one.
        let role = typed_into(&typed.into);
        if role == Some(SECURE) {
            facts.insert("secure".into(), json!(true));
        } else if let Some(text) = typed.text.or(batch).filter(|_| role.is_some()) {
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
mod tests;

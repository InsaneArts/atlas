//! The input agents send: clicks, drags and keys as one batch each, and
//! text typed batch by batch in small parts, with a report of exactly how
//! much was confirmed typed.
use super::expect::Expected;
use crate::{
    actions::Button,
    client::{Check, Client},
    error::{Code, Error, Holder, Result},
    interrupt::Interrupt,
};
use serde_json::{Value, json};
use std::time::Instant;
use unicode_segmentation::UnicodeSegmentation;

/// How much text `atlas type` sends at once: 64 characters, about half a
/// second of typing, so a busy Mac typing several times slower than usual
/// still finishes each part well within the helper's deadline. Control is
/// held across the parts, so nothing lands between them.
pub(super) const TYPE_BATCH: usize = 64;

/// How much text one `type` action holds: 16 characters. With a target the
/// helper checks focus before each action, so at most this much can follow
/// a focus change before Atlas sees it.
pub(super) const TYPE_PART: usize = 16;

pub fn click(x: f64, y: f64, which: Button, count: u8, modifiers: &[String]) -> Value {
    json!([{
        "type": "click",
        "x": x,
        "y": y,
        "button": which,
        "count": count,
        "modifiers": modifiers,
    }])
}

pub fn drag(from: (f64, f64), to: (f64, f64), which: Button) -> Value {
    json!([{
        "type": "drag",
        "fromX": from.0,
        "fromY": from.1,
        "toX": to.0,
        "toY": to.1,
        "button": which,
    }])
}

/// Move the pointer to `x`, `y`.
pub fn move_to(x: f64, y: f64) -> Value {
    json!([{ "type": "move", "x": x, "y": y }])
}

/// Scroll at `at` by `by` lines: down and right are positive.
pub fn scroll(at: (f64, f64), by: (i32, i32)) -> Value {
    json!([{ "type": "scroll", "x": at.0, "y": at.1, "dx": by.0, "dy": by.1 }])
}

/// Text in pieces of at most `limit` characters, never splitting a
/// grapheme cluster (an emoji sequence or a letter with combining marks); a
/// longer cluster goes on its own.
fn split_text(text: &str, limit: usize) -> Vec<String> {
    let mut pieces = Vec::new();
    let mut piece = String::new();
    let mut length = 0;

    for grapheme in text.graphemes(true) {
        let size = grapheme.chars().count();
        if length > 0 && length + size > limit {
            pieces.push(std::mem::take(&mut piece));
            length = 0;
        }

        piece.push_str(grapheme);
        length += size;
    }

    if !piece.is_empty() {
        pieces.push(piece);
    }

    pieces
}

/// Text as `type` batches of at most [`TYPE_BATCH`] characters, never
/// splitting a grapheme cluster; a longer cluster goes on its own, up to
/// [`TEXT_MAX`](crate::actions::TEXT_MAX).
pub(super) fn type_batches(text: &str) -> Result<Vec<String>> {
    if text
        .graphemes(true)
        .any(|grapheme| grapheme.chars().count() > crate::actions::TEXT_MAX)
    {
        return Err(Error::new(
            Code::Input,
            "A single character cluster is too long to type",
        ));
    }

    let batches = split_text(text, TYPE_BATCH);
    if batches.is_empty() {
        return Err(Error::new(Code::Input, "Provide text to type"));
    }

    Ok(batches)
}

/// A batch as `type` actions of at most [`TYPE_PART`] characters, never
/// splitting a grapheme cluster; a longer cluster goes on its own.
pub(super) fn type_parts(batch: &str) -> Vec<String> {
    split_text(batch, TYPE_PART)
}

/// Type `text` batch by batch, stopping at the first batch that is not
/// confirmed. Each batch goes as `type` actions of [`TYPE_PART`]
/// characters, and with an expectation each is sent only while the app,
/// and the window and field when named, have keyboard focus. The report
/// says exactly how much was confirmed typed and, on failure, whether the
/// failed batch's delivery is uncertain. The whole text is typed under one
/// ownership into the helper as it was at the start: no other input lands
/// between batches, and a helper that restarts midway is refused rather
/// than typed into. A signal's stop ends it before the next batch, with
/// the same report.
pub async fn type_text(client: &Client, text: &str, expected: Option<&Expected>) -> Result<Value> {
    let batches = type_batches(text)?;

    let ownership = client.controller.acquire_beside_commands()?;
    // A signal's stop waits for the report of how much was typed.
    let interrupt = client.controller.interrupt();
    let _typing = interrupt.step();
    let verified = client.verify().await?;
    let check = match expected {
        Some(expected) => Some(Check::Required(expected.resolve(client, &verified).await?)),
        None => None,
    };
    let epoch = verified.session().epoch.clone();

    let send = async |actions: &Value| {
        client
            .input_owned(&ownership, actions, &epoch, check.clone())
            .await
    };
    Ok(type_recorded(client, &batches, interrupt, send).await)
}

/// The terminal command row must describe all batches, including confirmed
/// text before a later refusal. The reader replaces the individual batch rows
/// with this one. Only action counts and the reply's safe projection are kept.
pub(super) async fn type_recorded(
    client: &Client,
    batches: &[String],
    interrupt: &Interrupt,
    mut send: impl AsyncFnMut(&Value) -> Result<Value>,
) -> Value {
    if client.doing.is_none() {
        return type_each(batches, interrupt, send).await;
    }
    let started = Instant::now();
    let mut identity = json!({});
    let mut report = type_each(batches, interrupt, async |actions: &Value| {
        let outcome = send(actions).await;
        if let Ok(reply) = &outcome {
            if !reply["app"].is_null() {
                identity["app"] = reply["app"].clone();
                identity["bundleId"] = reply["bundleId"].clone();
            }
            if !reply["field"].is_null() {
                identity["field"] = reply["field"].clone();
            }
        }
        outcome
    })
    .await;

    let mut reply = report.clone();
    if report["uncertain"] == true {
        reply["code"] = json!(Code::Uncertain);
    }
    for field in ["app", "bundleId", "field"] {
        if let Some(value) = identity.get(field) {
            reply[field] = value.clone();
        }
    }
    // Count the requested input actions without copying any typed text into
    // the recording request. This request is never sent to the helper.
    let actions: Vec<Value> = batches
        .iter()
        .flat_map(|batch| type_parts(batch))
        .map(|_| json!({"type": "type"}))
        .collect();
    if client
        .record(
            &json!({"op": "input", "actions": actions}),
            &reply,
            started.elapsed(),
        )
        .await
        .is_err()
    {
        report["warnings"] =
            json!(["Operation result is valid; local metadata could not be recorded"]);
    }
    report
}

/// Send each of `batches` as `type` actions through `send`, stopping at
/// the first that is not confirmed, or before the next once `interrupt`
/// was asked to stop: the report says how much was confirmed typed.
pub(super) async fn type_each(
    batches: &[String],
    interrupt: &Interrupt,
    mut send: impl AsyncFnMut(&Value) -> Result<Value>,
) -> Value {
    let (mut typed, mut confirmed) = (0, 0);

    for batch in batches {
        let parts = type_parts(batch);
        let actions: Vec<Value> = parts
            .iter()
            .map(|part| json!({ "type": "type", "text": part }))
            .collect();
        let outcome = match interrupt.check() {
            Ok(()) => send(&Value::Array(actions)).await,
            Err(stop) => Err(stop),
        };

        let failure = match outcome {
            Ok(reply) if reply["ok"] == true => {
                confirmed += 1;
                typed += batch.chars().count();
                continue;
            }
            Ok(reply) => Unconfirmed::from_reply(&reply),
            Err(error) => Unconfirmed::from(error),
        };

        return stopped_typing(typed, batches.len(), confirmed, &parts, failure);
    }

    json!({ "ok": true, "typed": typed, "batches": batches.len() })
}

/// A batch the helper did not confirm: why, how many of its actions were
/// performed, and whether the failing one had sent some of its events.
pub(super) struct Unconfirmed {
    error: Error,
    performed: usize,
    partial: bool,
}

impl Unconfirmed {
    /// A failure the helper replied with. Agents act on the code (busy,
    /// lease-lost, session-changed, focus-changed), on who holds control
    /// when busy and on what was found instead of the app, so those are
    /// kept; nothing else of the reply is.
    pub(super) fn from_reply(reply: &Value) -> Self {
        let code = reply["code"]
            .as_str()
            .and_then(crate::error::Code::parse)
            .unwrap_or(Code::Input);
        let message = reply["error"]
            .as_str()
            .unwrap_or("The helper did not confirm the text");
        let mut error = Error::new(code, message);
        if reply["uncertain"] == true {
            error = error.uncertain();
        }
        if let Some(holder) = reply["holder"].as_str().and_then(Holder::parse) {
            error = error.held_by(holder);
        }
        for kept in ["reason", "unread", "found"] {
            if !reply[kept].is_null() {
                error = error.with(kept, &reply[kept]);
            }
        }

        Self {
            error,
            performed: reply["performed"]
                .as_u64()
                .and_then(|performed| usize::try_from(performed).ok())
                .unwrap_or(0),
            partial: reply["partial"] == true,
        }
    }
}

impl From<Error> for Unconfirmed {
    /// A failure before or around the batch: none of it is known performed.
    fn from(error: Error) -> Self {
        Self {
            error,
            performed: 0,
            partial: false,
        }
    }
}

/// The report of a `type` that stopped at a batch: everything confirmed
/// before it, `typed` characters in `confirmed` of `batches`, plus the
/// `parts` of that batch the helper says it performed. `partial` says some
/// of the text was sent.
pub(super) fn stopped_typing(
    typed: usize,
    batches: usize,
    confirmed: usize,
    parts: &[String],
    failure: Unconfirmed,
) -> Value {
    let typed = typed
        + parts
            .iter()
            .take(failure.performed)
            .map(|part| part.chars().count())
            .sum::<usize>();

    let mut report = json!(failure.error);
    report["typed"] = json!(typed);
    report["batches"] = json!(batches);
    report["confirmedBatches"] = json!(confirmed);
    report["partial"] = json!(typed > 0 || failure.partial);

    report
}

/// Keys pressed in order, as one batch; an unknown key is named.
pub fn keys(keys: &[String]) -> Result<Value> {
    if let Some(unknown) = keys.iter().find(|key| !crate::actions::valid_key(key)) {
        return Err(Error::new(
            Code::Usage,
            format!(
                "{unknown} is not a key Atlas knows. Use a letter, digit or symbol, a name such \
                 as return, tab, escape, space, backspace, up or f5, with modifiers like cmd+s \
                 or cmd+shift+z."
            ),
        ));
    }

    Ok(Value::Array(
        keys.iter()
            .map(|key| json!({ "type": "key", "key": key }))
            .collect(),
    ))
}

//! The input agents send: clicks, drags and keys as one batch each, and
//! text typed batch by batch in small parts, with a report of exactly how
//! much was confirmed typed.
use super::expect::Expected;
use crate::client::Client;
use atlas_control::interrupt::Interrupt;
use atlas_core::{
    actions::Button,
    error::{Code, Error, Holder, Result},
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
/// helper prepares and checks focus before each part. At most this much can
/// follow a focus change before Atlas sees it.
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
/// [`TEXT_MAX`](atlas_core::actions::TEXT_MAX).
pub(super) fn type_batches(text: &str) -> Result<Vec<String>> {
    if text
        .graphemes(true)
        .any(|grapheme| grapheme.chars().count() > atlas_core::actions::TEXT_MAX)
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
///
/// # Errors
///
/// Rejects invalid text, unavailable control or a changed target. A later failure may
/// follow earlier text chunks; retain uncertainty and do not replay the text.
pub async fn type_text(client: &Client, text: &str, expected: Option<&Expected>) -> Result<Value> {
    let batches = type_batches(text)?;
    let ownership = client.controller.acquire_beside_commands()?;
    // A signal's stop waits for the report of how much was typed.
    let interrupt = client.controller.interrupt();
    let _typing = interrupt.step();
    let verified = client.verify().await?;
    let check = match expected {
        Some(expected) => Some(expected.resolve(client, &verified).await?),
        None => None,
    };
    let epoch = &verified.session().epoch;

    let send = async |actions: &Value| {
        client
            .input_owned(
                &ownership,
                atlas_core::actions::Batch::decode(actions),
                Some((epoch, Instant::now())),
                check.clone(),
            )
            .await
    };
    Ok(type_recorded(client, &batches, interrupt, send).await)
}

/// Send each of `batches` as `type` actions through `send`, stopping at
/// the first that is not confirmed, or before the next once `interrupt`
/// was asked to stop: the report says how much was confirmed typed. Of a
/// batch that stopped it, only the parts the helper says it performed
/// count, and `partial` says whether some of the text was sent.
///
/// With a command under way, its terminal row describes all batches,
/// including confirmed text before a later refusal. The reader replaces
/// the individual batch rows with this one. Only action counts and the
/// reply's safe projection are kept: the field only when every batch sent
/// named the same one. The whole text is noted for the facts beside the
/// command's picture just before that row, when every batch's row has
/// said which field it went into.
pub(super) async fn type_recorded(
    client: &Client,
    batches: &[String],
    interrupt: &Interrupt,
    mut send: impl AsyncFnMut(&Value) -> Result<Value>,
) -> Value {
    let started = Instant::now();
    let mut identity = json!({});
    // The field each batch sent went into, as its reply says.
    let mut fields = Vec::new();
    let (mut typed, mut confirmed) = (0, 0);
    let mut stopped = None;
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
        if let Ok(reply) = &outcome
            && !reply["app"].is_null()
        {
            identity["app"] = reply["app"].clone();
            identity["bundleId"] = reply["bundleId"].clone();
        }
        // A batch that failed, or whose reply was lost, may have typed some
        // of its text where nobody can tell.
        fields.push(match &outcome {
            Ok(reply) if reply["ok"] == true => reply["field"].clone(),
            Ok(_) | Err(_) => Value::Null,
        });

        let (error, performed, partial) = match outcome {
            Ok(reply) if reply["ok"] == true => {
                confirmed += 1;
                typed += batch.chars().count();
                continue;
            }
            Ok(reply) => (
                unconfirmed(&reply),
                reply["performed"]
                    .as_u64()
                    .and_then(|performed| usize::try_from(performed).ok())
                    .unwrap_or(0),
                reply["partial"] == true,
            ),
            Err(error) => (error, 0, false),
        };
        let typed = typed
            + parts
                .iter()
                .take(performed)
                .map(|part| part.chars().count())
                .sum::<usize>();
        let mut report = json!(error);
        report["typed"] = json!(typed);
        report["batches"] = json!(batches.len());
        report["confirmedBatches"] = json!(confirmed);
        report["partial"] = json!(typed > 0 || partial);
        stopped = Some(report);
        break;
    }
    let mut report =
        stopped.unwrap_or_else(|| json!({ "ok": true, "typed": typed, "batches": batches.len() }));
    if client.doing.is_none() {
        return report;
    }

    if let Some(first) = fields.first()
        && !first.is_null()
        && fields.iter().all(|field| field == first)
    {
        identity["field"] = first.clone();
    }
    let mut reply = report.clone();
    if report["uncertain"] == true {
        reply["code"] = json!(Code::Uncertain);
    }
    if let Some(doing) = &client.doing {
        doing.before().typing(&batches.concat());
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
        report["warnings"] = json!([crate::client::NOT_RECORDED]);
    }
    report
}

/// The failure a helper replied with to a batch it did not confirm. Agents
/// act on the code (busy, lease-lost, session-changed, focus-changed), on
/// who holds control when busy and on what was found instead of the app,
/// so those are kept; nothing else of the reply is.
fn unconfirmed(reply: &Value) -> Error {
    let mut error = Error::refused(
        Code::of_reply(reply, Code::Input),
        reply,
        "The helper did not confirm the text",
    );
    if let Some(holder) = reply["holder"].as_str().and_then(Holder::parse) {
        error = error.held_by(holder);
    }
    for kept in ["reason", "unread", "found"] {
        if !reply[kept].is_null() {
            error = error.with(kept, reply[kept].clone());
        }
    }
    error
}

/// Keys pressed in order, as one batch; an unknown key is named.
///
/// # Errors
///
/// Rejects empty or unsupported key combinations. This only constructs a batch and sends no
/// input.
pub fn keys(keys: &[String]) -> Result<Value> {
    if let Some(unknown) = keys.iter().find(|key| !atlas_core::actions::valid_key(key)) {
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

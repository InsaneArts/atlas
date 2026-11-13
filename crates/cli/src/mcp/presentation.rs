//! The model's status wording and the view's privacy projections.
use atlas_client::host_session::record;
use atlas_core::host::{Host, HostSession, Key};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

/// What a row names a session it came from that no list names: another
/// session, never the commands no host named.
const ANOTHER_SESSION: &str = "Another session";

/// A tool result: `text` for the model, `facts` for the view.
pub(crate) fn tool_result(text: &str, facts: Value) -> Value {
    json!({
        "content": [{"type": "text", "text": text}],
        "structuredContent": facts,
    })
}

/// Say where the person's clicks land: display points, as input takes.
pub(super) fn add_points(result: &mut Value, (width, height): (f64, f64)) {
    result["structuredContent"]["points"] = json!({"width": width, "height": height});
}

/// This chat's host session, only where Atlas is certain of it: a Claude
/// Code session whose host process started this server (`parent`) and
/// which Atlas has recorded, moved on to the newest session that process
/// recorded, as after `/clear`. While a session on its first command has
/// no record yet (`unrecorded`), it may be this chat's newest, so none is
/// named. Hosts that share one server among their chats name none.
pub(super) fn own_session(
    own: Option<&HostSession>,
    seen: &[record::Seen],
    parent: i32,
    unrecorded: bool,
) -> Option<Key> {
    let own = own.filter(|own| own.host() == Host::Claude)?;
    let process = own.process().filter(|process| process.pid == parent)?;
    let recorded = seen.iter().any(|seen| seen.record.key == *own.key());
    if !recorded || unrecorded {
        return None;
    }

    seen.iter()
        .map(|seen| &seen.record)
        .filter(|record| record.host_process == Some(process))
        .max_by_key(|record| record.last_seen)
        .map(|record| record.key.clone())
}

/// The name of the host session a shown row names by its key `session`,
/// when its record is there: never its key or the host's own id.
pub(super) fn session_name<'a>(
    session: &str,
    named: &'a BTreeMap<Key, String>,
) -> Option<&'a String> {
    named.get(&Key::parse(session)?)
}

/// The items of the array `value`, none when it is anything else.
fn items(value: &Value) -> &[Value] {
    value.as_array().map(Vec::as_slice).unwrap_or_default()
}

/// What the view and the model read of a status: the overview's state, its
/// words and recent rows, without the doctor report around it. Rows name
/// their session, by `named`, only when they come from two sessions or
/// more or the apps speak of sessions, so one agent's status costs it
/// nothing more. The sessions are told by name and state, only as much as
/// the view draws and never by the host's own id; `own` is this chat's,
/// when certain.
pub(super) fn summary(status: &Value, named: &BTreeMap<Key, String>, own: Option<&Key>) -> Value {
    let overview = &status["overview"];
    let shown = items(&overview["activity"]);
    let host_sessions = items(&overview["hostSessions"]);
    let sessions_shown = overview["sessionsShown"] == true;
    let sessions: BTreeSet<&str> = shown
        .iter()
        .filter_map(|row| row["session"].as_str())
        .collect();
    let names_rows = sessions.len() > 1 || sessions_shown;
    // The overview's own list names the sessions its rows come from,
    // those on their first command included; records name the rest.
    let listed_names: BTreeMap<&str, &str> = host_sessions
        .iter()
        .filter_map(|session| Some((session["key"].as_str()?, session["name"].as_str()?)))
        .collect();
    let rows = shown
        .iter()
        .map(|row| {
            let mut kept = json!({
                "says": row["says"],
                "ok": row["ok"],
                "actor": row["actor"],
                "ageSeconds": row["ageSeconds"],
                "nothingSent": row["nothingSent"] == true,
                "uncertain": row["uncertain"] == true,
            });
            if let Some(key) = row["session"].as_str().filter(|_| names_rows) {
                let name = listed_names
                    .get(key)
                    .copied()
                    .or_else(|| session_name(key, named).map(String::as_str))
                    .unwrap_or(ANOTHER_SESSION);
                kept["session"] = json!(name);
            }
            kept
        })
        .collect::<Vec<_>>();
    let listed = host_sessions
        .iter()
        .map(|session| {
            json!({
                "key": session["key"],
                "host": session["host"],
                "name": session["name"],
                "state": session["state"],
                "uncertainVerb": session["uncertainVerb"],
                "holdsHelper": session["holdsHelper"] == true,
            })
        })
        .collect::<Vec<_>>();

    // The view is the person's: a step that names a command reads as where
    // to look instead.
    let headline = match overview["headline"].as_str() {
        Some(text) if text.contains("atlas ") => "The helper isn't ready. Open Atlas to see why",
        Some(text) => text,
        None => "",
    };

    json!({
        "state": overview["state"],
        "label": overview["label"],
        "headline": headline,
        "reason": overview["reason"],
        "helper": overview["accountName"].as_str().or(overview["account"].as_str()),
        "display": overview["display"],
        "pending": overview["pending"],
        "commands": overview["commands"],
        "activity": rows,
        "sessionsShown": sessions_shown,
        "holder": overview["holder"],
        "self": own,
        "sessions": listed,
    })
}

/// A tool result for `status`: its words and the facts it acts on for the
/// model, and the whole status, history included, for the view in `_meta`.
pub(super) fn said(status: &Value) -> Value {
    let label = status["label"].as_str().unwrap_or("Not ready");
    let headline = status["headline"].as_str().unwrap_or_default();
    let mut text = format!("Atlas: {}", sentence(label));
    if !headline.is_empty() {
        text.push_str(&format!(" {}", sentence(headline)));
    }
    let earlier = items(&status["activity"])
        .iter()
        .filter(|row| row["actor"] != "viewer")
        .filter_map(|row| row["says"].as_str())
        .take(3)
        .collect::<Vec<_>>();
    if !earlier.is_empty() {
        text.push_str(&format!(" Recent: {}", sentence(&earlier.join("; "))));
    }

    let brief = json!({
        "state": status["state"],
        "label": status["label"],
        "headline": status["headline"],
        "reason": status["reason"],
        "pending": status["pending"],
        "commands": status["commands"],
        "holder": status["holder"],
        "self": status["self"],
    });
    let mut result = tool_result(&text, json!({"status": brief}));
    result["_meta"] = json!({ super::resources::STATUS_META: status });
    result
}

/// `text` ended as a sentence: with a full stop, unless it already ends
/// with one, an ellipsis ("Running a command…") or another mark that ends
/// a sentence.
fn sentence(text: &str) -> String {
    if text.ends_with(['.', '…', '!', '?']) {
        text.to_owned()
    } else {
        format!("{text}.")
    }
}

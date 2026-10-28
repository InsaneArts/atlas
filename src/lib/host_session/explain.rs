//! Naming other sessions in a refusal: who holds the helper on `busy`, who
//! last used the app in front on `focus-changed`, and whose an unknown
//! outcome is on `uncertain`. Every read is of a local file and none takes
//! the controller's lock. It only adds to a refusal; what was refused, and
//! why, stays as it was.
use super::{
    Asker, HostSession, Key, Recovery,
    names::{name_of, names_with},
    store::Store,
};
use crate::{
    client::Client,
    error::{Holder, Result},
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, time::Duration};

/// How long after a session used an app it is named as its last user.
const LAST_USED_WITHIN: Duration = Duration::from_secs(10 * 60);
/// How many recent rows are looked through for the app's last user.
const ROWS: usize = 64;
/// The most other sessions an unknown outcome names.
const CAUSED_BY: usize = 3;

/// A refusal that can say more about other sessions.
enum Refusal {
    /// Another Atlas command holds the helper.
    Busy,
    /// Another app, this one, was in front.
    FocusChanged(String),
    /// An earlier action's outcome is unknown.
    Uncertain,
}

impl Refusal {
    /// What `code`, `holder`, `uncertain` and `found` of a refusal say.
    fn of(code: &str, holder: Option<Holder>, uncertain: bool, found: &Value) -> Option<Self> {
        match code {
            "busy" if holder == Some(Holder::Agent) => Some(Self::Busy),
            "focus-changed" => found["app"]
                .as_str()
                .map(|app| Self::FocusChanged(app.to_owned())),
            "uncertain" if !uncertain => Some(Self::Uncertain),
            _ => None,
        }
    }
}

/// `result` with what Atlas knows of other sessions added to a refusal:
/// who holds the helper on busy, who last used the app in front on
/// focus-changed, and whose an unknown outcome is on uncertain. `client`
/// is asked for only when there is a refusal to add to; `own` is this
/// command's session, if any.
pub fn reply(
    result: Result<Value>,
    client: impl FnOnce() -> Option<Client>,
    own: Option<&HostSession>,
) -> Result<Value> {
    let refusal = match &result {
        Err(error) => Refusal::of(
            error.code,
            error.holder,
            error.uncertain,
            error.details.get("found").unwrap_or(&Value::Null),
        ),
        Ok(value) if value["ok"] == false => Refusal::of(
            value["code"].as_str().unwrap_or_default(),
            value["holder"].as_str().and_then(Holder::parse),
            value["uncertain"] == true,
            &value["found"],
        ),
        Ok(_) => None,
    };
    let Some(refusal) = refusal else {
        return result;
    };
    let Some(client) = client() else {
        return result;
    };

    let added = additions(&refusal, &client, own.map(HostSession::key));
    match result {
        Err(error) => Err(added
            .into_iter()
            .fold(error, |error, (key, value)| error.with(key, value))),
        Ok(mut value) => {
            if let Some(fields) = value.as_object_mut() {
                fields.extend(
                    added
                        .into_iter()
                        .map(|(key, value)| (key.to_owned(), value)),
                );
            }
            Ok(value)
        }
    }
}

/// The fields `refusal` gains, read from `client`'s files, for a command
/// of session `own`.
fn additions(refusal: &Refusal, client: &Client, own: Option<&Key>) -> Vec<(&'static str, Value)> {
    let Ok(output) = client.output_dir() else {
        return Vec::new();
    };
    // A session on its first command, as a holder, is named as the
    // status names it: from its note.
    let named = || {
        let recent = Store::at(output).recent();
        let notes: Vec<_> = client
            .controller
            .holder_note()
            .into_iter()
            .chain(client.controller.waiting_notes())
            .collect();
        names_with(recent.iter().map(|seen| &seen.record), &notes)
    };

    match refusal {
        Refusal::Busy => held_by(client, &named()),
        Refusal::FocusChanged(app) => last_used_by(output, app, own, &named()),
        Refusal::Uncertain => caused_by(client, own, &named()),
    }
}

/// Who holds the helper, from the holder note: only while its command
/// still runs, and never this command itself (the input service's grant
/// can answer busy while this command holds the lock).
fn held_by(client: &Client, names: &BTreeMap<Key, String>) -> Vec<(&'static str, Value)> {
    let Some(note) = client
        .controller
        .holder_note()
        .filter(|note| note.live() && !note.process.is_this_one())
    else {
        return Vec::new();
    };
    let Some(session) = &note.session else {
        return Vec::new();
    };

    vec![(
        "heldBy",
        json!({
            "session": session,
            "name": name_of(names, session, note.host),
            "host": note.host,
            "since": note.since,
        }),
    )]
}

/// The other session that last used `app`, within [`LAST_USED_WITHIN`],
/// from the activity log.
fn last_used_by(
    output: &std::path::Path,
    app: &str,
    own: Option<&Key>,
    names: &BTreeMap<Key, String>,
) -> Vec<(&'static str, Value)> {
    let now = crate::client::timestamp();
    let within = LAST_USED_WITHIN.as_nanos();
    let found = crate::overview::activity(output, ROWS)
        .into_iter()
        .filter(|row| row["actor"] == "agent")
        .filter(|row| {
            row["at"]
                .as_str()
                .and_then(|at| at.parse::<u128>().ok())
                .is_some_and(|at| now.saturating_sub(at) <= within)
        })
        .find_map(|row| {
            let session = row["session"].as_str().and_then(Key::parse)?;
            let used = row["app"]
                .as_str()
                .is_some_and(|used| used.eq_ignore_ascii_case(app));
            (used && Some(&session) != own).then_some(session)
        });

    found
        .map(|session| {
            vec![(
                "lastUsedBy",
                json!({"session": session, "name": name_of(names, &session, None)}),
            )]
        })
        .unwrap_or_default()
}

/// Whose the pending changes are: `yours` when every one is this
/// session's and none still runs, false when any is another's, and the
/// other sessions' names; and what to do about them, by the rule every
/// surface follows ([`Recovery`]). A command with no session of its own is
/// told only whose they are: none of them can be its.
fn caused_by(
    client: &Client,
    own: Option<&Key>,
    names: &BTreeMap<Key, String>,
) -> Vec<(&'static str, Value)> {
    let Ok(causes) = client.controller.pending_causes() else {
        return Vec::new();
    };
    let Some(recovery) = Recovery::of(&causes, Asker::Agent(own)) else {
        return Vec::new();
    };

    let mut others: Vec<&Key> = causes
        .iter()
        .filter_map(|cause| cause.session.as_ref())
        .filter(|session| Some(*session) != own)
        .collect();
    others.sort();
    others.dedup();

    let mut added = Vec::new();
    if recovery == Recovery::Own {
        added.push(("yours", json!(true)));
    } else if !others.is_empty() {
        if own.is_some() {
            added.push(("yours", json!(false)));
        }
        let named: Vec<String> = others
            .iter()
            .take(CAUSED_BY)
            .map(|session| name_of(names, session, None))
            .collect();
        added.push(("causedBy", json!(named)));
    }
    added.push(("next", json!(recovery.step())));
    added
}

#[cfg(test)]
mod tests;

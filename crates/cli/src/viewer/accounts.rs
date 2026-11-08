//! The helper's accounts as the person's viewer shows and changes them: the
//! list for the status, and adding an app or removing an account from
//! Settings. Agents have no way in here: the viewer serves only the app.
use super::{Viewer, unpoisoned};
use atlas_client::sign_ins::{self, Entry, How, Kind, Level, Listed};
use atlas_core::error::{Code, Error, Result};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

impl Viewer {
    /// The home folder whose accounts this viewer shows and changes; none
    /// when the person's could not be found, which lists nothing as known.
    pub(super) fn accounts_home(&self) -> Option<PathBuf> {
        unpoisoned(&self.accounts_home).clone()
    }
}

/// The list for the status: every account, or null when the record can't be
/// read, and the newest account Atlas added after the person typed a
/// password, for the one-time note.
pub(super) fn for_status(home: &Option<PathBuf>) -> (Value, Value) {
    let listed = home.as_deref().map_or(Listed::Unknown, sign_ins::listed_in);
    match listed {
        Listed::Accounts(accounts) => {
            let note = accounts
                .iter()
                .filter(|entry| entry.how == How::TakeControl)
                .max_by_key(|entry| entry.added)
                .map_or(
                    Value::Null,
                    |entry| json!({"name": entry.name, "added": entry.added}),
                );
            (json!(accounts), note)
        }
        Listed::Unknown => (Value::Null, Value::Null),
    }
}

/// Add the app `body` names by its bundle identifier and name: the person
/// chose it in Settings.
pub(super) fn add(home: &Option<PathBuf>, body: &Value) -> Result<Value> {
    let (Some(id), Some(name)) = (text(&body["id"]), text(&body["name"])) else {
        return Err(Error::new(Code::Input, "Name the app by its id and name"));
    };
    let entry = Entry {
        kind: Kind::App,
        id: id.to_owned(),
        name: name.to_owned(),
        browser: None,
        added: atlas_core::time::now_ms(),
        how: How::ByHand,
        level: Level::default(),
    };
    let added = sign_ins::add_in(known(home)?, entry, true)?;
    Ok(json!({"ok": true, "added": added}))
}

/// Remove the account `body` names: its kind, id, and for a site the
/// browser it was seen in.
pub(super) fn remove(home: &Option<PathBuf>, body: &Value) -> Result<Value> {
    let (kind, id) = named(body)?;
    let browser = body["browser"].as_str();
    let removed = sign_ins::remove_in(known(home)?, kind, id, browser, true)?;
    Ok(json!({"ok": true, "removed": removed}))
}

/// Set the level of the account `body` names: its kind, id and browser, and
/// `level` (`lookOnly`, `ask` or `trusted`).
pub(super) fn set_level(home: &Option<PathBuf>, body: &Value) -> Result<Value> {
    let (kind, id) = named(body)?;
    let level = serde_json::from_value::<Level>(body["level"].clone())
        .map_err(|_| Error::new(Code::Input, "Name the level: lookOnly, ask or trusted"))?;
    let browser = body["browser"].as_str();
    let changed = sign_ins::set_level_in(known(home)?, kind, id, browser, level, true)?;
    Ok(json!({"ok": true, "changed": changed}))
}

/// The account `body` names, by its kind and id.
fn named(body: &Value) -> Result<(Kind, &str)> {
    let kind = match body["kind"].as_str() {
        Some("site") => Kind::Site,
        Some("app") => Kind::App,
        _ => return Err(Error::new(Code::Input, "Name the account's kind")),
    };
    let Some(id) = text(&body["id"]) else {
        return Err(Error::new(Code::Input, "Name the account by its id"));
    };
    Ok((kind, id))
}

/// The questions up now, for the person: who asks, the account, what would
/// be sent and, for a message, its preview.
pub(super) fn asks(client: &atlas_client::client::Client) -> Value {
    let now = atlas_core::time::now_ms();
    let asks = client.controller.asks();
    json!({"ok": true, "asks": asks.open(now), "allowed": asks.allowed(now)})
}

/// The status's part of them: the questions up, without what would be sent,
/// and what the person allowed, once the grants whose agent session or host
/// process ended are ended.
pub(super) fn asks_for_status(client: &atlas_client::client::Client) -> (Value, Value) {
    let now = atlas_core::time::now_ms();
    let asks = client.controller.asks();
    // What the person allowed a task ends with its agent session; a status
    // that can't end it still never lets it cover anything.
    let _ = asks.end_unless(now, |allowed| atlas_client::asking::lives(client, allowed));
    let open: Vec<Value> = asks
        .open(now)
        .into_iter()
        .map(|ask| {
            json!({
                "id": ask.id,
                "who": ask.who,
                "task": ask.task,
                "account": ask.account,
                "what": ask.question["what"],
                "ends": ask.ends,
            })
        })
        .collect();
    (json!(open), json!(asks.allowed(now)))
}

/// The person's answer to the question `body` names: `task`, `once` or
/// `deny`.
pub(super) fn answer(client: &atlas_client::client::Client, body: &Value) -> Result<Value> {
    let Some(id) = text(&body["id"]) else {
        return Err(Error::new(Code::Input, "Name the question by its id"));
    };
    let answer = serde_json::from_value::<atlas_control::asks::Answer>(body["choice"].clone())
        .map_err(|_| Error::new(Code::Input, "Answer task, once or deny"))?;
    client
        .controller
        .asks()
        .answer(id, answer, true, atlas_core::time::now_ms())?;
    Ok(json!({"ok": true}))
}

/// End what the person allowed in the account `body` names.
pub(super) fn end_allowed(client: &atlas_client::client::Client, body: &Value) -> Result<Value> {
    let account = &body["account"];
    if !account.is_object() {
        return Err(Error::new(
            Code::Input,
            "Name the account by its kind, id and browser",
        ));
    }
    client
        .controller
        .asks()
        .end(Some(account), None, true, atlas_core::time::now_ms())?;
    Ok(json!({"ok": true}))
}

/// Add the account the person signed in to while they had control, as the
/// helper saw it (`seen`): the app, and the page's host. A record that
/// can't be written leaves it out; giving control back never fails for it.
pub(super) fn note_sign_in(home: &Option<PathBuf>, seen: &Value) {
    let (Ok(home), Ok(seen)) = (
        known(home),
        serde_json::from_value::<sign_ins::Seen>(seen.clone()),
    ) else {
        return;
    };
    let entry = sign_ins::account_of(&seen, atlas_core::time::now_ms());
    // The person's lease proved it: only their typing is looked at.
    let _ = sign_ins::add_in(home, entry, true);
}

/// The home folder, when the person's was found.
fn known(home: &Option<PathBuf>) -> Result<&Path> {
    home.as_deref()
        .ok_or_else(|| Error::new(Code::Config, "Atlas couldn't find your home folder"))
}

/// A string member with something in it, and no longer than a name needs.
fn text(value: &Value) -> Option<&str> {
    value
        .as_str()
        .map(str::trim)
        .filter(|text| !text.is_empty() && text.chars().count() <= 200)
}

#[cfg(test)]
mod tests;

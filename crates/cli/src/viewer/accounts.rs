//! The helper's accounts as the person's viewer shows and changes them: the
//! list for the status, and adding an app or removing an account from
//! Settings. Agents have no way in here: the viewer serves only the app.
use atlas_client::sign_ins::{self, Entry, How, Kind, Level, Listed};
use atlas_core::error::{Code, Error, Result};
use serde_json::{Value, json};

/// The list for the status: every account, or null when the record can't be
/// read, and the newest account Atlas added after the person typed a
/// password, for the one-time note.
pub(super) fn for_status() -> (Value, Value) {
    match sign_ins::listed() {
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
pub(super) fn add(body: &Value) -> Result<Value> {
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
    let added = sign_ins::add(entry, true)?;
    Ok(json!({"ok": true, "added": added}))
}

/// Remove the account `body` names: its kind, id, and for a site the
/// browser it was seen in.
pub(super) fn remove(body: &Value) -> Result<Value> {
    let kind = match body["kind"].as_str() {
        Some("site") => Kind::Site,
        Some("app") => Kind::App,
        _ => return Err(Error::new(Code::Input, "Name the account's kind")),
    };
    let Some(id) = text(&body["id"]) else {
        return Err(Error::new(Code::Input, "Name the account by its id"));
    };
    let browser = body["browser"].as_str();
    let removed = sign_ins::remove(kind, id, browser, true)?;
    Ok(json!({"ok": true, "removed": removed}))
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

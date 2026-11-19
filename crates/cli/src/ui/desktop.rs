//! The desktop a reply names. With more than one desktop listed, every reply
//! of a command that chose one says which, so an agent never mistakes one
//! desktop's answer for another's; with one listed, replies stay as they
//! always were.
use atlas_core::desktop::{Desktop, Kind};
use serde_json::{Value, json};
use std::{borrow::Cow, sync::OnceLock};

static NAMED: OnceLock<Value> = OnceLock::new();

/// `name`, the desktop `desktop` as the settings call it, with its kind.
pub fn of(name: &str, desktop: &Desktop) -> Value {
    let kind = match desktop.kind() {
        Kind::User(_) => "user",
        Kind::Vm(_) => "vm",
    };
    json!({"name": name, "kind": kind})
}

/// Name `desktop` ([`of`]) in every reply this process writes from now on:
/// set once, when its command chose it among several. The first stays.
pub fn name(desktop: Value) {
    let _ = NAMED.set(desktop);
}

/// The desktop this process's replies name: none when one desktop is
/// listed, or its command chose none.
pub fn named() -> Option<&'static Value> {
    NAMED.get()
}

/// `reply` naming `desktop`, when there is one to name and `reply` is an
/// object that names none of its own; otherwise `reply` as it is.
pub fn naming<'a>(reply: &'a Value, desktop: Option<&Value>) -> Cow<'a, Value> {
    match (desktop, reply) {
        (Some(desktop), Value::Object(fields)) if !fields.contains_key("desktop") => {
            let mut named = fields.clone();
            named.insert("desktop".to_owned(), desktop.clone());
            Cow::Owned(Value::Object(named))
        }
        _ => Cow::Borrowed(reply),
    }
}

#[cfg(test)]
#[path = "desktop_tests.rs"]
mod tests;

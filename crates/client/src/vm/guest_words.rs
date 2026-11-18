//! What a VM's guest says of itself, said of the VM. The guest's daemon
//! and input service are the very ones Second Login runs, so their
//! refusals, warnings and next steps may name "the helper": each exact
//! phrase of theirs that does is said of the VM by its name, where its
//! words come in from the guest. What the station says itself is worded
//! for its desktop where it is made, and never goes through this.
use serde_json::{Value, json};

/// How a phrase said of the helper is said of a VM.
#[derive(Clone, Copy)]
enum OnVm {
    /// The VM's name.
    Name,
    /// The VM's name, owning what follows.
    Owned,
    /// The VM's daemon.
    Daemon,
    /// The VM's name, then these words.
    NameThen(&'static str),
    /// These words in its place: a command a VM is served by instead.
    Instead(&'static str),
}

/// The exact phrases the station and the guest's daemon say of the helper,
/// each before any shorter one it begins, and how each is said of a VM.
const ON_VM: [(&str, OnVm); 12] = [
    ("The helper daemon", OnVm::Daemon),
    ("the helper daemon", OnVm::Daemon),
    ("This helper daemon", OnVm::Daemon),
    ("this helper daemon", OnVm::Daemon),
    ("the helper account", OnVm::Name),
    ("the hidden helper", OnVm::Name),
    ("The helper's", OnVm::Owned),
    ("the helper's", OnVm::Owned),
    ("The helper", OnVm::Name),
    ("the helper", OnVm::Name),
    ("Helper did not", OnVm::NameThen(" did not")),
    // `atlas apps` is not built for a VM: its apps are seen by its windows.
    ("atlas apps", OnVm::Instead("atlas windows")),
];

/// Whether a phrase may begin after `before`: at the start of the text,
/// after a space, or after punctuation that opens or separates words.
fn opens(before: Option<char>) -> bool {
    before.is_none_or(|before| before.is_whitespace() || "([{\"'`,;:!?".contains(before))
}

/// Whether a phrase may end before `after`, itself before `next`: at the
/// end of the text, before a space or punctuation that closes or separates
/// words, or a full stop that ends a sentence. Never inside a word, a name
/// such as `BuildHelper` or `helper-work`, or a path.
fn closes(after: Option<char>, next: Option<char>) -> bool {
    match after {
        None => true,
        Some('.') => next.is_none_or(char::is_whitespace),
        Some(after) => after.is_whitespace() || ")]}\"'`,;:!?".contains(after),
    }
}

/// `text`, which the station or the guest may have said of the helper,
/// said of the VM `name`: each exact phrase of [`ON_VM`] that stands as
/// words of their own, in one pass, so what it puts in is never read
/// again. Everything else, names and paths with `helper` in them among it,
/// is kept as it is.
pub(crate) fn said_of(text: &str, name: &str) -> String {
    let mut said = String::with_capacity(text.len());
    let (mut rest, mut before) = (text, None);
    'scan: while let Some(first) = rest.chars().next() {
        if opens(before) {
            for (phrase, on_vm) in ON_VM {
                let Some(after) = rest.strip_prefix(phrase) else {
                    continue;
                };
                let mut next = after.chars();
                if closes(next.next(), next.next()) {
                    match on_vm {
                        OnVm::Name => said.push_str(name),
                        OnVm::Owned => said.push_str(&format!("{name}'s")),
                        OnVm::Daemon => said.push_str(&format!("{name}'s daemon")),
                        OnVm::NameThen(then) => said.push_str(&format!("{name}{then}")),
                        OnVm::Instead(instead) => said.push_str(instead),
                    }
                    (rest, before) = (after, phrase.chars().last());
                    continue 'scan;
                }
            }
        }
        said.push(first);
        (rest, before) = (&rest[first.len_utf8()..], Some(first));
    }
    said
}

/// The guest's `reply`, its words said of the VM `name`: its refusal and
/// next step, and each of its warnings. Nothing else of it is touched.
pub(crate) fn reply_said_of(mut reply: Value, name: &str) -> Value {
    for field in ["error", "next"] {
        if let Some(text) = reply[field].as_str() {
            reply[field] = json!(said_of(text, name));
        }
    }
    if let Some(warnings) = reply.get_mut("warnings").and_then(Value::as_array_mut) {
        for warning in warnings {
            if let Some(text) = warning.as_str() {
                *warning = json!(said_of(text, name));
            }
        }
    }
    reply
}

#[cfg(test)]
#[path = "guest_words_tests.rs"]
mod tests;

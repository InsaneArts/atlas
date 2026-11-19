//! The native Save/Open check's reads in a machine desktop. The check's
//! fixture runs in a canary folder of its own,
//! `/private/tmp/atlas-canary.<letters or digits>`: it is opened once with
//! no link followed, only while it is this account's own and neither its
//! group nor any other account may write to it, and only its fixed files
//! are read beside that descriptor, each a plain file, never by its path.
//!
//! A scan of a panel is asked with a fresh challenge, written into the
//! folder for the fixture to copy into the next scan it starts. Only the
//! scan that carries that challenge back, of the attempt asked and in the
//! session the read was pinned to, answers it: one that does not come
//! within two seconds is answered late, never with an older scan.
use crate::{
    checked,
    error::Code,
    protocol::{SessionPin, failure},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::{Map, Value, json};
use std::{
    ffi::OsStr,
    path::Path,
    time::{Duration, Instant},
};

/// Where a canary folder is made, and how its name begins.
const ROOT: &str = "/private/tmp/";
const PREFIX: &str = "atlas-canary.";
/// The longest a canary folder's name may run past its prefix.
const SUFFIX: usize = 64;

/// The files of a canary folder a check reads, each by its fixed name.
const FILES: [&str; 6] = [
    "state.json",
    "events.jsonl",
    "panel-observer.json",
    "fixture.log",
    "atlas-fixture.txt",
    "atlas-open-fixture.txt",
];
/// The fixture's latest scan of a panel.
const OBSERVED: &str = "panel-observer.json";
/// The challenge a scan is asked with, for the fixture to copy.
const CHALLENGE: &str = "challenge.json";
/// The most of a canary folder's files read in one answer, all together.
const MOST: u64 = 1 << 20;
/// How long a scan of a new challenge is waited for.
pub(crate) const SCAN_WAIT: Duration = Duration::from_secs(2);
/// How often the fixture's scan is looked at while it is waited for.
const POLL: Duration = Duration::from_millis(50);

/// What a native read asks of a canary folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Its fixed files, each that is there.
    Files,
    /// The fixture's scan of the panel `attempt`, started once it was
    /// handed `challenge`.
    Scan { attempt: String, challenge: String },
    /// What each of `paths`, as the fixture said them, names in the folder:
    /// the folder itself, or one of its fixed files.
    Resolve { paths: Vec<String> },
}

/// The most paths one native read resolves.
pub(crate) const MOST_PATHS: usize = 4;
/// The longest path a native read resolves.
pub(crate) const LONGEST_PATH: usize = 1024;

/// Whether `directory` names a canary folder: one directly in
/// `/private/tmp`, named `atlas-canary.` and then only letters or digits.
#[must_use]
pub(crate) fn canary(directory: &str) -> bool {
    directory
        .strip_prefix(ROOT)
        .and_then(|name| name.strip_prefix(PREFIX))
        .is_some_and(|suffix| {
            (1..=SUFFIX).contains(&suffix.len())
                && suffix.bytes().all(|c| c.is_ascii_alphanumeric())
        })
}

/// The session `pin` names, as a scan carries it back: what binds a
/// challenge to the session its read was pinned to.
#[must_use]
pub fn epoch(pin: &SessionPin) -> String {
    format!("{}-{}", pin.security_session_id, pin.display_id)
}

/// The reply to a native read of `action` in the canary folder
/// `directory`, pinned to the session `pin` names: its files, each as
/// base64, or the scan that answered its challenge.
#[must_use]
pub fn answer(action: &Action, directory: &str, pin: SessionPin) -> Value {
    if !canary(directory) {
        return failure(Code::BadRequest, NOT_CANARY);
    }
    let folder = match checked::own_folder(Path::new(directory)) {
        Ok(folder) => folder,
        Err(_) => {
            return failure(
                Code::Native,
                "The canary folder is not one of this account's own that no one else may change.",
            );
        }
    };
    match action {
        Action::Files => files(&folder),
        Action::Scan { attempt, challenge } => scan(&folder, attempt, challenge, &epoch(&pin)),
        Action::Resolve { paths } => {
            let resolved: Vec<Value> = paths
                .iter()
                .map(|path| resolved(&folder, directory, path).map_or(Value::Null, Value::from))
                .collect();
            json!({"ok": true, "op": "native", "resolved": resolved})
        }
    }
}

/// What `path`, as the fixture said it, names in the canary folder
/// `folder`, opened at `directory`: empty for the folder itself, else the
/// fixed file's name, only while that entry is a plain file there; none for
/// anything else, a trailing slash, a link or a path outside it among them.
fn resolved(folder: &checked::Folder, directory: &str, path: &str) -> Option<String> {
    let within = path
        .strip_prefix(directory)
        .or_else(|| as_tmp_names(directory, path))?;
    if within.is_empty() {
        return Some(String::new());
    }
    let name = within.strip_prefix('/')?;
    if !FILES.contains(&name) {
        return None;
    }
    folder
        .holds_file(OsStr::new(name))
        .ok()?
        .then(|| name.to_owned())
}

/// The rest of `path` past the canary folder `directory` as `/tmp` names
/// it, only while this Mac's `/tmp` is macOS's own link to `private/tmp`.
fn as_tmp_names<'a>(directory: &str, path: &'a str) -> Option<&'a str> {
    let folder = directory.strip_prefix(ROOT)?;
    let rest = path.strip_prefix("/tmp/")?.strip_prefix(folder)?;
    (std::fs::read_link("/tmp").ok()? == Path::new("private/tmp")).then_some(rest)
}

/// The refusal of a directory that names no canary folder.
pub(crate) const NOT_CANARY: &str =
    "native names a canary folder: /private/tmp/atlas-canary. and then letters or digits";

/// Every fixed file of `folder` that is there, as base64, 1 MiB in all.
fn files(folder: &checked::Folder) -> Value {
    let mut left = MOST;
    let mut read = Map::new();
    for name in FILES {
        match folder.read_entry(OsStr::new(name), left) {
            Ok(Some(bytes)) => {
                left = left.saturating_sub(bytes.len() as u64);
                read.insert(name.to_owned(), json!(STANDARD.encode(bytes)));
            }
            Ok(None) => {}
            Err(refused) if refused.code == Code::Credentials => {
                return failure(
                    Code::Native,
                    format!(
                        "{name} in the canary folder is not a plain file of this account's own."
                    ),
                );
            }
            Err(_) => {
                return failure(
                    Code::Native,
                    format!(
                        "{name} in the canary folder could not be read within the 1 MiB a check reads."
                    ),
                );
            }
        }
    }
    json!({"ok": true, "op": "native", "files": read})
}

/// The fixture's scan of `attempt` in `folder` that carries `challenge`
/// and `epoch` back, handed to it first, waited for within [`SCAN_WAIT`].
///
/// The scan is not authenticated: any process of the fixture's account can
/// write it, as the guest's own code that reads it runs in that account
/// too. A forged scan can mislead this diagnostic check, never admit
/// anything, so the check's record says it is as its files report it.
fn scan(folder: &checked::Folder, attempt: &str, challenge: &str, epoch: &str) -> Value {
    let handed = json!({"challenge": challenge, "epoch": epoch}).to_string();
    if folder
        .replace(OsStr::new(CHALLENGE), handed.as_bytes(), 0o600)
        .is_err()
    {
        return failure(
            Code::Native,
            "The check's challenge could not be handed to the fixture.",
        );
    }
    let deadline = Instant::now() + SCAN_WAIT;
    loop {
        let observed = folder
            .read_entry(OsStr::new(OBSERVED), MOST)
            .ok()
            .flatten()
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok());
        // Judged once read: a scan that came after the bound never answers.
        let late = Instant::now() > deadline;
        if !late
            && let Some(observed) = observed
            && observed["challenge"] == challenge
            && observed["attemptID"] == attempt
            && observed["epoch"] == epoch
        {
            return json!({"ok": true, "op": "native", "scan": observed});
        }
        if late {
            let mut late = failure(
                Code::Native,
                "The fixture's scan did not answer this check's challenge within 2 seconds.",
            );
            late["late"] = json!(true);
            return late;
        }
        std::thread::sleep(POLL);
    }
}

#[cfg(test)]
#[path = "native_tests.rs"]
mod tests;

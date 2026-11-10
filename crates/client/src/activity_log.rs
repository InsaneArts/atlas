//! The activity log: one row for each step of a request that changes the
//! helper, appended to [`FILE`] in the output folder and read back from its
//! tail. Once it grows past [`ROTATE_AT`] it starts over, and the files it
//! filled are kept by age and size, as [`retained`] says. A row keeps only
//! what [`crate::doing::Doing`] names and fixed codes: never typed text,
//! argv, coordinates, window titles, output or image payloads.
mod retained;
pub mod session;
pub mod snapshot;

#[cfg(any(test, feature = "test-support"))]
pub(crate) use retained::HISTORY;
pub use session::{Reach, SessionRows, Whose, session_rows};
pub use snapshot::{recent, try_recent};

use atlas_core::{
    error::{Code, Error, Result},
    files::{LockOpen, open_lock_file},
};
use fs2::FileExt;
use serde::{Deserialize, Deserializer, Serialize, de::DeserializeOwned};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    fs::{self, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    os::unix::fs::OpenOptionsExt,
    path::Path,
};

/// The log's file name in the output folder.
pub const FILE: &str = "metadata.jsonl";
/// What the log is renamed to once it grows past [`ROTATE_AT`]: the newest
/// of the files it keeps.
pub(crate) const PREVIOUS: &str = "metadata.previous.jsonl";
/// The file whose lock writers take turns by.
pub(crate) const LOCK: &str = "metadata.lock";
/// How long the log grows before it starts over.
const ROTATE_AT: u64 = 1024 * 1024;
/// How much of the end of the log is read at a time.
const TAIL: u64 = 64 * 1024;
/// The most of the log one read goes through, from its end.
const READ_LIMIT: u64 = 4 * 1024 * 1024;
/// Personal input belongs to another run after two minutes without it,
/// or on another local calendar day.
const INPUT_GAP_MS: u64 = 120_000;

/// Each op with its word in requests and rows, declared once.
macro_rules! ops {
    ($($op:ident => $word:literal),* $(,)?) => {
        /// What a request asked of the helper, as its `op` names it. One this
        /// build does not know is kept as it was written.
        #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
        #[serde(from = "String", into = "String")]
        pub enum Op { $($op,)* Other(String) }

        impl Op {
            pub(crate) fn as_str(&self) -> &str {
                match self { $(Self::$op => $word,)* Self::Other(op) => op }
            }
        }

        impl From<&str> for Op {
            fn from(op: &str) -> Self {
                match op { $($word => Self::$op,)* other => Self::Other(other.to_owned()) }
            }
        }
    };
}

ops! {
    Apps => "apps",
    Collect => "collect",
    Input => "input",
    Job => "job",
    Logout => "logout",
    Read => "read",
    Restart => "restart",
    Run => "run",
    Screenshot => "screenshot",
}

impl Op {
    /// The op `request` names, when it names one.
    pub(crate) fn of(request: &Value) -> Option<Self> {
        request["op"].as_str().map(Self::from)
    }
}

impl From<String> for Op {
    fn from(op: String) -> Self {
        Self::from(op.as_str())
    }
}

impl From<Op> for String {
    fn from(op: Op) -> Self {
        op.as_str().to_owned()
    }
}

/// Why a row's request failed: one of Atlas's error codes or, for a
/// command run in the helper, that it ran too long (`timeout`) or ended
/// with a failure (`exit`). A word this build does not know is kept as it
/// was written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Failure {
    Code(Code),
    Timeout,
    Exit,
    Later(String),
}

impl Failure {
    /// The failure a row's word names.
    pub(crate) fn parse(word: &str) -> Self {
        match word {
            "timeout" => Self::Timeout,
            "exit" => Self::Exit,
            word => Code::parse(word).map_or_else(|| Self::Later(word.to_owned()), Self::Code),
        }
    }

    /// The word a row keeps for it.
    pub(crate) fn as_str(&self) -> &str {
        match self {
            Self::Code(code) => code.as_str(),
            Self::Timeout => "timeout",
            Self::Exit => "exit",
            Self::Later(word) => word,
        }
    }
}

impl Serialize for Failure {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for Failure {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        String::deserialize(deserializer).map(|word| Self::parse(&word))
    }
}

/// Which step of a request a row stands for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Step {
    /// A command began: what it does shows while it runs.
    Start,
    /// The request ended, as its reply says.
    End,
}

/// One row of the log. A field of another type than this build reads is
/// read as missing, as is one the row lacks, so a row any build wrote
/// still reads.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Row {
    /// When the action was recorded, in nanoseconds since 1970, as text.
    #[serde(deserialize_with = "lenient")]
    pub at: Option<String>,
    #[serde(deserialize_with = "lenient")]
    pub operation: Option<Op>,
    /// Whether it worked; none while a command it began runs.
    #[serde(deserialize_with = "lenient")]
    pub ok: Option<bool>,
    #[serde(deserialize_with = "lenient")]
    pub elapsed_ms: Option<f64>,
    /// How many input actions it sent; none for other requests.
    #[serde(deserialize_with = "lenient")]
    pub action_count: Option<u64>,
    /// Who asked: an agent, or the person through a viewer.
    #[serde(deserialize_with = "lenient")]
    pub actor: Option<String>,
    /// Why it failed, as a fixed code: never its message.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    pub code: Option<Failure>,
    /// Why another app in front refused it, as the input service's word.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// Whether such a refusal came after some of it was sent.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    pub partial: Option<bool>,
    /// The agent's host session's key.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    pub session: Option<String>,
    /// The key of a task split from its session.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    pub task: Option<String>,
    /// How many files and bytes a collection kept.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    pub count: Option<u64>,
    #[serde(deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    pub bytes: Option<u64>,
    /// The file name a screenshot gave.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    pub capture: Option<String>,
    /// What the agent's command did, as [`crate::doing::Doing`] names it.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    pub verb: Option<String>,
    /// The command's own token, the same on each of its rows.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    pub app: Option<String>,
    /// The bundle identifier of the app it acted in, such as
    /// `com.apple.TextEdit`, as [`crate::doing::bundle_id`] keeps it: never
    /// a path.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    pub bundle_id: Option<String>,
    #[serde(deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Whether it is a command's first step.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    pub started: Option<bool>,
    /// The file name of the picture taken right before an agent's input, as
    /// [`crate::before`] names it.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    pub before: Option<String>,
    /// The account the person listed that an agent's input acted in, when
    /// it acted in one: never an app or site they didn't list.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    pub account: Option<Account>,
    /// What let it act there: the person allowed it once or for the task,
    /// or the account is Trusted. None when nothing needed allowing.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    pub allowed: Option<Allowed>,
    /// The row's id, from what it holds as the log keeps it: never written.
    #[serde(skip)]
    pub id: String,
    /// The oldest member's identity while gathering personal input: never written.
    #[serde(skip)]
    pub first_input_id: Option<String>,
}

/// A listed account as a row names it: the name the person reads, and the
/// key it is listed by. One whose parts don't read is no account.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    pub name: String,
    pub key: AccountKey,
}

/// Which listed account: a site in a browser, or an app.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountKey {
    pub kind: AccountKind,
    pub id: String,
    /// The bundle identifier of the browser a site was listed in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub browser: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AccountKind {
    Site,
    App,
}

/// What let an agent's input act in a listed account.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Allowed {
    /// The person allowed this action once.
    Once,
    /// The person allowed the rest of the task in the account.
    Task,
    /// The account is Trusted.
    Trusted,
}

impl Account {
    /// The account `value` names, when it reads whole and its names are
    /// plain: one that doesn't is none.
    pub(crate) fn of(value: &Value) -> Option<Self> {
        let account: Self = serde_json::from_value(value.clone()).ok()?;
        let fits = |text: &str| !text.trim().is_empty() && text.chars().count() <= 200;
        (fits(&account.name)
            && fits(&account.key.id)
            && account.key.browser.as_deref().is_none_or(fits))
        .then_some(account)
    }

    /// The site it is, for a site: what a browser's action happened on.
    pub fn site(&self) -> Option<&str> {
        (self.key.kind == AccountKind::Site).then_some(self.name.as_str())
    }
}

impl Row {
    /// `value` as a row, with its id, when it names its operation.
    pub fn parse(value: &Value) -> Option<Self> {
        let mut row: Self = serde_json::from_value(value.clone()).ok()?;
        row.operation.as_ref()?;
        row.id = id(value);
        Some(row)
    }

    /// The action's identity across command steps or a growing personal input run.
    pub fn logical_id(&self) -> &str {
        self.command
            .as_deref()
            .or(self.first_input_id.as_deref())
            .or_else(|| self.persons_input().then_some(self.at.as_deref()).flatten())
            .unwrap_or(&self.id)
    }

    /// Nanoseconds since it was recorded, at `now`, when its time reads.
    pub(crate) fn age(&self, now: u128) -> Option<u128> {
        self.at_nanos().map(|at| now.saturating_sub(at))
    }

    /// When it was recorded, in milliseconds since 1970.
    pub fn at_ms(&self) -> Option<u64> {
        u64::try_from(self.at_nanos()? / 1_000_000).ok()
    }

    fn at_nanos(&self) -> Option<u128> {
        self.at.as_deref()?.parse().ok()
    }

    /// Whether it worked.
    pub fn worked(&self) -> bool {
        self.ok == Some(true)
    }

    /// Whether it failed: not under way, and not of no outcome.
    pub(crate) fn failed(&self) -> bool {
        self.ok == Some(false)
    }

    /// Whether it is a command's step still under way.
    pub(crate) fn under_way(&self) -> bool {
        self.started == Some(true) && self.ok.is_none()
    }

    /// Whether it failed with `code`.
    pub(crate) fn coded(&self, code: Code) -> bool {
        self.code == Some(Failure::Code(code))
    }

    /// Who asked, as the row says, an agent unless it says otherwise.
    pub fn actor(&self) -> &str {
        self.actor.as_deref().unwrap_or("agent")
    }

    /// Whether `op` is what it did.
    pub(crate) fn is(&self, op: &Op) -> bool {
        self.operation.as_ref() == Some(op)
    }

    /// Whether it is input the person sent through a viewer, which comes
    /// in a quick run of rows as they use the mouse and keyboard.
    pub(crate) fn persons_input(&self) -> bool {
        self.actor() == "viewer" && self.is(&Op::Input) && self.command.is_none()
    }
}

/// A field as this build reads it, or missing when it was written as
/// something else.
fn lenient<'de, D: Deserializer<'de>, T: DeserializeOwned>(
    deserializer: D,
) -> std::result::Result<Option<T>, D::Error> {
    Ok(serde_json::from_value(Value::deserialize(deserializer)?).ok())
}

/// A logged row's id: the same on every read, and never another row's.
///
/// The log keeps no sequence number, and a row's place in it moves: it is
/// read from its tail, and it starts over once it grows past a megabyte.
/// What a row holds never changes once written, though: it carries the
/// nanosecond its action was recorded at and, for an agent's command, that
/// command's random token. So the id is a digest of the row itself. A
/// command's newest step stands for it, so the row shown for a command
/// under way is another one, with another id, once it ends.
fn id(value: &Value) -> String {
    atlas_core::name::hex(&Sha256::digest(value.to_string().as_bytes())[..8])
}

/// Append `row` to the log in `directory`, under the log's lock, starting
/// the log over once it has grown past [`ROTATE_AT`] and keeping the file
/// it filled, as [`retained::rotate`] does. It waits its turn behind other
/// writers, so it runs off any async runtime.
pub(crate) fn append(directory: &Path, row: &Row) -> Result<()> {
    append_using(directory, row, |file, bytes| file.write_all(bytes)).map_err(
        atlas_core::context::context("Append activity log in", directory),
    )
}

fn append_using(
    directory: &Path,
    row: &Row,
    write: impl FnOnce(&mut fs::File, &[u8]) -> std::io::Result<()>,
) -> Result<()> {
    let lock = open_lock_file(&directory.join(LOCK), LockOpen::WriteNonblocking)?;
    if !lock.metadata()?.is_file() {
        return Err(Error::new(
            Code::State,
            "The activity lock is not a plain file",
        ));
    }
    let lock = atlas_core::files::LockFile(lock);
    FileExt::lock_exclusive(&lock.0)?;

    let path = directory.join(FILE);
    let mut file = open_log(&path)?;
    repair_tail(&mut file)?;
    if file.metadata()?.len() > ROTATE_AT {
        retained::rotate(directory)?;
        file = open_log(&path)?;
    }

    // Through a value, so its keys are written in the order the log has
    // always kept them. One buffer includes the newline so a failed write
    // can be rolled back as one append while the writer lock is still held.
    let line = serde_json::to_value(row)
        .map_err(|cause| Error::new(Code::State, cause.to_string()).with_source(cause))?;
    let bytes = format!("{line}\n");
    let length = file.metadata()?.len();
    if let Err(error) = write(&mut file, bytes.as_bytes()) {
        if let Err(rollback) = file.set_len(length) {
            return Err(Error::from(rollback).with_source(error));
        }
        return Err(error.into());
    }
    Ok(())
}

fn open_log(path: &Path) -> Result<fs::File> {
    let file = OpenOptions::new()
        .create(true)
        .read(true)
        .append(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)?;
    if !file.metadata()?.is_file() {
        return Err(Error::new(
            Code::State,
            "The activity log is not a plain file",
        ));
    }

    Ok(file)
}

/// Discard an unfinished append left by an interrupted writer or a failed
/// rollback. Only newline-terminated rows were acknowledged; preserve them
/// all, scanning backwards with bounded memory under the writer lock.
fn repair_tail(file: &mut fs::File) -> std::io::Result<()> {
    let length = file.metadata()?.len();
    if length == 0 {
        return Ok(());
    }
    let mut last = [0];
    file.seek(SeekFrom::End(-1))?;
    file.read_exact(&mut last)?;
    if last == [b'\n'] {
        return Ok(());
    }
    let mut end = length;
    let mut bytes = [0; TAIL as usize];
    while end > 0 {
        let start = end.saturating_sub(TAIL);
        let chunk = &mut bytes[..(end - start) as usize];
        file.seek(SeekFrom::Start(start))?;
        file.read_exact(chunk)?;
        if let Some(newline) = chunk.iter().rposition(|byte| *byte == b'\n') {
            let complete = start + newline as u64 + 1;
            if complete != length {
                file.set_len(complete)?;
            }
            return Ok(());
        }
        end = start;
    }
    file.set_len(0)?;
    Ok(())
}

/// Every retained row since `since`, in milliseconds, or no answer when
/// an unreadable or truncated log cannot establish that window's activity.
pub(crate) fn checked_since(directory: &Path, since: u64) -> Option<Vec<Row>> {
    let mut files = Vec::new();
    let mut rotated = false;
    for name in [FILE, PREVIOUS] {
        let path = directory.join(name);
        match atlas_core::files::try_open_plain(&path).ok()? {
            Some(file) => {
                rotated |= name == PREVIOUS;
                files.push(file);
            }
            None if fs::symlink_metadata(&path)
                .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound) => {}
            None => return None,
        }
    }
    let mut room = READ_LIMIT;
    let mut rows = Vec::new();
    for file in files {
        if !read_back(file, &mut room, &mut rows).ok()? {
            return None;
        }
    }
    if rows.iter().any(|row| row.at_ms().is_none())
        || (rotated
            && !rows
                .iter()
                .any(|row| row.at_ms().is_some_and(|at| at <= since)))
    {
        return None;
    }
    rows.retain(|row| row.at_ms().is_some_and(|at| at >= since));
    Some(rows)
}

/// Candidates read in reverse append order, ordered for [`selected`]. Wall-clock
/// time can move backwards during a command, so its last committed step wins
/// by append order before timestamps order the commands for display.
/// Equal or missing times keep reverse append order; rows without a readable
/// time follow those with one.
fn ordered(rows: &mut Vec<Row>) {
    let mut commands = HashSet::new();
    rows.retain(|row| {
        row.command
            .as_ref()
            .is_none_or(|command| commands.insert(command.clone()))
    });
    rows.sort_by_key(|row| std::cmp::Reverse(row.at_nanos()));
}

/// Select commands and complete personal input runs from ordered candidates.
fn selected(candidates: &[Row], limit: usize) -> Vec<Row> {
    if limit == 0 {
        return Vec::new();
    }

    let mut rows = Vec::new();
    let mut oldest_at = None;
    for row in candidates {
        let at = row.at_ms();
        if !gathered(&mut rows, row, oldest_at) {
            if rows.len() == limit {
                break;
            }
            rows.push(row.clone());
        }
        oldest_at = at;
    }
    rows
}

/// Read candidate rows in reverse append order, using at most `room` bytes,
/// and report whether every byte decoded into a row.
fn read_back(file: fs::File, room: &mut u64, rows: &mut Vec<Row>) -> std::io::Result<bool> {
    let end = file.metadata()?.len();
    read_rows(file, end, room, rows)
}

fn read_rows(
    mut file: impl Read + Seek,
    mut end: u64,
    room: &mut u64,
    rows: &mut Vec<Row>,
) -> std::io::Result<bool> {
    // The start of the oldest line read so far, whose beginning is in the
    // part of the file before it.
    let mut partial = Vec::new();
    let mut complete = true;

    while end > 0 && *room > 0 {
        let size = TAIL.min(end).min(*room);
        let start = end - size;
        file.seek(SeekFrom::Start(start))?;
        let mut bytes = vec![0; size as usize];
        *room -= size;
        file.read_exact(&mut bytes)?;
        bytes.append(&mut partial);

        // Lines are cut at their newline only, so a character is never
        // split; the first line of a read that did not reach the file's
        // start waits for the part before it.
        let whole = if start == 0 {
            0
        } else {
            match bytes.iter().position(|byte| *byte == b'\n') {
                Some(newline) => newline + 1,
                None => bytes.len(),
            }
        };
        partial = bytes[..whole].to_vec();

        for line in bytes[whole..].split_inclusive(|byte| *byte == b'\n').rev() {
            // The writer acknowledges a row only once its newline is written.
            let Some(line) = line.strip_suffix(b"\n") else {
                complete = false;
                continue;
            };
            let Some(row) = serde_json::from_slice::<Value>(line)
                .ok()
                .and_then(|value| Row::parse(&value))
            else {
                complete = false;
                continue;
            };
            rows.push(row);
        }

        end = start;
    }
    Ok(complete && end == 0)
}

/// Gather `row` into the last selected personal input run,
/// adding its actions when it belongs to that run; whether it did.
fn gathered(rows: &mut [Row], row: &Row, oldest_at: Option<u64>) -> bool {
    let Some(newer) = rows.last_mut() else {
        return false;
    };

    if row.persons_input()
        && newer.persons_input()
        && (row.ok, &row.code) == (newer.ok, &newer.code)
        && row.at_ms().zip(oldest_at).is_some_and(|(older, newer)| {
            older <= newer
                && newer - older <= INPUT_GAP_MS
                && atlas_core::time::same_local_day(older, newer)
        })
    {
        newer.first_input_id = Some(row.logical_id().to_owned());
        newer.action_count = match (newer.action_count, row.action_count) {
            (None, None) => None,
            (newer, older) => Some(newer.unwrap_or(0).saturating_add(older.unwrap_or(0))),
        };
        return true;
    }

    false
}

#[cfg(test)]
#[path = "activity_log_tests.rs"]
mod tests;

//! The activity log: one row for each step of a request that changes the
//! helper, appended to [`FILE`] in the output folder and read back from its
//! tail. A row keeps only what [`crate::doing::Doing`] names and fixed
//! codes: never typed text, argv, coordinates, window titles, output or
//! image payloads.
pub(crate) mod snapshot;

use crate::error::{Code, Error, Result};
use fs2::FileExt;
use serde::{Deserialize, Deserializer, Serialize, de::DeserializeOwned};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    os::unix::fs::OpenOptionsExt,
    path::Path,
};

/// The log's file name in the output folder.
pub const FILE: &str = "metadata.jsonl";
/// What the log is renamed to once it grows past [`ROTATE_AT`].
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

/// What a request asked of the helper, as its `op` names it. One this
/// build does not know is kept as it was written.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "String", into = "String")]
pub enum Op {
    Apps,
    Collect,
    Input,
    Job,
    Logout,
    Read,
    Restart,
    Run,
    Screenshot,
    Other(String),
}

impl Op {
    /// The op `request` names, when it names one.
    pub fn of(request: &Value) -> Option<Self> {
        request["op"].as_str().map(Self::from)
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::Apps => "apps",
            Self::Collect => "collect",
            Self::Input => "input",
            Self::Job => "job",
            Self::Logout => "logout",
            Self::Read => "read",
            Self::Restart => "restart",
            Self::Run => "run",
            Self::Screenshot => "screenshot",
            Self::Other(op) => op,
        }
    }
}

impl From<&str> for Op {
    fn from(op: &str) -> Self {
        match op {
            "apps" => Self::Apps,
            "collect" => Self::Collect,
            "input" => Self::Input,
            "job" => Self::Job,
            "logout" => Self::Logout,
            "read" => Self::Read,
            "restart" => Self::Restart,
            "run" => Self::Run,
            "screenshot" => Self::Screenshot,
            other => Self::Other(other.to_owned()),
        }
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
    pub fn parse(word: &str) -> Self {
        match word {
            "timeout" => Self::Timeout,
            "exit" => Self::Exit,
            word => Code::parse(word).map_or_else(|| Self::Later(word.to_owned()), Self::Code),
        }
    }

    /// The word a row keeps for it.
    pub fn as_str(&self) -> &str {
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
pub enum Step {
    /// A command began: what it does shows while it runs.
    Start,
    /// The request ended, as its reply says.
    End,
}

/// One row of the log. A field of another type than this build reads is
/// read as missing, as is one the row lacks, so a row any build wrote
/// still reads.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Row {
    /// When the action was recorded, in nanoseconds since 1970, as text.
    #[serde(default, deserialize_with = "lenient")]
    pub at: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    pub operation: Option<Op>,
    /// Whether it worked; none while a command it began runs.
    #[serde(default, deserialize_with = "lenient")]
    pub ok: Option<bool>,
    #[serde(default, deserialize_with = "lenient")]
    pub elapsed_ms: Option<f64>,
    /// How many input actions it sent; none for other requests.
    #[serde(default, deserialize_with = "lenient")]
    pub action_count: Option<u64>,
    /// Who asked: an agent, or the person through a viewer.
    #[serde(default, deserialize_with = "lenient")]
    pub actor: Option<String>,
    /// Why it failed, as a fixed code: never its message.
    #[serde(
        default,
        deserialize_with = "lenient",
        skip_serializing_if = "Option::is_none"
    )]
    pub code: Option<Failure>,
    /// Why another app in front refused it, as the input service's word.
    #[serde(
        default,
        deserialize_with = "lenient",
        skip_serializing_if = "Option::is_none"
    )]
    pub reason: Option<String>,
    /// Whether such a refusal came after some of it was sent.
    #[serde(
        default,
        deserialize_with = "lenient",
        skip_serializing_if = "Option::is_none"
    )]
    pub partial: Option<bool>,
    /// The agent's host session's key.
    #[serde(
        default,
        deserialize_with = "lenient",
        skip_serializing_if = "Option::is_none"
    )]
    pub session: Option<String>,
    /// The key of a task split from its session.
    #[serde(
        default,
        deserialize_with = "lenient",
        skip_serializing_if = "Option::is_none"
    )]
    pub task: Option<String>,
    /// How many files and bytes a collection kept.
    #[serde(
        default,
        deserialize_with = "lenient",
        skip_serializing_if = "Option::is_none"
    )]
    pub count: Option<u64>,
    #[serde(
        default,
        deserialize_with = "lenient",
        skip_serializing_if = "Option::is_none"
    )]
    pub bytes: Option<u64>,
    /// The file name a screenshot gave.
    #[serde(
        default,
        deserialize_with = "lenient",
        skip_serializing_if = "Option::is_none"
    )]
    pub capture: Option<String>,
    /// What the agent's command did, as [`crate::doing::Doing`] names it.
    #[serde(
        default,
        deserialize_with = "lenient",
        skip_serializing_if = "Option::is_none"
    )]
    pub verb: Option<String>,
    /// The command's own token, the same on each of its rows.
    #[serde(
        default,
        deserialize_with = "lenient",
        skip_serializing_if = "Option::is_none"
    )]
    pub command: Option<String>,
    #[serde(
        default,
        deserialize_with = "lenient",
        skip_serializing_if = "Option::is_none"
    )]
    pub app: Option<String>,
    /// The bundle identifier of the app it acted in, such as
    /// `com.apple.TextEdit`, as [`crate::doing::bundle_id`] keeps it: never
    /// a path.
    #[serde(
        default,
        deserialize_with = "lenient",
        skip_serializing_if = "Option::is_none"
    )]
    pub bundle_id: Option<String>,
    #[serde(
        default,
        deserialize_with = "lenient",
        skip_serializing_if = "Option::is_none"
    )]
    pub label: Option<String>,
    /// Whether it is a command's first step.
    #[serde(
        default,
        deserialize_with = "lenient",
        skip_serializing_if = "Option::is_none"
    )]
    pub started: Option<bool>,
    /// The row's id, from what it holds as the log keeps it: never written.
    #[serde(skip)]
    pub id: String,
    /// The oldest member's identity while gathering personal input: never written.
    #[serde(skip)]
    pub first_input_id: Option<String>,
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
    pub(crate) fn logical_id(&self) -> &str {
        self.command
            .as_deref()
            .or(self.first_input_id.as_deref())
            .or_else(|| self.persons_input().then_some(self.at.as_deref()).flatten())
            .unwrap_or(&self.id)
    }

    /// Nanoseconds since it was recorded, at `now`, when its time reads.
    pub fn age(&self, now: u128) -> Option<u128> {
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
    pub fn failed(&self) -> bool {
        self.ok == Some(false)
    }

    /// Whether it is a command's step still under way.
    pub fn under_way(&self) -> bool {
        self.started == Some(true) && self.ok.is_none()
    }

    /// Whether it failed with `code`.
    pub fn coded(&self, code: Code) -> bool {
        self.code == Some(Failure::Code(code))
    }

    /// Who asked, as the row says, an agent unless it says otherwise.
    pub fn actor(&self) -> &str {
        self.actor.as_deref().unwrap_or("agent")
    }

    /// Whether `op` is what it did.
    pub fn is(&self, op: &Op) -> bool {
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
    crate::name::hex(&Sha256::digest(value.to_string().as_bytes())[..8])
}

/// Append `row` to the log in `directory`, under the log's lock, starting
/// the log over once it has grown past [`ROTATE_AT`]. It waits its turn
/// behind other writers, so it runs off any async runtime.
pub(crate) fn append(directory: &Path, row: &Row) -> Result<()> {
    append_using(directory, row, |file, bytes| file.write_all(bytes)).map_err(
        crate::context::context(format_args!(
            "Append activity log in {}",
            directory.display()
        )),
    )
}

fn append_using(
    directory: &Path,
    row: &Row,
    write: impl FnOnce(&mut fs::File, &[u8]) -> std::io::Result<()>,
) -> Result<()> {
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(directory.join(LOCK))?;
    if !lock.metadata()?.is_file() {
        return Err(Error::new(
            Code::State,
            "The activity lock is not a plain file",
        ));
    }
    FileExt::lock_exclusive(&lock)?;

    let path = directory.join(FILE);
    let mut file = open_log(&path)?;
    repair_tail(&mut file)?;
    if file.metadata()?.len() > ROTATE_AT {
        fs::rename(&path, directory.join(PREVIOUS))?;
        file = open_log(&path)?;
    }

    // Through a value, so its keys are written in the order the log has
    // always kept them. One buffer includes the newline so a failed write
    // can be rolled back as one append while the writer lock is still held.
    let line =
        serde_json::to_value(row).map_err(|error| Error::new(Code::State, error.to_string()))?;
    let bytes = format!("{line}\n");
    let length = file.metadata()?.len();
    if let Err(error) = write(&mut file, bytes.as_bytes()) {
        file.set_len(length)
            .map_err(crate::context::context(format_args!(
                "Restore activity log length after append failed ({error})"
            )))?;
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

/// One row per command from the log in `directory`, newest first, at most
/// `limit`. A command that acted in several steps (typing in batches,
/// opening then waiting) is its newest step, and a run of the person's
/// input through a viewer is its newest row, counting every action of the
/// run. A line that does not read, or names no operation, is passed over.
///
/// Candidates are read from the end, within [`READ_LIMIT`] bytes across
/// both logs. Each command's last appended step is selected before ordering
/// by recorded time, grouping personal input and limiting. Buffered personal
/// input can be appended after a newer action,
/// including across rotation. Equal or missing times keep reverse append
/// order; rows without a readable time follow those with one.
pub fn recent(directory: &Path, limit: usize) -> Vec<Row> {
    snapshot::recent(directory, limit)
}

/// Like [`recent`], but reports an unreadable or refused log instead of
/// returning only the rows that could be read. Missing logs are empty;
/// malformed rows are still skipped within the same byte budget.
pub fn try_recent(directory: &Path, limit: usize) -> std::io::Result<Vec<Row>> {
    snapshot::try_recent(directory, limit)
}

/// Every retained row since `since`, in milliseconds, or no answer when
/// an unreadable or truncated log cannot establish that window's activity.
pub(crate) fn checked_since(directory: &Path, since: u64) -> Option<Vec<Row>> {
    let mut files = Vec::new();
    let mut rotated = false;
    for name in [FILE, PREVIOUS] {
        let path = directory.join(name);
        match crate::files::try_open_plain(&path).ok()? {
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
        let mut bytes = Vec::new();
        file.seek(SeekFrom::Start(start))?;
        bytes.resize(size as usize, 0);
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
                && crate::time::same_local_day(older, newer)
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
mod tests {
    use super::*;
    use serde_json::json;
    use std::{fs, io::Write};

    #[test]
    fn a_failed_append_rolls_back_its_prefix_before_the_row_is_retried() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(FILE);
        let original = log([agents(1)]);
        let row = Row::parse(&agents(2)).unwrap();
        for fraction in 0..=4 {
            fs::write(&path, &original).unwrap();
            let error = append_using(directory.path(), &row, |file, bytes| {
                file.write_all(&bytes[..bytes.len() * fraction / 4])?;
                Err(std::io::Error::other("simulated disk exhaustion"))
            })
            .unwrap_err();
            assert!(error.error.contains("simulated disk exhaustion"));
            assert_eq!(fs::read_to_string(&path).unwrap(), original);
            append(directory.path(), &row).unwrap();
            assert_eq!(times(&recent(directory.path(), 5)), ["2", "1"]);
            assert_eq!(fs::read_to_string(&path).unwrap().lines().count(), 2);
        }
    }

    #[test]
    fn an_unfinished_tail_is_removed_before_an_append_or_rotation() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(FILE);
        let row = Row::parse(&agents(2)).unwrap();
        let tails = [
            "{\"operation\":".to_owned(),
            agents(2).to_string(),
            "x".repeat(ROTATE_AT as usize + 1),
        ];
        for original in [String::new(), log([agents(1)])] {
            for tail in &tails {
                fs::write(&path, format!("{original}{tail}")).unwrap();
                append(directory.path(), &row).unwrap();
                let expected = format!("{original}{}\n", serde_json::to_value(&row).unwrap());
                assert_eq!(fs::read_to_string(&path).unwrap(), expected);
                assert!(!directory.path().join(PREVIOUS).exists());
            }
        }
    }

    #[test]
    fn a_failed_append_after_rotation_keeps_the_previous_rows_and_can_be_retried() {
        let directory = tempfile::tempdir().unwrap();
        let original = log([agents(1)]).repeat(ROTATE_AT as usize / log([agents(1)]).len() + 1);
        fs::write(directory.path().join(FILE), &original).unwrap();
        let row = Row::parse(&agents(2)).unwrap();
        assert!(
            append_using(directory.path(), &row, |file, bytes| {
                file.write_all(&bytes[..bytes.len() / 2])?;
                Err(std::io::Error::other("simulated disk exhaustion"))
            })
            .is_err()
        );
        assert_eq!(
            fs::read_to_string(directory.path().join(PREVIOUS)).unwrap(),
            original
        );
        assert_eq!(fs::metadata(directory.path().join(FILE)).unwrap().len(), 0);
        append(directory.path(), &row).unwrap();
        assert_eq!(times(&recent(directory.path(), 2)), ["2", "1"]);
    }

    #[test]
    fn the_activity_log_is_read_only_as_a_plain_file_never_a_link_or_a_pipe() {
        let dir = tempfile::tempdir().unwrap();
        let elsewhere = dir.path().join("elsewhere.jsonl");
        fs::write(&elsewhere, "{\"at\":\"1\",\"operation\":\"input\"}\n").unwrap();
        std::os::unix::fs::symlink(&elsewhere, dir.path().join(FILE)).unwrap();
        assert!(recent(dir.path(), 5).is_empty(), "a link");

        fs::remove_file(dir.path().join(FILE)).unwrap();
        nix::unistd::mkfifo(
            &dir.path().join(FILE),
            nix::sys::stat::Mode::S_IRUSR | nix::sys::stat::Mode::S_IWUSR,
        )
        .unwrap();
        // A pipe would hold a blocking open forever; this returns at once.
        assert!(recent(dir.path(), 5).is_empty(), "a pipe");
    }

    #[test]
    fn a_command_of_several_steps_is_one_row() {
        let dir = tempfile::tempdir().unwrap();
        let rows = [
            json!({"operation": "run", "ok": true, "verb": "open", "command": "a"}),
            json!({"operation": "input", "ok": true, "verb": "type", "command": "b"}),
            json!({"operation": "input", "ok": false, "verb": "type", "command": "b"}),
            json!({"operation": "input", "ok": true}),
            json!({"operation": "input", "ok": true}),
        ];
        let log = rows
            .iter()
            .map(|row| format!("{row}\n"))
            .collect::<String>();
        fs::write(dir.path().join(FILE), log).unwrap();

        let shown = recent(dir.path(), 12);
        let says = shown.iter().map(crate::doing::says).collect::<Vec<_>>();
        assert_eq!(
            says,
            [
                "used the mouse and keyboard",
                "used the mouse and keyboard",
                "couldn't type",
                "opened a file"
            ],
            "the newest step stands for its command; rows of no command stay apart"
        );
        assert_eq!(recent(dir.path(), 2).len(), 2);
    }

    #[test]
    fn the_newest_rows_are_read_from_the_logs_tail() {
        let dir = tempfile::tempdir().unwrap();
        assert!(recent(dir.path(), 5).is_empty());

        let rows: String = (0..30)
            .map(|i| format!("{{\"at\":\"{i}\",\"operation\":\"input\",\"ok\":true}}\n"))
            .chain(["not json\n".to_owned()])
            .collect();
        fs::write(dir.path().join(FILE), rows).unwrap();
        let newest = recent(dir.path(), 3);
        assert_eq!(newest.len(), 3);
        assert_eq!(newest[0].at.as_deref(), Some("29"));
        assert_eq!(newest[2].at.as_deref(), Some("27"));
    }

    #[test]
    fn a_complete_row_without_its_newline_is_not_read_as_written() {
        for name in [FILE, PREVIOUS] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join(name);
            let mut start = agents(1);
            start["ok"] = Value::Null;
            start["started"] = json!(true);
            let mut end = agents(2);
            end["command"] = start["command"].clone();
            for original in [String::new(), log([start.clone()])] {
                fs::write(&path, format!("{original}{end}")).unwrap();
                let shown = recent(dir.path(), 5);
                if original.is_empty() {
                    assert!(shown.is_empty());
                } else {
                    assert_eq!(times(&shown), ["1"]);
                    assert!(shown[0].under_way());
                }

                OpenOptions::new()
                    .append(true)
                    .open(&path)
                    .unwrap()
                    .write_all(b"\n")
                    .unwrap();
                let shown = recent(dir.path(), 5);
                assert_eq!(times(&shown), ["2"]);
                assert!(shown[0].worked());
            }
        }
    }

    #[test]
    fn a_row_split_across_a_chunk_boundary_still_reads_once_it_is_terminated() {
        for name in [FILE, PREVIOUS] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join(name);
            let mut row = agents(2);
            row["pad"] = json!("é".repeat(TAIL as usize));
            let line = row.to_string();
            assert!(line.len() as u64 > 2 * TAIL);
            fs::write(&path, format!("{}{line}", log([agents(1)]))).unwrap();
            assert_eq!(times(&recent(dir.path(), 5)), ["1"]);

            OpenOptions::new()
                .append(true)
                .open(&path)
                .unwrap()
                .write_all(format!("\n{}", log([agents(3)])).as_bytes())
                .unwrap();
            assert_eq!(times(&recent(dir.path(), 5)), ["3", "2", "1"]);
        }
    }

    #[test]
    fn a_tail_that_starts_inside_a_character_still_reads() {
        let dir = tempfile::tempdir().unwrap();
        let mut text = format!(
            "{{\"operation\":\"old\",\"pad\":\"{}\"}}\n{{\"at\":\"1\",\"operation\":\"input\"}}\n",
            "é".repeat(40_000)
        );
        let cut = |text: &str| text.as_bytes()[text.len() - TAIL as usize];
        if cut(&text) & 0xC0 != 0x80 {
            text.push('\n');
        }

        assert_eq!(cut(&text) & 0xC0, 0x80, "the tail starts inside é");
        fs::write(dir.path().join(FILE), text).unwrap();
        let rows = recent(dir.path(), 1);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].at.as_deref(), Some("1"));

        let rows = recent(dir.path(), 5);
        assert_eq!(rows.len(), 2, "the line the tail cut is read whole");
        assert_eq!(rows[1].operation, Some(Op::Other("old".into())));
    }

    /// `rows` as the log's lines.
    fn log(rows: impl IntoIterator<Item = Value>) -> String {
        rows.into_iter().map(|row| format!("{row}\n")).collect()
    }

    /// One of the person's inputs through a viewer, at `at`.
    fn persons(at: u64) -> Value {
        json!({"at": at.to_string(), "operation": "input", "ok": true, "actionCount": 1,
            "actor": "viewer", "elapsedMs": 2.5})
    }

    /// An agent's click, at `at`.
    fn agents(at: u64) -> Value {
        json!({"at": at.to_string(), "operation": "input", "ok": true, "actionCount": 1,
            "actor": "agent", "verb": "click", "command": format!("{at:012}")})
    }

    #[test]
    fn delayed_input_cannot_displace_a_newer_action_even_across_chunks_or_rotation() {
        let directory = tempfile::tempdir().unwrap();
        for rotated in [false, true] {
            let older = log([agents(1), agents(3)]);
            let delayed = format!("{}{}", filler(TAIL as usize), log([persons(2)]));
            if rotated {
                fs::write(directory.path().join(PREVIOUS), older).unwrap();
                fs::write(directory.path().join(FILE), delayed).unwrap();
            } else {
                fs::write(directory.path().join(FILE), format!("{older}{delayed}")).unwrap();
            }
            assert_eq!(times(&recent(directory.path(), 1)), ["3"]);
            assert_eq!(times(&recent(directory.path(), 3)), ["3", "2", "1"]);
        }
    }

    #[test]
    fn timestamps_are_ordered_numerically_and_ties_keep_reverse_append_order() {
        let directory = tempfile::tempdir().unwrap();
        let rows = [
            json!({"operation": "read", "at": "10", "app": "first"}),
            json!({"operation": "read", "at": "10", "app": "second"}),
            json!({"operation": "read", "at": "9", "app": "older"}),
            json!({"operation": "read", "app": "missing"}),
            json!({"operation": "read", "at": "bad", "app": "invalid"}),
        ];
        fs::write(directory.path().join(FILE), log(rows)).unwrap();
        let shown = recent(directory.path(), 10);
        assert_eq!(
            shown
                .iter()
                .map(|row| row.app.as_deref().unwrap())
                .collect::<Vec<_>>(),
            ["second", "first", "older", "invalid", "missing"]
        );
    }

    #[test]
    fn interleaved_command_steps_are_counted_once_before_the_limit_is_applied() {
        let directory = tempfile::tempdir().unwrap();
        let mut start = agents(1);
        start["ok"] = Value::Null;
        start["started"] = json!(true);
        let mut end = agents(3);
        end["command"] = start["command"].clone();
        fs::write(
            directory.path().join(FILE),
            log([start, persons(2), end, persons(0)]),
        )
        .unwrap();
        let shown = recent(directory.path(), 3);
        assert_eq!(times(&shown), ["3", "2"]);
        assert_eq!(shown[0].ok, Some(true));
        assert_eq!(shown[1].action_count, Some(2));
    }

    #[test]
    fn a_run_of_the_persons_input_counts_once_so_agents_rows_stay_in_reach() {
        let dir = tempfile::tempdir().unwrap();
        let rows = (0..3)
            .map(agents)
            .chain((10..60).map(persons))
            .chain([agents(70)])
            .chain((80..90).map(persons));
        fs::write(dir.path().join(FILE), log(rows)).unwrap();

        let shown = recent(dir.path(), 5);
        let read = shown
            .iter()
            .map(|row| (row.actor(), row.at.as_deref().unwrap(), row.action_count))
            .collect::<Vec<_>>();
        assert_eq!(
            read,
            [
                ("viewer", "89", Some(10)),
                ("agent", "70", Some(1)),
                ("viewer", "59", Some(50)),
                ("agent", "2", Some(1)),
                ("agent", "1", Some(1)),
            ],
            "the newest of a run stands for it, with every action it sent"
        );

        let mut failed = persons(91);
        failed["ok"] = json!(false);
        failed["code"] = json!("busy");
        let mut file = OpenOptions::new()
            .append(true)
            .open(dir.path().join(FILE))
            .unwrap();
        file.write_all(log([failed]).as_bytes()).unwrap();
        let shown = recent(dir.path(), 2);
        assert_eq!(
            shown.iter().map(Row::failed).collect::<Vec<_>>(),
            [true, false],
            "a failure is never folded into input that worked"
        );
    }

    #[test]
    fn accepted_action_counts_saturate_when_a_personal_input_run_overflows() {
        let dir = tempfile::tempdir().unwrap();
        for counts in [
            [u64::MAX, 1],
            [1, u64::MAX],
            [u64::MAX, u64::MAX],
            [u64::MAX - 1, 1],
        ] {
            let rows = counts.into_iter().enumerate().map(|(at, count)| {
                let mut row = persons(at as u64);
                row["actionCount"] = json!(count);
                row
            });
            fs::write(dir.path().join(FILE), log(rows)).unwrap();
            let shown = recent(dir.path(), 1);
            assert_eq!(shown.len(), 1, "{counts:?}");
            assert_eq!(shown[0].action_count, Some(u64::MAX), "{counts:?}");
            assert_eq!(shown[0].at.as_deref(), Some("1"));
        }
    }

    #[test]
    fn personal_input_after_a_pause_remains_separate() {
        let dir = tempfile::tempdir().unwrap();
        let rows = [1, INPUT_GAP_MS + 2].map(|at| {
            let mut row = persons(0);
            row["at"] = json!((u128::from(at) * 1_000_000).to_string());
            row
        });
        fs::write(dir.path().join(FILE), log(rows)).unwrap();
        let shown = recent(dir.path(), 5);
        assert_eq!(shown.len(), 2);
        assert!(shown.iter().all(|row| row.action_count == Some(1)));
    }

    #[test]
    fn personal_input_at_local_midnight_stays_separate_across_rotation() {
        if !crate::test_support::in_timezone(
            "activity_log::tests::personal_input_at_local_midnight_stays_separate_across_rotation",
            "Asia/Tbilisi",
        ) {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let row = |at: u64| {
            let mut row = persons(0);
            row["at"] = json!((u128::from(at) * 1_000_000).to_string());
            row
        };
        // Older builds wrote individual inputs; both stages must preserve the day.
        fs::write(
            dir.path().join(PREVIOUS),
            log([row(71_999_998), row(71_999_999)]),
        )
        .unwrap();
        fs::write(
            dir.path().join(FILE),
            log([row(72_000_000), row(72_000_001)]),
        )
        .unwrap();
        for limit in [1, 5] {
            let shown = recent(dir.path(), limit);
            assert_eq!(shown.len(), limit.min(2));
            assert_eq!(shown[0].at_ms(), Some(72_000_001));
            assert!(shown.iter().all(|row| row.action_count == Some(2)));
            if limit > 1 {
                assert_eq!(shown[1].at_ms(), Some(71_999_999));
            }
        }
    }

    #[test]
    fn continuous_personal_input_compares_adjacent_times_across_rotation() {
        let dir = tempfile::tempdir().unwrap();
        let row = |at: u64| {
            let mut row = persons(0);
            row["at"] = json!((u128::from(at) * 1_000_000).to_string());
            row
        };
        fs::write(dir.path().join(PREVIOUS), log([row(0), row(INPUT_GAP_MS)])).unwrap();
        fs::write(
            dir.path().join(FILE),
            log([row(2 * INPUT_GAP_MS), row(3 * INPUT_GAP_MS)]),
        )
        .unwrap();
        let shown = recent(dir.path(), 1);
        assert_eq!(shown.len(), 1);
        assert_eq!(shown[0].action_count, Some(4));
        assert_eq!(shown[0].at_ms(), Some(3 * INPUT_GAP_MS));
    }

    #[test]
    fn the_last_selected_input_run_counts_every_action_across_chunks_and_rotation() {
        let dir = tempfile::tempdir().unwrap();
        let count = 2_000;
        let current = log((count..2 * count).map(persons));
        assert!(current.len() as u64 > TAIL);
        fs::write(dir.path().join(FILE), current).unwrap();
        fs::write(
            dir.path().join(PREVIOUS),
            log([agents(0)].into_iter().chain((1..count).map(persons))),
        )
        .unwrap();

        let one = recent(dir.path(), 1);
        let several = recent(dir.path(), 5);
        assert_eq!(one.len(), 1);
        assert_eq!(one[0].action_count, Some(2 * count - 1));
        assert_eq!(one[0].action_count, several[0].action_count);
        assert_eq!(several.len(), 2);
    }

    #[test]
    fn completing_the_last_input_run_stays_within_the_byte_budget() {
        let dir = tempfile::tempdir().unwrap();
        let line = log([persons(1)]);
        fs::write(
            dir.path().join(FILE),
            line.repeat((READ_LIMIT as usize / line.len()) + 2),
        )
        .unwrap();
        let mut room = READ_LIMIT;
        let mut rows = Vec::new();
        read_back(
            fs::File::open(dir.path().join(FILE)).unwrap(),
            &mut room,
            &mut rows,
        )
        .unwrap();
        assert_eq!(room, 0);
        let expected = ((READ_LIMIT - 1) / line.len() as u64) as usize;
        assert_eq!(rows.len(), expected);
        assert_eq!(recent(dir.path(), 1)[0].action_count, Some(expected as u64));
        assert_eq!(
            try_recent(dir.path(), 1).unwrap()[0].action_count,
            Some(expected as u64)
        );
    }

    #[test]
    fn a_long_run_of_the_persons_input_past_the_first_tail_still_finds_the_agents_rows_before_it() {
        let dir = tempfile::tempdir().unwrap();
        let run = 20_000;
        let rows = (0..3).map(agents).chain((10..10 + run).map(persons));
        let text = log(rows);
        assert!(text.len() as u64 > 10 * TAIL && (text.len() as u64) < READ_LIMIT);
        fs::write(dir.path().join(FILE), text).unwrap();

        let shown = recent(dir.path(), 200);
        assert_eq!(shown.len(), 4);
        assert_eq!(shown[0].action_count, Some(run));
        assert_eq!(
            shown[1..]
                .iter()
                .map(|row| row.at.as_deref().unwrap())
                .collect::<Vec<_>>(),
            ["2", "1", "0"]
        );
    }

    #[test]
    fn the_read_never_goes_past_its_byte_cap() {
        let dir = tempfile::tempdir().unwrap();
        let filler = format!("{{\"pad\":\"{}\"}}\n", "x".repeat(1000));
        let lines = READ_LIMIT as usize / filler.len() + 1;
        let text = format!(
            "{}{}{}",
            log([agents(0)]),
            filler.repeat(lines),
            log([agents(9)])
        );
        assert!(text.len() as u64 > READ_LIMIT);
        fs::write(dir.path().join(FILE), &text).unwrap();

        let shown = recent(dir.path(), 5);
        assert_eq!(
            shown
                .iter()
                .map(|row| row.at.as_deref().unwrap())
                .collect::<Vec<_>>(),
            ["9"],
            "the row past the cap is never reached"
        );

        let near = format!(
            "{}{}{}",
            log([agents(0)]),
            filler.repeat(lines / 2),
            log([agents(9)])
        );
        fs::write(dir.path().join(FILE), near).unwrap();
        assert_eq!(recent(dir.path(), 5).len(), 2, "one within it is");
    }

    /// The `at` of each of `rows`, in order.
    fn times(rows: &[Row]) -> Vec<&str> {
        rows.iter()
            .map(|row| row.at.as_deref().unwrap_or_default())
            .collect()
    }

    /// A line that names no operation, about `bytes` long, to fill a log.
    fn filler(bytes: usize) -> String {
        format!("{{\"pad\":\"{}\"}}\n", "x".repeat(bytes))
    }

    #[test]
    fn right_after_the_log_rotates_the_rows_before_it_still_read() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(PREVIOUS), log((0..5).map(agents))).unwrap();
        fs::write(dir.path().join(FILE), log((10..12).map(agents))).unwrap();

        assert_eq!(
            times(&recent(dir.path(), 5)),
            ["11", "10", "4", "3", "2"],
            "the new log's rows, then the previous log's newest"
        );
        assert_eq!(
            times(&recent(dir.path(), 2)),
            ["11", "10"],
            "the limit applies after both logs are read"
        );

        fs::remove_file(dir.path().join(FILE)).unwrap();
        assert_eq!(
            times(&recent(dir.path(), 2)),
            ["4", "3"],
            "nor is it lost in the moment before the new log is made"
        );
    }

    #[test]
    fn a_command_or_a_run_of_the_persons_input_across_the_rotation_is_one_row() {
        let dir = tempfile::tempdir().unwrap();
        let step = |at: u64, ok: bool| {
            json!({"at": at.to_string(), "operation": "input", "ok": ok, "verb": "type",
                "command": "typing"})
        };
        fs::write(dir.path().join(PREVIOUS), log([agents(0), step(1, true)])).unwrap();
        fs::write(dir.path().join(FILE), log([step(2, false)])).unwrap();
        assert_eq!(
            times(&recent(dir.path(), 5)),
            ["2", "0"],
            "the newest step stands for its command"
        );

        fs::write(
            dir.path().join(PREVIOUS),
            log([agents(0), persons(1), persons(2)]),
        )
        .unwrap();
        fs::write(dir.path().join(FILE), log([persons(3)])).unwrap();
        let shown = recent(dir.path(), 5);
        assert_eq!(times(&shown), ["3", "0"]);
        assert_eq!(
            shown[0].action_count,
            Some(3),
            "the run counts every action"
        );
    }

    #[test]
    fn the_previous_log_is_read_within_the_same_byte_cap() {
        let dir = tempfile::tempdir().unwrap();
        let mib = 1024 * 1024;
        let current = format!(
            "{}{}{}",
            log([agents(5)]),
            filler(3 * mib),
            log([agents(9)])
        );
        let previous = format!(
            "{}{}{}",
            log([agents(0)]),
            filler(2 * mib),
            log([agents(1)])
        );
        assert!((current.len() as u64) < READ_LIMIT);
        assert!(((current.len() + previous.len()) as u64) > READ_LIMIT);
        fs::write(dir.path().join(FILE), current).unwrap();
        fs::write(dir.path().join(PREVIOUS), previous).unwrap();

        assert_eq!(
            times(&recent(dir.path(), 5)),
            ["9", "5", "1"],
            "the previous log's oldest row is past what one read goes through"
        );
    }

    #[test]
    fn the_previous_log_is_never_read_twice_or_through_a_link() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(FILE), log((0..3).map(agents))).unwrap();
        fs::hard_link(dir.path().join(FILE), dir.path().join(PREVIOUS)).unwrap();
        assert_eq!(
            times(&recent(dir.path(), 10)),
            ["2", "1", "0"],
            "the same file under both names, as when the log rotates mid-read"
        );

        fs::remove_file(dir.path().join(PREVIOUS)).unwrap();
        let elsewhere = dir.path().join("elsewhere.jsonl");
        fs::write(&elsewhere, log([agents(7)])).unwrap();
        std::os::unix::fs::symlink(&elsewhere, dir.path().join(PREVIOUS)).unwrap();
        assert_eq!(times(&recent(dir.path(), 10)), ["2", "1", "0"]);
    }

    #[test]
    fn a_row_reads_back_as_the_bytes_it_was_written_as() {
        let line = r#"{"actionCount":2,"actor":"agent","app":"TextEdit","at":"5","code":"focus-changed","command":"c","elapsedMs":1.5,"ok":false,"operation":"input","partial":true,"reason":"window","session":"0123456789ab","started":true,"task":"ba9876543210","verb":"type"}"#;
        let row = Row::parse(&serde_json::from_str(line).unwrap()).unwrap();
        assert_eq!(serde_json::to_value(&row).unwrap().to_string(), line);
    }

    #[test]
    fn a_rows_bundle_id_reads_back_as_written_and_a_row_without_one_still_reads() {
        let line = r#"{"actionCount":1,"actor":"agent","app":"TextEdit","at":"5","bundleId":"com.apple.TextEdit","elapsedMs":2.0,"ok":true,"operation":"input","verb":"click"}"#;
        let row = Row::parse(&serde_json::from_str(line).unwrap()).unwrap();
        assert_eq!(row.bundle_id.as_deref(), Some("com.apple.TextEdit"));
        assert_eq!(serde_json::to_value(&row).unwrap().to_string(), line);

        let older = r#"{"actionCount":1,"actor":"agent","app":"TextEdit","at":"5","elapsedMs":2.0,"ok":true,"operation":"input","verb":"click"}"#;
        let row = Row::parse(&serde_json::from_str(older).unwrap()).unwrap();
        assert_eq!(row.bundle_id, None);
        assert_eq!(
            serde_json::to_value(&row).unwrap().to_string(),
            older,
            "a row with none is written as before"
        );
        let odd = Row::parse(&json!({"operation": "input", "bundleId": 7})).unwrap();
        assert_eq!(odd.bundle_id, None);
    }

    #[test]
    fn a_field_an_older_build_wrote_otherwise_reads_as_missing_and_an_unknown_op_is_kept() {
        let row = Row::parse(
            &json!({"operation": "later", "ok": "yes", "actionCount": -1,
            "verb": 3, "app": "Notes", "more": {"x": 1}}),
        )
        .unwrap();
        assert_eq!(row.operation, Some(Op::Other("later".into())));
        assert_eq!((row.ok, row.action_count, &row.verb), (None, None, &None));
        assert_eq!(row.app.as_deref(), Some("Notes"));
        assert_eq!(
            serde_json::to_value(&row).unwrap()["operation"],
            "later",
            "kept as it was written"
        );
        assert!(Row::parse(&json!({"operation": 7})).is_none());
        assert!(Row::parse(&json!(["input"])).is_none());
    }

    #[test]
    fn a_rows_age_and_time_read_from_its_nanoseconds() {
        let row = Row::parse(&json!({"operation": "run", "at": "5000000000"})).unwrap();
        assert_eq!(row.age(7_000_000_000), Some(2_000_000_000));
        assert_eq!(row.age(1), Some(0));
        assert_eq!(row.at_ms(), Some(5000));
        let undated = Row::parse(&json!({"operation": "run", "at": "soon"})).unwrap();
        assert_eq!((undated.age(1), undated.at_ms()), (None, None));
    }
}

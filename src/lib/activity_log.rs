//! The activity log: one row for each step of a request that changes the
//! helper, appended to [`FILE`] in the output folder and read back from its
//! tail. A row keeps only what [`crate::doing::Doing`] names and fixed
//! codes: never typed text, argv, coordinates, window titles, output or
//! image payloads.
use crate::error::{Code, Error, Result};
use fs2::FileExt;
use serde::{Deserialize, Deserializer, Serialize, de::DeserializeOwned};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    os::unix::fs::{MetadataExt, OpenOptionsExt},
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
/// How much of the end of the log is read at least, and as much more as a
/// row takes at most for each row asked for past that.
const TAIL: u64 = 64 * 1024;
const ROW_BYTES: u64 = 512;
/// The most of the log one read goes through, from its end.
const READ_LIMIT: u64 = 4 * 1024 * 1024;

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
    /// When it was written, in nanoseconds since 1970, as text.
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
}

impl Row {
    /// `value` as a row, with its id, when it names its operation.
    pub fn parse(value: &Value) -> Option<Self> {
        let mut row: Self = serde_json::from_value(value.clone()).ok()?;
        row.operation.as_ref()?;
        row.id = id(value);
        Some(row)
    }

    /// Nanoseconds since it was written, at `now`, when its time reads.
    pub fn age(&self, now: u128) -> Option<u128> {
        self.at_nanos().map(|at| now.saturating_sub(at))
    }

    /// When it was written, in milliseconds since 1970.
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
/// What a row holds never changes once written, though, and no two rows
/// hold the same: each carries the nanosecond it was written at (taken
/// under the log's exclusive lock, so writers take turns) and, for an
/// agent's command, that command's random token. So the id is a digest of
/// the row itself. A command's newest step stands for it, so the row shown
/// for a command under way is another one, with another id, once it ends.
fn id(value: &Value) -> String {
    crate::name::hex(&Sha256::digest(value.to_string().as_bytes())[..8])
}

/// Append `row` to the log in `directory`, under the log's lock, starting
/// the log over once it has grown past [`ROTATE_AT`]. It waits its turn
/// behind other writers, so it runs off any async runtime.
pub(crate) fn append(directory: &Path, row: &Row) -> Result<()> {
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
    if fs::symlink_metadata(&path).is_ok_and(|m| m.is_file() && m.len() > ROTATE_AT) {
        fs::rename(&path, directory.join(PREVIOUS))?;
    }

    let mut file = OpenOptions::new()
        .create(true)
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

    // Through a value, so its keys are written in the order the log has
    // always kept them.
    let line =
        serde_json::to_value(row).map_err(|error| Error::new(Code::State, error.to_string()))?;
    writeln!(file, "{line}")?;
    Ok(())
}

/// One row per command from the log in `directory`, newest first, at most
/// `limit`. A command that acted in several steps (typing in batches,
/// opening then waiting) is its newest step, and a run of the person's
/// input through a viewer is its newest row, counting every action of the
/// run. A line that does not read, or names no operation, is passed over.
///
/// The log is read from its end, a [`TAIL`] at a time, until `limit` rows
/// are found or [`READ_LIMIT`] bytes were read: it is read often, so never
/// more than that, however long it is. When it runs out first, as just
/// after it starts over, the reading goes on into [`PREVIOUS`] within the
/// same limit.
pub fn recent(directory: &Path, limit: usize) -> Vec<Row> {
    if limit == 0 {
        return Vec::new();
    }

    // Never through a link, and never waiting on a pipe put in its place.
    // Both are opened before either is read, and the previous log is passed
    // over when it is the same file, as when the log started over between
    // the two: so no row is read twice.
    let current = crate::files::open_plain(&directory.join(FILE));
    let previous = crate::files::open_plain(&directory.join(PREVIOUS)).filter(|previous| {
        current
            .as_ref()
            .is_none_or(|current| !same_file(current, previous))
    });

    let mut rows = Vec::new();
    let mut room = READ_LIMIT;
    for file in [current, previous].into_iter().flatten() {
        if rows.len() >= limit || room == 0 {
            break;
        }
        read_back(file, limit, &mut room, &mut rows);
    }

    rows.truncate(limit);
    rows
}

/// Whether `one` and `other` are the same file.
fn same_file(one: &fs::File, other: &fs::File) -> bool {
    match (one.metadata(), other.metadata()) {
        (Ok(one), Ok(other)) => (one.dev(), one.ino()) == (other.dev(), other.ino()),
        // Unknown, so it is passed over: a row is never read twice.
        _ => true,
    }
}

/// Add the rows of `file`, read from its end a [`TAIL`] at a time, to
/// `rows`, older than those already there, until they number `limit` or
/// `room` bytes are spent.
fn read_back(mut file: fs::File, limit: usize, room: &mut u64, rows: &mut Vec<Row>) {
    let wanted = limit.saturating_sub(rows.len()) as u64;
    let mut chunk = TAIL.max(ROW_BYTES.saturating_mul(wanted));
    let mut end = file.metadata().map(|m| m.len()).unwrap_or(0);
    // The start of the oldest line read so far, whose beginning is in the
    // part of the file before it.
    let mut partial = Vec::new();

    while end > 0 && *room > 0 && rows.len() < limit {
        let size = chunk.min(end).min(*room);
        let start = end - size;
        let mut bytes = Vec::new();
        if file.seek(SeekFrom::Start(start)).is_err()
            || (&mut file).take(size).read_to_end(&mut bytes).is_err()
        {
            break;
        }
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

        for line in bytes[whole..].split(|byte| *byte == b'\n').rev() {
            let Some(row) = serde_json::from_slice::<Value>(line)
                .ok()
                .and_then(|value| Row::parse(&value))
            else {
                continue;
            };
            if !gathered(rows, row) && rows.len() == limit {
                break;
            }
        }

        end = start;
        *room -= size;
        chunk = TAIL;
    }
}

/// Add `row`, older than those in `rows`, to them: as a step of the
/// command or the run of the person's input the newest of them stands for,
/// when it is one, adding its actions to the run's; whether it was.
fn gathered(rows: &mut Vec<Row>, row: Row) -> bool {
    let Some(newer) = rows.last_mut() else {
        rows.push(row);
        return false;
    };

    if row.command.is_some() && newer.command == row.command {
        return true;
    }
    if row.persons_input()
        && newer.persons_input()
        && (row.ok, &row.code) == (newer.ok, &newer.code)
    {
        newer.action_count = match (newer.action_count, row.action_count) {
            (None, None) => None,
            (newer, older) => Some(newer.unwrap_or(0) + older.unwrap_or(0)),
        };
        return true;
    }

    rows.push(row);
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::{fs, io::Write};

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
            "the previous log is read only when the new one runs out"
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

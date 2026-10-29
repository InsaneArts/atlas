//! Protocol v1 requests the helper daemon accepts, validated before anything
//! runs. Refusals keep their exact wording: clients and the live checks
//! match on it.
use crate::host_session::Key;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::time::Duration;

/// Longest request line, in bytes.
pub const MAX_REQUEST: usize = 1024 * 1024;
const MAX_ARGUMENTS: usize = 4096;
const MAX_ARGUMENT_BYTES: usize = 256 * 1024;
const MAX_TIMEOUT_MS: u64 = 10 * 60 * 1000;
const DEFAULT_TIMEOUT_MS: u64 = 30_000;
const MAX_DIMENSION: u64 = 8192;

/// Farthest a region's top-left corner may lie from the display's, and its
/// longest side, in display points.
const MAX_POINTS: i32 = 16384;

/// The longest a background job may run, and the shortest.
pub const JOB_LIFETIME_MAX: Duration = Duration::from_secs(24 * 60 * 60);
const JOB_LIFETIME_MIN: Duration = Duration::from_secs(1);
/// The longest a stop waits between TERM and KILL, and the shortest.
pub const JOB_GRACE_MAX: Duration = Duration::from_secs(60);
const JOB_GRACE_MIN: Duration = Duration::from_secs(1);
/// How long a stop waits between TERM and KILL unless told.
pub const JOB_GRACE: Duration = Duration::from_secs(5);
/// The most of a job's log one request reads.
pub const JOB_LOG_MAX: usize = 1024 * 1024;
/// The most characters a job's name keeps.
pub const JOB_NAME_MAX: usize = 64;

/// The largest part of a file one read returns.
pub const READ_RANGE_MAX: u64 = 8 * 1024 * 1024;
/// The largest file a read accepts.
pub const READ_SIZE_MAX: u64 = 1 << 30;
/// The longest path a read takes, in bytes.
pub const READ_PATH_MAX: usize = 1024;

/// A rectangle of the display in whole points: its top-left corner and size.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Region {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl Region {
    /// The region, if it lies within the protocol's bounds.
    fn new(x: i64, y: i64, width: i64, height: i64) -> Option<Self> {
        let corner = -i64::from(MAX_POINTS)..=i64::from(MAX_POINTS);
        let side = 1..=i64::from(MAX_POINTS);
        (corner.contains(&x)
            && corner.contains(&y)
            && side.contains(&width)
            && side.contains(&height))
        .then_some(Self {
            x: x as i32,
            y: y as i32,
            width: width as i32,
            height: height as i32,
        })
    }

    /// The smallest whole-point region holding x, y, width, height (floor
    /// the corner, ceil the far edges); None outside the protocol's bounds.
    pub fn from_points(x: f64, y: f64, width: f64, height: f64) -> Option<Self> {
        if ![x, y, width, height].iter().all(|n| n.is_finite()) {
            return None;
        }

        let whole = |n: f64| (n.abs() <= f64::from(MAX_POINTS) * 4.0).then_some(n as i64);
        let (left, top) = (whole(x.floor())?, whole(y.floor())?);
        let (right, bottom) = (whole((x + width).ceil())?, whole((y + height).ceil())?);

        Self::new(left, top, right - left, bottom - top)
    }
}

/// What a screenshot shows instead of the whole display.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Crop {
    /// Every on-screen window of this process.
    App {
        pid: i64,
    },
    Region(Region),
}

/// The daemon capability to refuse a request meant for another session of
/// the helper, as its hello lists it.
pub const SESSION_PIN: &str = "session-pin";

/// The helper session a request was verified in: the daemon refuses it in
/// any other, so a helper that logged in again is never read as the one
/// that was checked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SessionPin {
    pub security_session_id: u64,
    pub display_id: u64,
}

/// The daemon capability to keep background jobs, as its hello lists it.
pub const JOBS: &str = "jobs";

/// The daemon capability to read files for the person's side to collect.
pub const READ: &str = "read";

/// A background job's handle: 12 lowercase hexadecimal digits, drawn at
/// random when it starts, so it says nothing of the job.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct JobId(String);

impl JobId {
    /// How many hexadecimal digits a job's ID has.
    pub const LEN: usize = 12;

    /// A new random ID.
    pub fn new() -> Self {
        let mut id = crate::control::token();
        id.truncate(Self::LEN);
        Self(id)
    }

    /// `text` as a job's ID, when it is exactly [`JobId::LEN`] lowercase
    /// hexadecimal digits.
    pub fn parse(text: &str) -> Option<Self> {
        crate::digest::lower_hex(text, Self::LEN).then(|| Self(text.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for JobId {
    fn default() -> Self {
        Self::new()
    }
}

impl TryFrom<String> for JobId {
    type Error = String;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        Self::parse(&text).ok_or_else(|| format!("{text} is not a job's ID"))
    }
}

impl From<JobId> for String {
    fn from(id: JobId) -> Self {
        id.0
    }
}

impl std::fmt::Display for JobId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// What the person's side knows a job to be: the boot it started in, its
/// leader's PID and exact start time, and its process group. A record a
/// later build wrote may hold more beside it; a request names exactly
/// these.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Expected {
    pub boot: String,
    pub pid: i32,
    pub start: [u64; 2],
    pub pgid: i32,
}

/// Why every job is being stopped at once.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StopReason {
    /// The person stopped agents.
    Person,
    /// The helper came on screen.
    OnConsole,
    /// Atlas is logging the helper out or being uninstalled.
    Stopped,
}

/// What is asked of the helper's background jobs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JobRequest {
    /// Start `argv` as a job for the helper account `uid` that runs for at
    /// most `lifetime`, named `name`, for `task` of host session `session`.
    Start {
        argv: Vec<String>,
        lifetime: Duration,
        name: Option<String>,
        task: Option<Key>,
        session: Option<Key>,
        uid: u32,
        pin: Option<SessionPin>,
    },
    /// Stop job `id` when it is still what `expect` says, waiting `grace`
    /// between TERM and KILL.
    Stop {
        id: JobId,
        expect: Expected,
        grace: Duration,
    },
    /// Stop every job, by the daemon's own records alone.
    StopAll { reason: StopReason },
    /// One job, or every one.
    Show { id: Option<JobId> },
    /// The last `bytes` of a job's output.
    Log { id: JobId, bytes: usize },
}

/// What a file is, as `fstat` of its open descriptor says: the same path
/// with any of these changed is another file, or one being written.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileIdentity {
    pub dev: u64,
    pub ino: u64,
    pub size: u64,
    /// Its modification and change times, in seconds and nanoseconds.
    pub mtime: [i64; 2],
    pub ctime: [i64; 2],
}

/// Part of a regular file in the helper, or its digest, for the person's
/// side to collect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadRequest {
    /// As given: relative to the helper's home, or absolute.
    pub path: String,
    pub offset: u64,
    /// At most [`READ_RANGE_MAX`]; 0 with `digest`.
    pub length: u64,
    /// What the first part found the file to be; none for the first part.
    pub expect: Option<FileIdentity>,
    /// Hash the whole file instead of returning a part.
    pub digest: bool,
    /// The largest file accepted.
    pub max_size: u64,
    pub pin: Option<SessionPin>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    Hello,
    Screenshot {
        max_dimension: Option<u32>,
        /// At most one pixel per display point of the area captured, too.
        per_point: bool,
        crop: Option<Crop>,
    },
    Apps {
        pin: Option<SessionPin>,
    },
    Run {
        argv: Vec<String>,
        timeout: Duration,
        pin: Option<SessionPin>,
    },
    Restart,
    /// Log this helper out; `force` ends its session without asking its apps.
    Logout {
        force: bool,
    },
    /// Background jobs: start, stop, stop every one, show, read a log.
    Job(JobRequest),
    /// Part of a file in the helper, or its digest.
    Read(ReadRequest),
}

/// A refusal in the shape every Atlas failure has, so a reader needs one
/// parser. Nothing here is of unknown outcome: the request never ran.
pub fn failure(code: &str, error: impl Into<String>) -> Value {
    json!({"ok": false, "code": code, "error": error.into(), "uncertain": false})
}

fn bad(error: impl Into<String>) -> Value {
    failure("bad-request", error)
}

/// Parse one request line, or the reply that refuses it.
pub fn parse(line: &[u8]) -> Result<Request, Value> {
    if line.len() > MAX_REQUEST {
        return Err(bad("request exceeds 1 MiB"));
    }

    let request: Value =
        serde_json::from_slice(line).map_err(|_| bad("request is not a JSON object"))?;
    if !request.is_object() {
        return Err(bad("request is not a JSON object"));
    }

    let op = request["op"]
        .as_str()
        .ok_or_else(|| bad("request is missing an \"op\" string"))?;
    match op {
        "hello" => Ok(Request::Hello),
        "apps" => Ok(Request::Apps {
            pin: pin(&request)?,
        }),
        "restart" => Ok(Request::Restart),
        "logout" => Ok(Request::Logout {
            force: match &request["force"] {
                Value::Null => false,
                value => value
                    .as_bool()
                    .ok_or_else(|| bad("\"force\" must be true or false"))?,
            },
        }),
        "screenshot" => {
            let max_dimension = match &request["maxDimension"] {
                Value::Null => None,
                value => Some(
                    value
                        .as_u64()
                        .filter(|n| (1..=MAX_DIMENSION).contains(n))
                        .and_then(|n| u32::try_from(n).ok())
                        .ok_or_else(|| bad("\"maxDimension\" must be 1–8192"))?,
                ),
            };

            let per_point = match &request["perPoint"] {
                Value::Null => false,
                value => value
                    .as_bool()
                    .ok_or_else(|| bad("\"perPoint\" must be true or false"))?,
            };

            Ok(Request::Screenshot {
                max_dimension,
                per_point,
                crop: crop(&request)?,
            })
        }
        "run" => {
            let argv = request["argv"]
                .as_array()
                .filter(|argv| !argv.is_empty())
                .ok_or_else(|| bad("run requires a non-empty \"argv\" array"))?;
            if argv.len() > MAX_ARGUMENTS {
                return Err(bad("run accepts at most 4096 arguments"));
            }

            let argv = argv
                .iter()
                .map(|argument| {
                    argument
                        .as_str()
                        .filter(|text| text.len() <= MAX_ARGUMENT_BYTES && !text.contains('\0'))
                        .map(str::to_owned)
                        .ok_or_else(|| bad("run arguments must be strings without NUL"))
                })
                .collect::<Result<Vec<_>, _>>()?;
            if argv[0].is_empty() {
                return Err(bad("run requires a non-empty \"argv\" array"));
            }

            let timeout = match &request["timeoutMs"] {
                Value::Null => DEFAULT_TIMEOUT_MS,
                value => value
                    .as_u64()
                    .filter(|n| (1..=MAX_TIMEOUT_MS).contains(n))
                    .ok_or_else(|| bad("\"timeoutMs\" must be 1–600000"))?,
            };

            Ok(Request::Run {
                argv,
                timeout: Duration::from_millis(timeout),
                pin: pin(&request)?,
            })
        }
        "job" => job(&request).map(Request::Job),
        "read" => read(&request).map(Request::Read),
        other => Err(bad(format!("unknown op '{other}'"))),
    }
}

/// A job start's program and its arguments, bounded as `run`'s are.
fn job_argv(request: &Value) -> Result<Vec<String>, Value> {
    let empty = || bad("job start requires a non-empty \"argv\" array");
    let argv = request["argv"]
        .as_array()
        .filter(|argv| !argv.is_empty())
        .ok_or_else(empty)?;
    if argv.len() > MAX_ARGUMENTS {
        return Err(bad("job start accepts at most 4096 arguments"));
    }

    let argv = argv
        .iter()
        .map(|argument| {
            argument
                .as_str()
                .filter(|text| text.len() <= MAX_ARGUMENT_BYTES && !text.contains('\0'))
                .map(str::to_owned)
                .ok_or_else(|| bad("job start arguments must be strings without NUL"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    if argv[0].is_empty() {
        return Err(empty());
    }
    Ok(argv)
}

/// `request[field]` as milliseconds from `low` to `high`, or `default`
/// when it is absent and there is one.
fn millis(
    request: &Value,
    field: &str,
    (low, high): (Duration, Duration),
    default: Option<Duration>,
) -> Result<Duration, Value> {
    let (low, high) = (low.as_millis() as u64, high.as_millis() as u64);
    match (&request[field], default) {
        (Value::Null, Some(default)) => Ok(default),
        (value, _) => value
            .as_u64()
            .filter(|n| (low..=high).contains(n))
            .map(Duration::from_millis)
            .ok_or_else(|| bad(format!("\"{field}\" must be {low}–{high}"))),
    }
}

/// `request["id"]` as a job's ID.
fn job_id(request: &Value) -> Result<JobId, Value> {
    request["id"]
        .as_str()
        .and_then(JobId::parse)
        .ok_or_else(|| bad("\"id\" must be a job's ID: 12 lowercase hex digits"))
}

/// `request[field]` as a session or task key, when it is given.
fn key(request: &Value, field: &str) -> Result<Option<Key>, Value> {
    match &request[field] {
        Value::Null => Ok(None),
        value => value
            .as_str()
            .and_then(Key::parse)
            .map(Some)
            .ok_or_else(|| bad(format!("\"{field}\" must be 12 lowercase hex digits"))),
    }
}

/// Whether `name` may name a job: 1 to [`JOB_NAME_MAX`] characters, none
/// of them a control character.
pub fn job_name(name: &str) -> bool {
    let count = name.chars().count();
    (1..=JOB_NAME_MAX).contains(&count) && !name.chars().any(char::is_control)
}

/// A `job` request, by its `action`.
fn job(request: &Value) -> Result<JobRequest, Value> {
    let action = request["action"]
        .as_str()
        .ok_or_else(|| bad("job requires an \"action\" string"))?;
    match action {
        "start" => {
            let name = match &request["name"] {
                Value::Null => None,
                value => Some(
                    value
                        .as_str()
                        .filter(|name| job_name(name))
                        .map(str::to_owned)
                        .ok_or_else(|| {
                            bad("\"name\" must be 1–64 characters, none of them a control character")
                        })?,
                ),
            };
            let uid = request["uid"]
                .as_u64()
                .and_then(|uid| u32::try_from(uid).ok())
                .ok_or_else(|| bad("job start requires the helper's \"uid\""))?;
            Ok(JobRequest::Start {
                argv: job_argv(request)?,
                lifetime: millis(
                    request,
                    "lifetimeMs",
                    (JOB_LIFETIME_MIN, JOB_LIFETIME_MAX),
                    None,
                )?,
                name,
                task: key(request, "task")?,
                session: key(request, "hostSession")?,
                uid,
                pin: pin(request)?,
            })
        }
        "stop" => Ok(JobRequest::Stop {
            id: job_id(request)?,
            expect: request["expect"]
                .as_object()
                .filter(|expect| {
                    expect.len() == 4
                        && ["boot", "pid", "start", "pgid"]
                            .iter()
                            .all(|field| expect.contains_key(*field))
                })
                .and_then(|_| Expected::deserialize(&request["expect"]).ok())
                .ok_or_else(|| {
                    bad("\"expect\" must name the job's boot, pid, start and pgid, and nothing else")
                })?,
            grace: millis(
                request,
                "graceMs",
                (JOB_GRACE_MIN, JOB_GRACE_MAX),
                Some(JOB_GRACE),
            )?,
        }),
        "stop-all" => Ok(JobRequest::StopAll {
            reason: StopReason::deserialize(&request["reason"])
                .map_err(|_| bad("\"reason\" must be person, on-console or stopped"))?,
        }),
        "show" => Ok(JobRequest::Show {
            id: match &request["id"] {
                Value::Null => None,
                _ => Some(job_id(request)?),
            },
        }),
        "log" => Ok(JobRequest::Log {
            id: job_id(request)?,
            bytes: request["bytes"]
                .as_u64()
                .and_then(|bytes| usize::try_from(bytes).ok())
                .filter(|bytes| (1..=JOB_LOG_MAX).contains(bytes))
                .ok_or_else(|| bad("\"bytes\" must be 1–1048576"))?,
        }),
        other => Err(bad(format!("unknown job action '{other}'"))),
    }
}

/// A `read` request.
fn read(request: &Value) -> Result<ReadRequest, Value> {
    let path = request["path"]
        .as_str()
        .filter(|path| !path.is_empty() && path.len() <= READ_PATH_MAX && !path.contains('\0'))
        .ok_or_else(|| bad("read requires a \"path\" of 1–1024 bytes without NUL"))?;
    let number = |field: &str| match &request[field] {
        Value::Null => Ok(0),
        value => value
            .as_u64()
            .ok_or_else(|| bad(format!("\"{field}\" must be a whole number"))),
    };
    let offset = number("offset")?;
    let length = number("length")?;
    if length > READ_RANGE_MAX {
        return Err(bad("\"length\" must be at most 8388608"));
    }
    let digest = match &request["digest"] {
        Value::Null => false,
        value => value
            .as_bool()
            .ok_or_else(|| bad("\"digest\" must be true or false"))?,
    };
    if digest && length != 0 {
        return Err(bad("A digest reads the whole file: give \"length\" 0"));
    }
    let max_size = request["maxSize"]
        .as_u64()
        .filter(|size| (1..=READ_SIZE_MAX).contains(size))
        .ok_or_else(|| bad("\"maxSize\" must be 1–1073741824"))?;
    let expect = match &request["expect"] {
        Value::Null => None,
        value => Some(FileIdentity::deserialize(value).map_err(|_| {
            bad("\"expect\" must be a file's dev, ino, size, mtime and ctime, and nothing else")
        })?),
    };
    Ok(ReadRequest {
        path: path.to_owned(),
        offset,
        length,
        expect,
        digest,
        max_size,
        pin: pin(request)?,
    })
}

/// A request's `session`, the helper session it must be served in.
fn pin(request: &Value) -> Result<Option<SessionPin>, Value> {
    match &request["session"] {
        Value::Null => Ok(None),
        value => SessionPin::deserialize(value)
            .map(Some)
            .map_err(|_| bad("\"session\" must name a securitySessionId and a displayId")),
    }
}

/// A screenshot request's `pid` or `region`, at most one of them.
fn crop(request: &Value) -> Result<Option<Crop>, Value> {
    match (&request["pid"], &request["region"]) {
        (Value::Null, Value::Null) => Ok(None),
        (pid, Value::Null) => pid
            .as_i64()
            .filter(|pid| (1..=i64::from(i32::MAX)).contains(pid))
            .map(|pid| Some(Crop::App { pid }))
            .ok_or_else(|| bad("\"pid\" must be a process ID")),
        (Value::Null, region) => {
            let side = |key: &str| region.get(key).and_then(Value::as_i64);
            side("x")
                .zip(side("y"))
                .zip(side("width").zip(side("height")))
                .and_then(|((x, y), (width, height))| Region::new(x, y, width, height))
                .map(|region| Some(Crop::Region(region)))
                .ok_or_else(|| {
                    bad("\"region\" must be whole points: x and y -16384–16384, width and height 1–16384")
                })
        }
        _ => Err(bad("give \"pid\" or \"region\", not both")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn refused(line: &str) -> Value {
        parse(line.as_bytes()).unwrap_err()
    }

    #[test]
    fn requests_parse_into_operations() {
        assert_eq!(parse(br#"{"op":"hello"}"#), Ok(Request::Hello));
        assert_eq!(parse(br#"{"op":"apps"}"#), Ok(Request::Apps { pin: None }));
        assert_eq!(parse(br#"{"op":"restart"}"#), Ok(Request::Restart));
        assert_eq!(
            parse(br#"{"op":"logout"}"#),
            Ok(Request::Logout { force: false })
        );
        assert_eq!(
            parse(br#"{"op":"logout","force":true}"#),
            Ok(Request::Logout { force: true })
        );
        assert_eq!(
            refused(r#"{"op":"logout","force":"yes"}"#)["error"],
            "\"force\" must be true or false"
        );
        assert_eq!(
            parse(br#"{"op":"screenshot","maxDimension":320}"#),
            Ok(Request::Screenshot {
                max_dimension: Some(320),
                per_point: false,
                crop: None
            })
        );
        assert_eq!(
            parse(br#"{"op":"screenshot"}"#),
            Ok(Request::Screenshot {
                max_dimension: None,
                per_point: false,
                crop: None
            })
        );
        assert_eq!(
            parse(br#"{"op":"run","argv":["/bin/echo","hi"],"timeoutMs":3000}"#),
            Ok(Request::Run {
                argv: vec!["/bin/echo".into(), "hi".into()],
                timeout: Duration::from_millis(3000),
                pin: None
            })
        );

        let Ok(Request::Run { timeout, .. }) = parse(br#"{"op":"run","argv":["true"]}"#) else {
            panic!("run without a timeout");
        };

        assert_eq!(timeout, Duration::from_secs(30));
    }

    /// A run or an app listing may name the helper session it was verified
    /// in; anything but its two numbers is refused.
    #[test]
    fn a_read_may_name_the_session_it_is_for() {
        let pin = Some(SessionPin {
            security_session_id: 42,
            display_id: 7,
        });
        assert_eq!(
            parse(br#"{"op":"apps","session":{"securitySessionId":42,"displayId":7}}"#),
            Ok(Request::Apps { pin })
        );
        let Ok(Request::Run { pin: run, .. }) = parse(
            br#"{"op":"run","argv":["true"],"session":{"securitySessionId":42,"displayId":7}}"#,
        ) else {
            panic!("a pinned run");
        };
        assert_eq!(run, pin);

        for session in [
            r#"{"securitySessionId":42}"#,
            r#"{"securitySessionId":"42","displayId":7}"#,
            r#"{"securitySessionId":42,"displayId":7,"uid":501}"#,
            r#"42"#,
        ] {
            let line = format!(r#"{{"op":"apps","session":{session}}}"#);
            assert_eq!(refused(&line)["code"], "bad-request", "{session}");
        }
    }

    /// The wording clients match on.
    #[test]
    fn refusals_keep_their_wording() {
        assert_eq!(
            refused(r#"{"notAnOp":true}"#),
            json!({"ok": false, "code": "bad-request", "error": "request is missing an \"op\" string", "uncertain": false})
        );
        assert_eq!(
            refused(r#"{"op":"nope"}"#),
            json!({"ok": false, "code": "bad-request", "error": "unknown op 'nope'", "uncertain": false})
        );
        assert_eq!(
            refused(r#"{"op":"run","argv":[],"timeoutMs":3000}"#),
            json!({"ok": false, "code": "bad-request", "error": "run requires a non-empty \"argv\" array", "uncertain": false})
        );
    }

    #[test]
    fn screenshots_parse_an_app_or_a_region() {
        let shot = |crop| {
            Ok(Request::Screenshot {
                max_dimension: None,
                per_point: false,
                crop,
            })
        };

        assert_eq!(
            parse(br#"{"op":"screenshot","pid":4242}"#),
            shot(Some(Crop::App { pid: 4242 }))
        );
        assert_eq!(
            parse(br#"{"op":"screenshot","region":{"x":-100,"y":80,"width":800,"height":600}}"#),
            shot(Some(Crop::Region(Region {
                x: -100,
                y: 80,
                width: 800,
                height: 600
            })))
        );
        assert_eq!(
            parse(br#"{"op":"screenshot","pid":null,"region":null}"#),
            shot(None)
        );
        assert_eq!(
            parse(br#"{"op":"screenshot","pid":7,"maxDimension":400}"#),
            Ok(Request::Screenshot {
                max_dimension: Some(400),
                per_point: false,
                crop: Some(Crop::App { pid: 7 })
            })
        );
        assert_eq!(
            refused(r#"{"op":"screenshot","pid":1,"region":{"x":0,"y":0,"width":1,"height":1}}"#)["error"],
            "give \"pid\" or \"region\", not both"
        );
        assert_eq!(
            refused(r#"{"op":"screenshot","pid":0}"#)["error"],
            "\"pid\" must be a process ID"
        );
        assert_eq!(
            refused(r#"{"op":"screenshot","region":{"x":0,"y":0,"width":0,"height":1}}"#)["error"],
            "\"region\" must be whole points: x and y -16384–16384, width and height 1–16384"
        );
    }

    #[test]
    fn a_screenshot_may_ask_for_at_most_one_pixel_per_point() {
        assert_eq!(
            parse(br#"{"op":"screenshot","maxDimension":1456,"perPoint":true}"#),
            Ok(Request::Screenshot {
                max_dimension: Some(1456),
                per_point: true,
                crop: None
            })
        );
        assert_eq!(
            parse(br#"{"op":"screenshot","pid":7,"perPoint":false}"#),
            Ok(Request::Screenshot {
                max_dimension: None,
                per_point: false,
                crop: Some(Crop::App { pid: 7 })
            })
        );
        assert_eq!(
            parse(br#"{"op":"screenshot","perPoint":null}"#),
            Ok(Request::Screenshot {
                max_dimension: None,
                per_point: false,
                crop: None
            })
        );
        for line in [
            r#"{"op":"screenshot","perPoint":"yes"}"#,
            r#"{"op":"screenshot","perPoint":1}"#,
        ] {
            assert_eq!(
                refused(line)["error"],
                "\"perPoint\" must be true or false",
                "{line}"
            );
        }
    }

    #[test]
    fn regions_widen_to_whole_points() {
        assert_eq!(
            Region::from_points(1.5, 2.25, 3.0, 4.0),
            Some(Region {
                x: 1,
                y: 2,
                width: 4,
                height: 5
            })
        );
        assert_eq!(
            Region::from_points(-50.0, -50.0, 200.0, 200.0),
            Some(Region {
                x: -50,
                y: -50,
                width: 200,
                height: 200
            })
        );
        assert_eq!(Region::from_points(0.0, 0.0, 16385.0, 10.0), None);
        assert_eq!(Region::from_points(16385.0, 0.0, 10.0, 10.0), None);
        assert_eq!(Region::from_points(-16384.5, 0.0, 10.0, 10.0), None);
        assert_eq!(Region::from_points(0.0, 0.0, f64::NAN, 10.0), None);
        assert_eq!(Region::from_points(0.0, 0.0, 1e300, 10.0), None);
    }

    #[test]
    fn every_bound_is_enforced() {
        for line in [
            "not json",
            "[1]",
            r#"{"op":7}"#,
            r#"{"op":"run","argv":"/bin/echo"}"#,
            r#"{"op":"run","argv":[""]}"#,
            r#"{"op":"run","argv":[7]}"#,
            "{\"op\":\"run\",\"argv\":[\"a\\u0000b\"]}",
            r#"{"op":"run","argv":["true"],"timeoutMs":0}"#,
            r#"{"op":"run","argv":["true"],"timeoutMs":600001}"#,
            r#"{"op":"run","argv":["true"],"timeoutMs":"5"}"#,
            r#"{"op":"screenshot","maxDimension":0}"#,
            r#"{"op":"screenshot","maxDimension":8193}"#,
            r#"{"op":"screenshot","maxDimension":1.5}"#,
            r#"{"op":"screenshot","pid":0}"#,
            r#"{"op":"screenshot","pid":-1}"#,
            r#"{"op":"screenshot","pid":"5"}"#,
            r#"{"op":"screenshot","pid":2147483648}"#,
            r#"{"op":"screenshot","pid":5.5}"#,
            r#"{"op":"screenshot","region":{"x":0,"y":0,"width":10}}"#,
            r#"{"op":"screenshot","region":{"x":0,"y":0,"width":0,"height":10}}"#,
            r#"{"op":"screenshot","region":{"x":0,"y":0,"width":16385,"height":10}}"#,
            r#"{"op":"screenshot","region":{"x":-16385,"y":0,"width":10,"height":10}}"#,
            r#"{"op":"screenshot","region":{"x":0.5,"y":0,"width":10,"height":10}}"#,
            r#"{"op":"screenshot","region":"0,0,10,10"}"#,
            r#"{"op":"screenshot","region":[0,0,10,10]}"#,
            r#"{"op":"screenshot","pid":5,"region":{"x":0,"y":0,"width":10,"height":10}}"#,
        ] {
            assert_eq!(refused(line)["code"], "bad-request", "{line}");
        }

        let many = format!(
            r#"{{"op":"run","argv":[{}]}}"#,
            vec!["\"x\""; MAX_ARGUMENTS + 1].join(",")
        );
        assert_eq!(refused(&many)["code"], "bad-request");

        let huge = vec![b' '; MAX_REQUEST + 1];
        assert_eq!(parse(&huge).unwrap_err()["error"], "request exceeds 1 MiB");
    }

    const EXPECT: &str = r#"{"boot":"B","pid":4312,"start":[1,2],"pgid":4312}"#;

    #[test]
    fn job_requests_parse_within_their_bounds() {
        let start = parse(
            br#"{"op":"job","action":"start","argv":["/bin/sleep","9"],"lifetimeMs":3600000,"uid":502,"name":"web","task":"3f9a1c07b2e4","hostSession":"3f9a1c07b2e4"}"#,
        )
        .unwrap();
        let Request::Job(JobRequest::Start {
            argv,
            lifetime,
            name,
            task,
            uid,
            ..
        }) = start
        else {
            panic!("a start");
        };
        assert_eq!(argv, ["/bin/sleep", "9"]);
        assert_eq!(lifetime, Duration::from_secs(3600));
        assert_eq!(name.as_deref(), Some("web"));
        assert_eq!(task.unwrap().as_str(), "3f9a1c07b2e4");
        assert_eq!(uid, 502);

        let stop =
            format!(r#"{{"op":"job","action":"stop","id":"7c2e91a04b3f","expect":{EXPECT}}}"#);
        let Ok(Request::Job(JobRequest::Stop { grace, expect, .. })) = parse(stop.as_bytes())
        else {
            panic!("a stop");
        };
        assert_eq!(grace, JOB_GRACE);
        assert_eq!(expect.pgid, 4312);

        assert_eq!(
            parse(br#"{"op":"job","action":"stop-all","reason":"on-console"}"#),
            Ok(Request::Job(JobRequest::StopAll {
                reason: StopReason::OnConsole
            }))
        );
        assert_eq!(
            parse(br#"{"op":"job","action":"show"}"#),
            Ok(Request::Job(JobRequest::Show { id: None }))
        );
        assert_eq!(
            parse(br#"{"op":"job","action":"log","id":"7c2e91a04b3f","bytes":16384}"#),
            Ok(Request::Job(JobRequest::Log {
                id: JobId::parse("7c2e91a04b3f").unwrap(),
                bytes: 16384
            }))
        );

        let stop = |rest: &str| {
            format!(r#"{{"op":"job","action":"stop","id":"7c2e91a04b3f","expect":{EXPECT}{rest}}}"#)
        };
        for line in [
            r#"{"op":"job","action":"start","argv":["x"],"lifetimeMs":999,"uid":502}"#.to_owned(),
            r#"{"op":"job","action":"start","argv":["x"],"lifetimeMs":86400001,"uid":502}"#.to_owned(),
            r#"{"op":"job","action":"start","argv":["x"],"lifetimeMs":5000}"#.to_owned(),
            r#"{"op":"job","action":"start","argv":[],"lifetimeMs":5000,"uid":502}"#.to_owned(),
            r#"{"op":"job","action":"start","argv":["x"],"lifetimeMs":5000,"uid":502,"name":""}"#.to_owned(),
            "{\"op\":\"job\",\"action\":\"start\",\"argv\":[\"x\"],\"lifetimeMs\":5000,\"uid\":502,\"name\":\"a\\nb\"}".to_owned(),
            r#"{"op":"job","action":"start","argv":["x"],"lifetimeMs":5000,"uid":502,"task":"ABC"}"#.to_owned(),
            stop(r#","graceMs":999"#),
            stop(r#","graceMs":60001"#),
            r#"{"op":"job","action":"log","id":"7c2e91a04b3f","bytes":0}"#.to_owned(),
            r#"{"op":"job","action":"log","id":"7c2e91a04b3f","bytes":1048577}"#.to_owned(),
            r#"{"op":"job","action":"stop-all","reason":"later"}"#.to_owned(),
            r#"{"op":"job"}"#.to_owned(),
        ] {
            assert_eq!(refused(&line)["code"], "bad-request", "{line}");
        }
    }

    #[test]
    fn a_job_expectation_has_exactly_its_fields() {
        for expect in [
            r#"{"boot":"B","pid":4312,"start":[1,2]}"#,
            r#"{"boot":"B","pid":4312,"start":[1,2],"pgid":4312,"uid":502}"#,
            r#"{"boot":"B","pid":4312,"start":[1],"pgid":4312}"#,
        ] {
            let line =
                format!(r#"{{"op":"job","action":"stop","id":"7c2e91a04b3f","expect":{expect}}}"#);
            assert_eq!(refused(&line)["code"], "bad-request", "{expect}");
        }
    }

    #[test]
    fn job_ids_and_keys_are_exact() {
        assert!(JobId::parse("7c2e91a04b3f").is_some());
        for id in [
            "7c2e91a04b3",
            "7c2e91a04b3f0",
            "7C2E91A04B3F",
            "7c2e91a04b3g",
        ] {
            assert!(JobId::parse(id).is_none(), "{id}");
            let line = format!(r#"{{"op":"job","action":"show","id":"{id}"}}"#);
            assert_eq!(refused(&line)["code"], "bad-request", "{id}");
        }
        assert_eq!(JobId::new().as_str().len(), JobId::LEN);
    }

    #[test]
    fn unknown_job_actions_are_bad_requests() {
        let reply = refused(r#"{"op":"job","action":"stop-everything"}"#);
        assert_eq!(reply["error"], "unknown job action 'stop-everything'");
        assert_eq!(reply["uncertain"], false);
    }

    #[test]
    fn read_requests_parse_within_their_bounds() {
        let Ok(Request::Read(read)) = parse(
            br#"{"op":"read","path":"atlas-work/a.bin","offset":8,"length":8388608,"maxSize":67108864}"#,
        ) else {
            panic!("a read");
        };
        assert_eq!(
            (read.offset, read.length, read.max_size, read.digest),
            (8, READ_RANGE_MAX, 64 << 20, false)
        );
        let Ok(Request::Read(digest)) = parse(
            br#"{"op":"read","path":"a","digest":true,"maxSize":1,"expect":{"dev":1,"ino":2,"size":3,"mtime":[4,5],"ctime":[6,7]}}"#,
        ) else {
            panic!("a digest");
        };
        assert!(digest.digest);
        assert_eq!(digest.expect.unwrap().size, 3);

        let long = format!(
            r#"{{"op":"read","path":"{}","maxSize":1}}"#,
            "a".repeat(1025)
        );
        for line in [
            r#"{"op":"read","maxSize":1}"#.to_owned(),
            r#"{"op":"read","path":"","maxSize":1}"#.to_owned(),
            long,
            r#"{"op":"read","path":"a","length":8388609,"maxSize":1}"#.to_owned(),
            r#"{"op":"read","path":"a","maxSize":0}"#.to_owned(),
            r#"{"op":"read","path":"a","maxSize":1073741825}"#.to_owned(),
            r#"{"op":"read","path":"a","digest":true,"length":5,"maxSize":1}"#.to_owned(),
            r#"{"op":"read","path":"a","maxSize":1,"expect":{"dev":1}}"#.to_owned(),
            "{\"op\":\"read\",\"path\":\"a\\u0000b\",\"maxSize\":1}".to_owned(),
        ] {
            assert_eq!(refused(&line)["code"], "bad-request", "{line}");
        }
    }
}

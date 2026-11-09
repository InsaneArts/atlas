//! Protocol v1 requests the helper daemon accepts, validated before anything
//! runs. Refusals keep their exact wording: clients and the live checks
//! match on it.
#[cfg(target_os = "macos")]
use crate::error::Error;
use crate::{error::Code, host::Key, time::ms};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{ops::RangeInclusive, time::Duration};

mod job;
mod read;

pub use job::*;
pub use job::{Expected, How, JobId, JobStart, JobView, StopReason};
pub use read::ReadRefusal;

/// Longest request line, in bytes.
pub const MAX_REQUEST: usize = 1024 * 1024;
/// Most event lines before the final reply.
pub const MAX_REPLY_EVENTS: usize = 1024;
const MAX_ARGUMENTS: usize = 4096;
const MAX_ARGUMENT_BYTES: usize = 256 * 1024;
const MAX_DIMENSION: u64 = 8192;
/// How long a command may run, and how long unless told.
const RUN_TIMEOUT_MAX: Duration = Duration::from_secs(10 * 60);
const RUN_TIMEOUT: Duration = Duration::from_secs(30);

/// Farthest a region's top-left corner may lie from the display's, and its
/// longest side, in display points.
const MAX_POINTS: i32 = 16384;

/// The longest a background job may run, and the shortest.
pub const JOB_LIFETIME_MAX: Duration = Duration::from_secs(24 * 60 * 60);
pub const JOB_LIFETIME_MIN: Duration = Duration::from_secs(1);
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
    /// One part of this process's crop, as the windows are when it is taken.
    Part {
        pid: i64,
        part: Part,
    },
}

/// Which part of an app's crop a screenshot shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Part {
    /// Only its window with this number.
    Window(i64),
    /// Only where its crop lies within this region.
    Within(Region),
}

/// The daemon capability to capture one window of an app, or its part of a
/// region, as its hello lists it.
pub const APP_PARTS: &str = "app-parts";

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

/// The daemon capability to mute the helper's apps, as its hello lists it.
pub const SOUND: &str = "helper-sound";

/// Readiness-driven, unencoded RGB frames on a persistent connection.
pub const RGB_STREAM: &str = "rgb-stream-v1";
/// The raw frame protocol with preview sizes through 4096 pixels.
pub const RGB_STREAM_HIGH_RES: &str = "rgb-stream-4096";
/// The experimental ScreenCaptureKit source for raw frames.
pub const SCK_EXPERIMENTAL: &str = "capture-sck-experimental";
/// A raw frame stream stops only when asked, answers once its source is
/// stopped and drained, and is asked about later by generation.
pub const RGB_STREAM_STOP: &str = "rgb-stream-stop-v1";
/// Fresh independent session proofs from a persistent child, without a
/// process per request.
pub const SESSION_PROOF: &str = "session-proof-v1";

/// Where a raw frame's pixels come from in the helper.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CaptureBackend {
    #[default]
    CoreGraphics,
    ScreenCaptureKit,
}

impl CaptureBackend {
    /// The word for it on the wire.
    pub fn name(self) -> &'static str {
        match self {
            Self::CoreGraphics => "cg",
            Self::ScreenCaptureKit => "sck",
        }
    }
}

/// Whether `id` is exactly 32 lowercase hex digits.
fn hex32(id: &str) -> bool {
    id.len() == 32
        && id
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}

/// Whether `challenge` may name one independent session proof: 1 to 128
/// printable ASCII bytes.
pub fn valid_challenge(challenge: &str) -> bool {
    (1..=128).contains(&challenge.len()) && challenge.bytes().all(|byte| byte.is_ascii_graphic())
}

/// The daemon capability to read files for the person's side to collect.
pub const READ: &str = "read";

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

/// Each request's shape, wire word and decoder are one row, so a declared
/// request cannot go undecoded.
macro_rules! requests {
    ($request:ident => $($(#[$attr:meta])* $word:literal $variant:ident $(($($tuple:ty),*))? $({$($field:tt)*})? = $decode:expr),+ $(,)?) => {
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub enum Request { $($(#[$attr])* $variant $(($($tuple),*))? $({$($field)*})?),+ }

        fn decode(op: &str, $request: &Value) -> Result<Request, Value> {
            match op {
                $($word => $decode,)+
                other => Err(bad(format!("unknown op '{other}'"))),
            }
        }
    };
}

requests! { request =>
    "hello" Hello = Ok(Request::Hello),
    /// One fresh session proof from the helper's persistent proof worker.
    "session-proof" SessionProof { challenge: String } = Ok(Request::SessionProof {
        challenge: request["challenge"]
            .as_str()
            .filter(|challenge| valid_challenge(challenge))
            .ok_or_else(|| bad("session-proof requires a printable ASCII challenge of 1–128 bytes"))?
            .to_owned(),
    }),
    "host-session" HostSession(Vec<crate::host::verification::HostClaim>) =
        crate::host::verification::claims(&request["claims"])
            .map(Request::HostSession)
            .ok_or_else(|| bad("invalid host session claims")),
    /// Whether a live capture generation is known to be stopped and drained.
    "stream-drain-status" StreamDrainStatus {
        daemon_pid: u32,
        producer_generation: u64,
        query_id: String,
        stop_request: Option<u64>,
    } = Ok(Request::StreamDrainStatus {
        daemon_pid: whole(&request["daemonPID"], 1..=u64::from(u32::MAX), "\"daemonPID\" must be a process ID")?
            as u32,
        producer_generation: whole(
            &request["producerGeneration"],
            1..=u64::MAX,
            "\"producerGeneration\" must be a generation",
        )?,
        query_id: request["queryID"]
            .as_str()
            .filter(|id| hex32(id))
            .ok_or_else(|| bad("\"queryID\" must be 32 lowercase hex digits"))?
            .to_owned(),
        stop_request: optional(&request["stopRequestID"], |value| {
            whole(value, 0..=u64::MAX, "\"stopRequestID\" must be a whole number")
        })?,
    }),
    /// Raw frames on this connection, each after a readiness byte.
    "stream" Stream { max_dimension: u32, pin: SessionPin, backend: CaptureBackend } =
        Ok(Request::Stream {
            backend: match request["backend"].as_str() {
                None if request["backend"].is_null() => CaptureBackend::CoreGraphics,
                Some("cg") => CaptureBackend::CoreGraphics,
                Some("sck") => CaptureBackend::ScreenCaptureKit,
                _ => return Err(bad("stream backend must be cg or sck")),
            },
            max_dimension: whole(&request["maxDimension"], 320..=4096, "\"maxDimension\" must be 320–4096")?
                as u32,
            pin: pin(request)?.ok_or_else(|| bad("stream requires a session pin"))?,
        }),
    /// One raw frame, framed as a live frame is: the picture before an
    /// agent's action, which one byte on the connection cancels.
    "frame" Frame { max_dimension: u32, pin: SessionPin } = Ok(Request::Frame {
        max_dimension: whole(&request["maxDimension"], 320..=4096, "\"maxDimension\" must be 320–4096")?
            as u32,
        pin: pin(request)?.ok_or_else(|| bad("frame requires a session pin"))?,
    }),
    "apps" Apps { pin: Option<SessionPin> } = Ok(Request::Apps { pin: pin(request)? }),
    "windows" Windows { dock: bool, pin: Option<SessionPin> } = Ok(Request::Windows {
        dock: boolean(request, "dock")?,
        pin: pin(request)?,
    }),
    "restart" Restart = Ok(Request::Restart),
    /// Log this helper out; `force` ends its session without asking its apps.
    "logout" Logout { force: bool } = Ok(Request::Logout {
        force: boolean(request, "force")?,
    }),
    "screenshot" Screenshot {
        max_dimension: Option<u32>,
        /// At most one pixel per display point of the area captured, too.
        per_point: bool,
        crop: Option<Crop>,
    } = Ok(Request::Screenshot {
        max_dimension: optional(&request["maxDimension"], |value| {
            whole(value, 1..=MAX_DIMENSION, "\"maxDimension\" must be 1–8192")
        })?
        .map(|dimension| dimension as u32),
        per_point: boolean(request, "perPoint")?,
        crop: crop(request)?,
    }),
    "run" Run {
        argv: Vec<String>,
        timeout: Duration,
        pin: Option<SessionPin>,
    } = Ok(Request::Run {
        argv: argv(request, "run")?,
        timeout: millis(
            request,
            "timeoutMs",
            Duration::from_millis(1)..=RUN_TIMEOUT_MAX,
            Some(RUN_TIMEOUT),
        )?,
        pin: pin(request)?,
    }),
    /// Background jobs: start, stop, stop every one, show, read a log.
    "job" Job(JobRequest) = job(request).map(Request::Job),
    /// Part of a file in the helper, or its digest.
    "read" Read(ReadRequest) = read(request).map(Request::Read),
    /// Whether the helper's apps can be heard; with `muted`, change it.
    "sound" Sound { muted: Option<bool> } = Ok(Request::Sound {
        muted: optional(&request["muted"], |value| {
            value.as_bool().ok_or_else(|| bad("\"muted\" must be true or false"))
        })?,
    }),
}

/// Admission and lost-reply handling are chosen together for every request.
#[cfg(target_os = "macos")]
#[derive(Clone, Copy)]
pub struct Policy {
    console: Console,
    pin: Option<SessionPin>,
    pub phase: ConnectionPhase,
}

#[cfg(target_os = "macos")]
#[derive(Clone, Copy)]
enum Console {
    /// Served on screen or off, to the helper's own processes too.
    Open,
    Any,
    Hidden,
}

#[cfg(target_os = "macos")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConnectionPhase {
    Refusal,
    Read(&'static str),
    Mutation(&'static str),
}

#[cfg(target_os = "macos")]
impl Request {
    pub fn policy(&self) -> Policy {
        use ConnectionPhase::{Mutation, Read};
        use Console::{Any, Hidden, Open};
        let (console, pin, phase) = match self {
            Self::Hello => (Open, None, Read("hello")),
            Self::SessionProof { .. } => (Hidden, None, Read("session-proof")),
            Self::HostSession(_) => (Any, None, Read("host-session")),
            Self::Stream { pin, .. } => (Hidden, Some(*pin), Read("stream")),
            Self::Frame { pin, .. } => (Hidden, Some(*pin), Read("frame")),
            Self::StreamDrainStatus { .. } => (Any, None, Read("stream-drain-status")),
            Self::Apps { pin } => (Any, *pin, Read("apps")),
            Self::Windows { pin, .. } => (Hidden, *pin, Read("windows")),
            Self::Screenshot { .. } => (Hidden, None, Read("screenshot")),
            Self::Read(ReadRequest { pin, .. }) => (Hidden, *pin, Read("read")),
            Self::Run { pin, .. } => (Hidden, *pin, Mutation("run")),
            Self::Restart => (Any, None, Mutation("restart")),
            Self::Logout { .. } => (Hidden, None, Mutation("logout")),
            Self::Job(JobRequest::Start { pin, .. }) => (Hidden, *pin, Mutation("job-start")),
            Self::Job(JobRequest::Stop { .. } | JobRequest::StopAll { .. }) => {
                (Any, None, Mutation("job-stop"))
            }
            Self::Job(JobRequest::Show { .. } | JobRequest::Log { .. }) => (Any, None, Read("job")),
            Self::Sound { muted: None } => (Any, None, Read("sound")),
            Self::Sound { muted: Some(_) } => (Any, None, Mutation("sound")),
        };
        Policy {
            console,
            pin,
            phase,
        }
    }
}

#[cfg(target_os = "macos")]
impl Policy {
    /// Console refusal precedes a session-pin refusal at every admission point.
    pub fn check(
        self,
        on_console: fn() -> bool,
        session: fn() -> Result<Value, Error>,
    ) -> Option<Value> {
        self.allowed(on_console())
            .or_else(|| self.pinned_elsewhere(session))
    }

    /// Ending and inspecting jobs remain available while the helper is visible.
    pub fn allowed(self, on_console: bool) -> Option<Value> {
        match self.console {
            Console::Hidden if on_console => Some(failure(
                Code::OnConsole,
                "The helper is on the screen; switch back to your own account first",
            )),
            Console::Hidden | Console::Any | Console::Open => None,
        }
    }

    /// Whether the helper's own processes may ask this, not only the
    /// controller.
    pub fn open_to_helper(self) -> bool {
        match self.console {
            Console::Open => true,
            Console::Any | Console::Hidden => false,
        }
    }

    /// A pin is checked against a fresh session observation at each admission point.
    fn pinned_elsewhere(self, session: fn() -> Result<Value, Error>) -> Option<Value> {
        let pin = self.pin?;
        let here = session()
            .and_then(|proof| crate::session::InputEpoch::from_proof(&proof, std::process::id()))
            .map(|epoch| SessionPin {
                security_session_id: epoch.security_session_id,
                display_id: epoch.display_id,
            });

        match here {
            Ok(here) if here == pin => None,
            Ok(_) => Some(refused(Error::session_changed())),
            Err(error) => Some(refused(error)),
        }
    }
}

/// A refusal in the shape every Atlas failure has, so a reader needs one
/// parser. Nothing here is of unknown outcome: the request never ran.
pub fn failure(code: Code, error: impl Into<String>) -> Value {
    json!({"ok": false, "code": code, "error": error.into(), "uncertain": false})
}

/// A failure once work began: it may have done some of it.
pub fn interrupted(code: Code, error: impl Into<String>) -> Value {
    let mut reply = failure(code, error);
    reply["uncertain"] = json!(true);
    reply
}

/// A pre-execution error is always a certain refusal.
#[cfg(target_os = "macos")]
pub fn refused(error: Error) -> Value {
    failure(error.code, error.error)
}

fn bad(error: impl Into<String>) -> Value {
    failure(Code::BadRequest, error)
}

/// Parse one request line, or the reply that refuses it.
///
/// # Errors
///
/// Returns the reply that refuses the line.
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
    decode(op, &request)
}

/// What `decode` makes of `value`, unless it is absent or null.
fn optional<T>(
    value: &Value,
    decode: impl FnOnce(&Value) -> Result<T, Value>,
) -> Result<Option<T>, Value> {
    match value {
        Value::Null => Ok(None),
        value => decode(value).map(Some),
    }
}

/// `value` as a whole number in `range`, or the refusal `said`.
fn whole(value: &Value, range: RangeInclusive<u64>, said: impl Into<String>) -> Result<u64, Value> {
    value
        .as_u64()
        .filter(|number| range.contains(number))
        .ok_or_else(|| bad(said))
}

/// Absent and null switches both retain the protocol's false default.
fn boolean(request: &Value, field: &str) -> Result<bool, Value> {
    optional(&request[field], |value| {
        value
            .as_bool()
            .ok_or_else(|| bad(format!("\"{field}\" must be true or false")))
    })
    .map(Option::unwrap_or_default)
}

/// Run and job starts accept the same bounded arguments, with distinct words.
fn argv(request: &Value, operation: &str) -> Result<Vec<String>, Value> {
    let empty = || bad(format!("{operation} requires a non-empty \"argv\" array"));
    let argv = request["argv"]
        .as_array()
        .filter(|argv| !argv.is_empty())
        .ok_or_else(empty)?;
    if argv.len() > MAX_ARGUMENTS {
        return Err(bad(format!("{operation} accepts at most 4096 arguments")));
    }

    let argv = argv
        .iter()
        .map(|argument| {
            argument
                .as_str()
                .filter(|text| text.len() <= MAX_ARGUMENT_BYTES && !text.contains('\0'))
                .map(str::to_owned)
                .ok_or_else(|| bad(format!("{operation} arguments must be strings without NUL")))
        })
        .collect::<Result<Vec<_>, _>>()?;
    if argv.first().is_none_or(String::is_empty) {
        return Err(empty());
    }
    Ok(argv)
}

/// `request[field]` as milliseconds within `range`, or `default` when it is
/// absent and there is one.
fn millis(
    request: &Value,
    field: &str,
    range: RangeInclusive<Duration>,
    default: Option<Duration>,
) -> Result<Duration, Value> {
    if let (Value::Null, Some(default)) = (&request[field], default) {
        return Ok(default);
    }
    let (low, high) = (ms(*range.start()), ms(*range.end()));
    whole(
        &request[field],
        low..=high,
        format!("\"{field}\" must be {low}–{high}"),
    )
    .map(Duration::from_millis)
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
    optional(&request[field], |value| {
        value
            .as_str()
            .and_then(Key::parse)
            .ok_or_else(|| bad(format!("\"{field}\" must be 12 lowercase hex digits")))
    })
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
            let name = optional(&request["name"], |value| {
                value
                    .as_str()
                    .filter(|name| job_name(name))
                    .map(str::to_owned)
                    .ok_or_else(|| {
                        bad("\"name\" must be 1–64 characters, none of them a control character")
                    })
            })?;
            let uid = whole(
                &request["uid"],
                0..=u64::from(u32::MAX),
                "job start requires the helper's \"uid\"",
            )?;
            Ok(JobRequest::Start {
                start: JobStart {
                    argv: argv(request, "job start")?,
                    lifetime: millis(
                        request,
                        "lifetimeMs",
                        JOB_LIFETIME_MIN..=JOB_LIFETIME_MAX,
                        None,
                    )?,
                    name,
                    task: key(request, "task")?,
                    session: key(request, "hostSession")?,
                },
                uid: uid as u32,
                pin: pin(request)?,
            })
        }
        "stop" => Ok(JobRequest::Stop {
            id: job_id(request)?,
            expect: request["expect"]
                .as_object()
                // Expected requires all four members; the length also excludes extras.
                .filter(|expect| expect.len() == 4)
                .and_then(|_| Expected::deserialize(&request["expect"]).ok())
                .ok_or_else(|| {
                    bad("\"expect\" must name the job's boot, pid, start and pgid, and nothing else")
                })?,
            grace: millis(
                request,
                "graceMs",
                JOB_GRACE_MIN..=JOB_GRACE_MAX,
                Some(JOB_GRACE),
            )?,
        }),
        "stop-all" => Ok(JobRequest::StopAll {
            reason: StopReason::deserialize(&request["reason"])
                .map_err(|_| bad("\"reason\" must be person, on-console or stopped"))?,
        }),
        "show" => Ok(JobRequest::Show {
            id: optional(&request["id"], |_| job_id(request))?,
        }),
        "log" => Ok(JobRequest::Log {
            id: job_id(request)?,
            bytes: whole(
                &request["bytes"],
                1..=JOB_LOG_MAX as u64,
                "\"bytes\" must be 1–1048576",
            )? as usize,
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
    let number = |field: &str| {
        optional(&request[field], |value| {
            value
                .as_u64()
                .ok_or_else(|| bad(format!("\"{field}\" must be a whole number")))
        })
        .map(Option::unwrap_or_default)
    };
    let offset = number("offset")?;
    let length = number("length")?;
    if length > READ_RANGE_MAX {
        return Err(bad("\"length\" must be at most 8388608"));
    }
    let digest = boolean(request, "digest")?;
    if digest && length != 0 {
        return Err(bad("A digest reads the whole file: give \"length\" 0"));
    }
    let max_size = whole(
        &request["maxSize"],
        1..=READ_SIZE_MAX,
        "\"maxSize\" must be 1–1073741824",
    )?;
    let expect = optional(&request["expect"], |value| {
        FileIdentity::deserialize(value).map_err(|_| {
            bad("\"expect\" must be a file's dev, ino, size, mtime and ctime, and nothing else")
        })
    })?;
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
    optional(&request["session"], |value| {
        SessionPin::deserialize(value)
            .map_err(|_| bad("\"session\" must name a securitySessionId and a displayId"))
    })
}

/// A screenshot request's `pid` or `region`, at most one of them; with
/// `pid`, at most one of `window` and `within`.
fn crop(request: &Value) -> Result<Option<Crop>, Value> {
    let pid = |pid: &Value| {
        pid.as_i64()
            .filter(|pid| (1..=i64::from(i32::MAX)).contains(pid))
            .ok_or_else(|| bad("\"pid\" must be a process ID"))
    };
    if !request["pid"].is_null() && !request["region"].is_null() {
        return Err(bad("give \"pid\" or \"region\", not both"));
    }
    match (
        &request["pid"],
        &request["region"],
        &request["window"],
        &request["within"],
    ) {
        (Value::Null, Value::Null, Value::Null, Value::Null) => Ok(None),
        (asked, Value::Null, Value::Null, Value::Null) => Ok(Some(Crop::App { pid: pid(asked)? })),
        (asked, Value::Null, window, Value::Null) if !asked.is_null() => {
            let number = window
                .as_i64()
                .filter(|number| *number >= 1)
                .ok_or_else(|| bad("\"window\" must be a window number"))?;
            Ok(Some(Crop::Part {
                pid: pid(asked)?,
                part: Part::Window(number),
            }))
        }
        (asked, Value::Null, Value::Null, within) if !asked.is_null() => Ok(Some(Crop::Part {
            pid: pid(asked)?,
            part: Part::Within(region_of(within, "within")?),
        })),
        (Value::Null, region, Value::Null, Value::Null) => {
            Ok(Some(Crop::Region(region_of(region, "region")?)))
        }
        _ => Err(bad(
            "give \"window\" or \"within\" with \"pid\" alone, not both",
        )),
    }
}

/// The region `value` names, as the request's `key`.
fn region_of(value: &Value, key: &str) -> Result<Region, Value> {
    let side = |name: &str| value.get(name).and_then(Value::as_i64);
    side("x")
        .zip(side("y"))
        .zip(side("width").zip(side("height")))
        .and_then(|((x, y), (width, height))| Region::new(x, y, width, height))
        .ok_or_else(|| {
            bad(format!(
                "\"{key}\" must be whole points: x and y -16384–16384, width and height 1–16384"
            ))
        })
}

#[cfg(test)]
#[path = "protocol_tests.rs"]
mod tests;

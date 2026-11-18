//! A VM's session keeper as a test scripts it: a socket that answers the
//! keeper's protocol for a guest that proves itself, paired with a station
//! key of the test's own, and keeps every request it was asked, so a test
//! of a command above the client reaches a VM end to end.
use super::StandInRunner;
use crate::{client::Client, link::Link};
use atlas_core::{
    error::{Code, Error, Result},
    files::Token,
    protocol::{
        EndedView, How, JobId, JobReply, JobState, JobView, Progress, Showing, Shown, StopCameTo,
        Stopped,
    },
    session::paired::Paired,
};
use atlas_session::{
    keys::Keypair,
    pairing::{Enrolment, StationRecords},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use std::{
    path::Path,
    sync::{Arc, Mutex, PoisonError},
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::UnixListener,
};

/// The facts of a machine guest's own screen: account 502's, its daemon,
/// process 123, the proof's parent.
#[must_use]
pub fn screen() -> Value {
    json!({"ok": true, "uid": 502, "hasGraphicAccess": true, "parentPID": 123,
        "onConsole": true, "securitySessionID": 42, "displayID": 7, "width": 1440.0,
        "height": 900.0, "scale": 2.0})
}

/// The hello of a machine guest that proves itself: account 502, in
/// machine mode, in a virtual machine, closed to anyone but Atlas, declaring
/// what a real machine guest serving everything declares, its Stop `stop`.
#[must_use]
pub fn machine_hello(stop: &str) -> Value {
    json!({"ok": true, "op": "hello", "user": {"uid": 502},
        "session": {"managerName": "Aqua", "onConsole": true},
        "display": {"width": 1440.0, "height": 900.0, "scale": 2.0},
        "permissions": {"accessibility": true, "postEvents": true, "screenCapture": true},
        "machine": {"mode": "machine", "boot": "boot-1", "virtual": true},
        "daemon": {"protocol": 1, "pid": 123, "version": "1.0.0", "build": "b1",
            "capabilities": atlas_session::compatibility::MACHINE},
        "remoteLogin": "off", "account": {"admin": false},
        "agents": {"admitted": true, "held": null, "stopped": false},
        "input": {"standing": "ready", "failure": null, "held": null},
        "stop": {"state": stop}, "work": {"state": "none"}})
}

/// The hello of a machine guest's input service, ready, of its daemon's
/// build, declaring what a real one declares.
#[must_use]
pub fn input_hello() -> Value {
    json!({"ok": true, "protocolVersion": 1, "pid": 456, "version": "1.0.0", "build": "b1",
        "ready": true, "accessibility": true, "session": screen(),
        "capabilities": atlas_native::native::service::CAPABILITIES})
}

/// A pairing kept in memory.
struct Remembered(Mutex<Option<Enrolment>>);

impl StationRecords for Remembered {
    fn read(&self) -> Result<Option<Enrolment>> {
        Ok(self
            .0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone())
    }

    fn replace(&self, _: Option<&Enrolment>, _: &Enrolment) -> Result<()> {
        Err(Error::new(Code::State, "A test's pairing is fixed."))
    }
}

/// A keeper trusted whoever listens.
struct Trusting;

impl crate::link::Trust for Trusting {
    fn trusts(
        &self,
        _: &atlas_core::desktop::DesktopId,
        _: &Path,
        _: &std::os::unix::net::UnixStream,
    ) -> Result<()> {
        Ok(())
    }
}

/// How a scripted keeper's observations differ from what it says otherwise.
pub type Observing = Option<fn(&mut Value)>;

/// The keeper a test scripts, and what it was asked.
pub struct ScriptedKeeper {
    /// Every request it was asked, in order.
    pub asked: Arc<Mutex<Vec<Value>>>,
    /// The guest's own Stop, as its hello says it.
    pub stop: Arc<Mutex<&'static str>>,
    /// Atlas VM, which runs the VM.
    pub runner: StandInRunner,
    /// Whether the keeper reaches no guest, as while the VM is stopped.
    pub away: Arc<Mutex<bool>>,
    /// What the guest's input service still has to be allowed, as its hello
    /// says it; none while it takes input from agents.
    pub held: Arc<Mutex<Option<&'static str>>>,
    /// What the guest answers every read and change it is asked but its
    /// hello, in place of its own answer.
    pub answering: Arc<Mutex<Option<Value>>>,
    /// What the keeper answers the next read it is asked, in place of its
    /// guest's answer: given once, then gone.
    pub refusing_once: Arc<Mutex<Option<Value>>>,
    /// How the keeper's every observation differs from what it says
    /// otherwise, as when its guest reconnected since its hello.
    pub observing: Arc<Mutex<Observing>>,
    serving: tokio::task::JoinHandle<()>,
}

impl ScriptedKeeper {
    /// Every request of operation `op` it was asked, in order.
    #[must_use]
    pub fn asked_for(&self, op: &str) -> Vec<Value> {
        self.asked
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .filter(|asked| asked["op"] == op)
            .cloned()
            .collect()
    }
}

impl Drop for ScriptedKeeper {
    fn drop(&mut self) {
        self.serving.abort();
    }
}

/// The generation the scripted keeper is live in.
pub const GENERATION: u64 = 7;

/// A VM client of a keeper this scripts on a socket in `directory`: its
/// guest proves itself, runs every command as one that printed `hi`, keeps
/// one background job ([`JOB`]) that any start starts and any stop ends,
/// runs no app and keeps no copy, gives a picture of its whole display for
/// a screenshot, and settles whatever it is asked to. The
/// VM is added, its lifecycle record kept in `directory`, and runs, as
/// Atlas VM, stood in for there too, says.
#[must_use]
pub fn vm_through_keeper(directory: &Path) -> (Client, ScriptedKeeper) {
    let (station, guest) = (
        Keypair::generate().expect("a key"),
        Keypair::generate().expect("a key"),
    );
    let paired = Paired {
        station: station.public().fingerprint(),
        guest: guest.public(),
        uid: 502,
        boot: "boot-1".to_owned(),
        change: Token::new(),
    };
    let socket = directory.join("keeper.sock");
    let listener = UnixListener::bind(&socket).expect("the keeper's socket");
    let asked = Arc::new(Mutex::new(Vec::new()));
    let stop = Arc::new(Mutex::new("clear"));
    let away = Arc::new(Mutex::new(false));
    let held = Arc::new(Mutex::new(None));
    let answering = Arc::new(Mutex::new(None));
    let refusing_once: Arc<Mutex<Option<Value>>> = Arc::new(Mutex::new(None));
    let observing: Arc<Mutex<Observing>> = Arc::new(Mutex::new(None));
    let (peer, own) = (json!(guest.public()), json!(station.public()));
    let serving = tokio::spawn({
        let (asked, stop, away, held, answering, refusing_once, observing) = (
            asked.clone(),
            stop.clone(),
            away.clone(),
            held.clone(),
            answering.clone(),
            refusing_once.clone(),
            observing.clone(),
        );
        async move {
            while let Ok((stream, _)) = listener.accept().await {
                let observing = *observing.lock().unwrap_or_else(PoisonError::into_inner);
                let (asked, stop, away, held, answering, refusing_once, peer, own) = (
                    asked.clone(),
                    stop.clone(),
                    away.clone(),
                    held.clone(),
                    answering.clone(),
                    refusing_once.clone(),
                    peer.clone(),
                    own.clone(),
                );
                tokio::spawn(async move {
                    let mut stream = BufReader::new(stream);
                    let mut line = String::new();
                    if stream.read_line(&mut line).await.is_err() {
                        return;
                    }
                    let Ok(request) = serde_json::from_str::<Value>(&line) else {
                        return;
                    };
                    asked
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .push(request.clone());
                    let stop = *stop.lock().unwrap_or_else(PoisonError::into_inner);
                    let held = *held.lock().unwrap_or_else(PoisonError::into_inner);
                    let answering = answering
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .clone();
                    let job_stopped = asked
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .iter()
                        .any(|asked| asked["request"]["action"] == "stop");
                    let refused = (request["op"] == "read")
                        .then(|| {
                            refusing_once
                                .lock()
                                .unwrap_or_else(PoisonError::into_inner)
                                .take()
                        })
                        .flatten();
                    let answer = if let Some(refused) = refused {
                        refused
                    } else if *away.lock().unwrap_or_else(PoisonError::into_inner)
                        && request["op"] != "hello"
                    {
                        json!({"ok": false, "code": "transport", "uncertain": false,
                            "error": "The control connection to the desktop failed."})
                    } else {
                        let guest = Guest {
                            stop,
                            held,
                            answering,
                            peer: &peer,
                            own: &own,
                        };
                        let mut answer = answered(&request, &guest, job_stopped);
                        if let Some(differ) = observing.filter(|_| request["op"] == "observe") {
                            differ(&mut answer);
                        }
                        answer
                    };
                    // A picture is the keeper's own line, then the guest's.
                    let lines = match answer {
                        Value::Array(lines) => lines,
                        one => vec![one],
                    };
                    for line in lines {
                        let written = stream
                            .get_mut()
                            .write_all(format!("{line}\n").as_bytes())
                            .await;
                        if written.is_err() {
                            return;
                        }
                    }
                });
            }
        }
    });
    let mut client = super::vm_client(directory, None);
    let id = client.desktop().id();
    added(directory, &id);
    let runner = StandInRunner::at(&crate::vm::runner::socket_in(directory, &id), "running");
    let records: Arc<dyn StationRecords> =
        Arc::new(Remembered(Mutex::new(Some(Enrolment::Paired(paired)))));
    client.link = Link::Keeper(
        crate::link::Keeper::at(
            Arc::new(client.selected.clone()),
            client.desktop().id(),
            socket,
            Duration::from_secs(10),
            Arc::new(Trusting),
        )
        .with_pairing(records)
        .with_lifecycle(client.lifecycle.clone()),
    );
    (
        client,
        ScriptedKeeper {
            asked,
            stop,
            runner,
            away,
            held,
            answering,
            refusing_once,
            observing,
            serving,
        },
    )
}

/// Keep the VM `id` as added, named `work`, in the lifecycle in `home`.
fn added(home: &Path, id: &atlas_core::desktop::DesktopId) {
    use crate::vm::lifecycle::{
        Lifecycle, Provenance, Reservation,
        record::{Record, State},
    };
    let reservation = Reservation {
        name: "work".to_owned(),
        id: id.clone(),
        named: true,
        resumed: false,
    };
    let mut record = Record::adding(
        &reservation,
        home.join("settings.toml"),
        Provenance {
            program: "/Applications/Atlas.app/Contents/MacOS/atlas".into(),
            requirement: "identifier \"com.insanearts.atlas\"".to_owned(),
        },
    );
    record.state = State::Created;
    Lifecycle::under(home)
        .keep_record(&record)
        .expect("the VM's lifecycle record");
}

/// Where the lifecycle record of the VM `id` under `home` is, made or not.
#[must_use]
pub fn lifecycle_record(home: &Path, id: &atlas_core::desktop::DesktopId) -> std::path::PathBuf {
    crate::vm::lifecycle::Lifecycle::under(home).record_at(id)
}

/// Keep the person's maintenance of the VM `id` at `phase` in the lifecycle
/// record under `home`, the VM added there first when it has no record: its
/// ID.
///
/// # Panics
///
/// When the record cannot be read or kept.
pub fn under_maintenance(
    home: &Path,
    id: &atlas_core::desktop::DesktopId,
    phase: crate::vm::lifecycle::record::Phase,
) -> Token {
    use crate::vm::lifecycle::{Lifecycle, record::Maintenance};
    let lifecycle = Lifecycle::under(home);
    if lifecycle
        .record(id)
        .expect("the VM's lifecycle record")
        .is_none()
    {
        added(home, id);
    }
    let mut record = lifecycle
        .record(id)
        .expect("the VM's lifecycle record")
        .expect("added");
    let change = Token::new();
    record.maintenance = Some(Maintenance {
        change: change.clone(),
        phase,
        restart: false,
        since_ms: atlas_core::time::now_ms(),
        open: None,
        close: None,
        opened_by: None,
        ended: None,
    });
    lifecycle
        .keep_record(&record)
        .expect("the VM's lifecycle record");
    change
}

/// The one background job the scripted keeper's guest keeps.
pub const JOB: &str = "0123456789ab";

/// The scripted guest's job: running as account 502, or ended once
/// `stopped`.
fn job(stopped: bool) -> JobView {
    JobView {
        id: JobId::parse(JOB).expect("a job's ID"),
        name: "sh".into(),
        task: None,
        session: None,
        uid: 502,
        boot: "boot-1".into(),
        pid: Some(4321),
        start: Some([1_700_000_000, 5]),
        pgid: Some(4321),
        state: if stopped {
            JobState::Ended
        } else {
            JobState::Running
        },
        started_at: 1,
        ends_at: u64::MAX,
        log_resets: 0,
        adopted: false,
        progress: if stopped {
            Progress::Ended(EndedView {
                how: How::Stopped,
                ended_at: 2,
                signal: Some("SIGTERM".into()),
                exit_code: None,
                exit_signal: None,
                left: Vec::new(),
                remaining: None,
            })
        } else {
            Progress::Neither {}
        },
    }
}

/// What the scripted guest answers a job's `request`, its job stopped
/// once `stopped`.
fn job_answered(request: &Value, stopped: bool) -> Value {
    match request["action"].as_str() {
        Some("start") => JobReply::Start { job: job(false) }.into_value(),
        Some("stop") => JobReply::Stop(Stopped {
            job: job(false),
            came_to: StopCameTo::Signalled("SIGTERM".into()),
        })
        .into_value(),
        Some("show") if request["id"].is_string() => JobReply::Show(Showing::One {
            job: Some(Shown::Job(Box::new(job(stopped)))),
        })
        .into_value(),
        Some("show") => JobReply::Show(Showing::Every {
            jobs: vec![Shown::Job(Box::new(job(stopped)))],
        })
        .into_value(),
        _ => json!({"ok": false, "code": "bad-request", "error": "Not scripted."}),
    }
}

/// What the scripted guest's environment worker answers `asked`, a request
/// of its records: no transactions listed, anything else refused as its
/// own refusal.
fn environment_answered(asked: &Value) -> Value {
    if asked["operation"] == "transactions" {
        return json!({"ok": true, "op": "environment", "exitCode": 0,
            "result": {"ok": true, "transactions": [], "hidden": 0, "more": 0, "unreadable": 0}});
    }
    json!({"ok": true, "op": "environment",
        "exitCode": crate::environment::WORKER_REFUSED,
        "result": {"ok": false, "code": "environment", "error": "Not scripted.",
            "refusal": "scripted"}})
}

/// The scripted guest as one request finds it.
struct Guest<'a> {
    /// Its own Stop.
    stop: &'a str,
    /// What its input service still has to be allowed; none while it takes
    /// input from agents.
    held: Option<&'a str>,
    /// What it answers every read and change but its hello, when a test
    /// says.
    answering: Option<Value>,
    /// Its key, and the station's.
    peer: &'a Value,
    own: &'a Value,
}

impl Guest<'_> {
    /// Its daemon's hello.
    fn hello(&self) -> Value {
        let mut hello = machine_hello(self.stop);
        if let Some(held) = self.held {
            hello["input"]["held"] = json!(held);
            hello["permissions"]["accessibility"] = json!(false);
        }
        hello
    }

    /// Its input service's hello.
    fn input(&self) -> Value {
        let mut hello = input_hello();
        if self.held.is_some() {
            hello["ready"] = json!(false);
            hello["accessibility"] = json!(false);
        }
        hello
    }
}

/// The scripted guest's one app with a window: Finder, process 812, its
/// window number 42.
pub const FINDER: i64 = 812;
pub const FINDER_WINDOW: i64 = 42;

/// What the scripted keeper answers `request` with, from `guest`, its job
/// stopped once `job_stopped`.
fn answered(request: &Value, guest: &Guest<'_>, job_stopped: bool) -> Value {
    if let Some(answering) = &guest.answering
        && matches!(request["op"].as_str(), Some("read" | "change"))
        && request["request"]["op"] != "hello"
    {
        return answering.clone();
    }
    match request["op"].as_str() {
        Some("hello") => json!({"ok": true, "op": "hello",
            "keeper": {"protocol": crate::vm::keeper::PROTOCOL,
                "capabilities": crate::vm::keeper::CAPABILITIES},
            "guest": {"protocol": atlas_session::compatibility::PROTOCOL,
                "capabilities": atlas_session::compatibility::MACHINE, "version": "1.0.0"},
            "generation": GENERATION, "live": true}),
        Some("observe") => json!({"ok": true, "op": "observe", "generation": GENERATION,
            "declared": {"protocol": atlas_session::compatibility::PROTOCOL,
                "capabilities": atlas_session::compatibility::MACHINE, "version": "1.0.0"},
            "peer": guest.peer, "station": guest.own,
            "status": {"hello": guest.hello(), "inputService": guest.input(),
                "sessionProof": {"ok": true, "challenge": request["challenge"],
                    "proof": screen()}},
            "runner": {"ok": true, "power": "running", "confirmed": true,
                "window": "closed", "incarnation": "6f1c0de2-run", "fence": 0}}),
        Some("read") if request["request"]["op"] == "hello" => guest.hello(),
        Some("read") if request["request"]["op"] == "job" => {
            job_answered(&request["request"], job_stopped)
        }
        Some("read") if request["request"]["op"] == "apps" => {
            json!({"ok": true, "apps": [], "starts": {}})
        }
        Some("read") if request["request"]["op"] == "windows" => {
            json!({"ok": true, "starts": {FINDER.to_string(): [123, 456]},
                "windows": [{"number": FINDER_WINDOW, "pid": FINDER, "owner": "Finder",
                    "title": "Documents", "layer": 0, "alpha": 1.0,
                    "bounds": {"x": 10, "y": 20, "width": 400, "height": 300}}]})
        }
        // Its input service keeps a look of Finder's one button.
        Some("read") if request["request"]["op"] == "elements" => {
            json!({"ok": true, "look": "8123abc", "truncated": false, "reasons": [],
                "errors": {}, "elements": [{"index": 0, "depth": 0, "role": "AXButton",
                    "title": "OK", "enabled": true,
                    "window": {"x": 10, "y": 20, "width": 400, "height": 300},
                    "frame": {"x": 20, "y": 30, "width": 60, "height": 20}}]})
        }
        Some("read") if request["request"]["op"] == "copies" => {
            json!({"ok": true, "op": "copies", "copies": [], "next": null, "unknown": []})
        }
        Some("read") => json!({"ok": true, "outcome": "unknown"}),
        Some("change") if request["request"]["op"] == "job" => {
            job_answered(&request["request"], job_stopped)
        }
        // Its environment worker runs a command as any command runs, lists
        // no transactions, and refuses anything else as its own refusal.
        Some("change")
            if request["request"]["op"] == "environment"
                && request["request"]["packet"]["request"]["operation"] != "run" =>
        {
            environment_answered(&request["request"]["packet"]["request"])
        }
        // Its input service takes, sends and lets go of whatever it is asked,
        // and says it checked what it was to and how it acted on an element.
        Some("change") if request["lane"] == "input" => match request["request"]["op"].as_str() {
            Some("acquire") => json!({"ok": true}),
            Some("input") => inputted(&request["request"]),
            Some("release") => json!({"ok": true, "released": true}),
            _ => json!({"ok": false, "code": "bad-request", "error": "Not scripted."}),
        },
        Some("change") => json!({"ok": true, "exitCode": 0, "timedOut": false,
            "signal": null, "truncated": false,
            "events": [{"event": "started", "pid": 321}, {"event": "output", "data": "aGkK"}]}),
        Some("settle") => json!({"ok": true, "op": "settle", "settled": request["changes"]}),
        Some("picture") if request["request"]["op"] == "screenshot" => {
            json!([{"ok": true, "op": "picture"}, screenshot(&request["request"])])
        }
        _ => json!({"ok": false, "code": "bad-request", "error": "Not scripted."}),
    }
}

/// What the scripted guest's input service answers an `input` request
/// with: every action performed, its expectation echoed as checked, and an
/// action on an element said to have gone through Accessibility.
fn inputted(request: &Value) -> Value {
    let actions: Vec<atlas_core::actions::Action> =
        serde_json::from_value(request["actions"].clone()).unwrap_or_default();
    let mut reply = json!({"ok": true, "app": null, "bundleId": null,
        "performed": request["actions"].as_array().map_or(0, Vec::len)});
    if request["expect"].is_object() {
        reply["expect"] = request["expect"].clone();
    }
    if actions
        .iter()
        .any(|action| action.element_action().is_some())
    {
        reply["mechanism"] = json!("accessibility");
        reply["verification"] = json!("not-checked");
    }
    reply
}

/// The scripted guest's screenshot of its whole 1440 × 900 point display,
/// 2880 × 1800 pixels, no larger than `request` asks: the head of a PNG
/// of that size, all its size is read from.
fn screenshot(request: &Value) -> Value {
    let width = request["maxDimension"].as_u64().unwrap_or(2880).min(2880);
    let height = width * 900 / 1440;
    let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
    png.extend(u32::try_from(width).unwrap_or_default().to_be_bytes());
    png.extend(u32::try_from(height).unwrap_or_default().to_be_bytes());
    png.resize(33, 0);
    json!({"ok": true, "png": STANDARD.encode(png), "width": width, "height": height,
        "scale": 2.0, "offDisplayWindows": []})
}

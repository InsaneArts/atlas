//! A VM's session keeper as a test scripts it: a socket that answers the
//! keeper's protocol for a guest that proves itself, paired with a station
//! key of the test's own, and keeps every request it was asked, so a test
//! of a command above the client reaches a VM end to end.
use crate::{client::Client, link::Link};
use atlas_core::{
    error::{Code, Error, Result},
    files::Token,
    session::paired::Paired,
};
use atlas_session::{
    keys::Keypair,
    pairing::{Enrolment, StationRecords},
};
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
/// machine mode, in a virtual machine, closed to anyone but Atlas, its Stop
/// `stop`.
#[must_use]
pub fn machine_hello(stop: &str) -> Value {
    json!({"ok": true, "op": "hello", "user": {"uid": 502},
        "session": {"managerName": "Aqua", "onConsole": true},
        "display": {"width": 1440.0, "height": 900.0, "scale": 2.0},
        "permissions": {"accessibility": true, "postEvents": true, "screenCapture": true},
        "machine": {"mode": "machine", "boot": "boot-1", "virtual": true},
        "daemon": {"protocol": 1, "pid": 123, "version": "1.0.0", "build": "b1",
            "capabilities": ["machine-proof-v1"]},
        "remoteLogin": "off", "account": {"admin": false},
        "agents": {"admitted": true, "held": null, "stopped": false},
        "input": {"standing": "ready", "failure": null, "held": null},
        "stop": {"state": stop}, "work": {"state": "none"}})
}

/// The hello of a machine guest's input service, ready, of its daemon's
/// build.
#[must_use]
pub fn input_hello() -> Value {
    json!({"ok": true, "protocolVersion": 1, "pid": 456, "version": "1.0.0", "build": "b1",
        "ready": true, "accessibility": true, "session": screen()})
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

/// The keeper a test scripts, and what it was asked.
pub struct ScriptedKeeper {
    /// Every request it was asked, in order.
    pub asked: Arc<Mutex<Vec<Value>>>,
    /// The guest's own Stop, as its hello says it.
    pub stop: Arc<Mutex<&'static str>>,
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
/// guest proves itself, runs every command as one that printed `hi`, and
/// settles whatever it is asked to.
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
    let (peer, own) = (json!(guest.public()), json!(station.public()));
    let serving = tokio::spawn({
        let (asked, stop) = (asked.clone(), stop.clone());
        async move {
            while let Ok((stream, _)) = listener.accept().await {
                let (asked, stop, peer, own) =
                    (asked.clone(), stop.clone(), peer.clone(), own.clone());
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
                    let answer = answered(&request, stop, &peer, &own);
                    let _ = stream
                        .get_mut()
                        .write_all(format!("{answer}\n").as_bytes())
                        .await;
                });
            }
        }
    });
    let mut client = super::vm_client(directory, None);
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
        .with_pairing(records),
    );
    (
        client,
        ScriptedKeeper {
            asked,
            stop,
            serving,
        },
    )
}

/// What the scripted keeper answers `request` with, its guest's Stop `stop`,
/// the guest's key `peer` and the station's `own`.
fn answered(request: &Value, stop: &str, peer: &Value, own: &Value) -> Value {
    match request["op"].as_str() {
        Some("hello") => json!({"ok": true, "op": "hello",
            "keeper": {"protocol": crate::vm::keeper::PROTOCOL, "capabilities": []},
            "generation": GENERATION, "live": true}),
        Some("observe") => json!({"ok": true, "op": "observe", "generation": GENERATION,
            "peer": peer, "station": own,
            "status": {"hello": machine_hello(stop), "inputService": input_hello(),
                "sessionProof": {"ok": true, "challenge": request["challenge"],
                    "proof": screen()}},
            "runner": {"ok": true, "power": "running", "confirmed": true,
                "window": "closed"}}),
        Some("read") if request["request"]["op"] == "hello" => machine_hello(stop),
        Some("read") => json!({"ok": true, "outcome": "unknown"}),
        Some("change") => json!({"ok": true, "exitCode": 0, "timedOut": false,
            "signal": null, "truncated": false,
            "events": [{"event": "started", "pid": 321}, {"event": "output", "data": "aGkK"}]}),
        Some("settle") => json!({"ok": true, "op": "settle", "settled": request["changes"]}),
        _ => json!({"ok": false, "code": "bad-request", "error": "Not scripted."}),
    }
}

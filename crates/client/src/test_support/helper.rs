//! A stand-in for the helper's daemon, answering a test client's socket as a
//! verified hidden helper would, and for its input service when asked to.
use crate::client::Client;
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use std::{
    future::Future,
    path::Path,
    sync::{
        Arc, Mutex, MutexGuard, PoisonError,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    net::{UnixListener, UnixStream},
};

/// How the stand-in answers a request for the picture before an action.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pictures {
    /// With a frame of its session.
    Taken,
    /// With a refusal, as on screen.
    Refused,
    /// Never: the connection stays open with no answer.
    Late,
    /// Only once cancelled: then refused, as a daemon answers a cancel.
    Cancelled,
    /// With a frame of its session, only once the input came.
    AfterInput,
    /// With a frame's header at once, and its pixels only once the input
    /// came.
    PixelsAfterInput,
}

/// The helper's home, as its hello reports it.
const HOME: &str = "/Users/helper";

/// The security session the helper's identity proofs name.
const SESSION: u64 = 42;

/// The longest a test may take to finish against the stand-in.
const LIMIT: Duration = Duration::from_secs(15);

/// A run in the helper that exited with `code` after printing `output`.
pub fn exited(code: u8, output: &str) -> Value {
    json!({"ok": true, "exitCode": code, "timedOut": false, "signal": null,
        "events": [{"event": "output", "data": STANDARD.encode(output)}]})
}

/// A run in the helper that succeeded after printing `output`.
pub fn completed(output: &str) -> Value {
    exited(0, output)
}

/// The helper's socket, answering hello and the identity probe itself and
/// every other run through the test.
pub struct Helper {
    listener: UnixListener,
    /// The input service's socket, when it stands in for that too, and
    /// what that service says it can do.
    input: Option<UnixListener>,
    capabilities: Vec<String>,
    uid: u32,
    /// How many identity proofs name the first session before the rest
    /// name another, as after the helper logged in again.
    moves_after: Option<usize>,
    /// How many identity proofs it gave.
    proofs: AtomicUsize,
    requests: AtomicUsize,
    unavailable_after: Option<usize>,
    /// Whether it refuses a read meant for another session, as a current
    /// daemon does; an older one cannot be told the session.
    pins: bool,
    /// Whether it crops one window or an app's part of a region.
    parts: bool,
    /// How it answers a picture's frame request; none hands it to the test.
    pictures: Option<Pictures>,
    /// Each request's op as it came, and `frame` once a frame was sent.
    log: Arc<Mutex<Vec<String>>>,
}

impl Helper {
    /// A client of helper account `uid` and the stand-in serving it, both
    /// kept in `directory`. The client accepts the socket from the account
    /// running the test.
    pub fn client(directory: &Path, uid: u32) -> (Client, Self) {
        let mut client = super::client(directory);
        client.config.helper_uid = uid;
        client.transport.peer_uid = nix::unistd::getuid().as_raw();
        client.input_peer_uid = Some(nix::unistd::getuid().as_raw());
        std::fs::create_dir_all(client.transport.path.parent().unwrap()).unwrap();

        let helper = Self {
            listener: UnixListener::bind(&client.transport.path).unwrap(),
            input: None,
            capabilities: Vec::new(),
            uid,
            moves_after: None,
            proofs: AtomicUsize::new(0),
            requests: AtomicUsize::new(0),
            unavailable_after: None,
            pins: true,
            parts: true,
            pictures: None,
            log: Arc::default(),
        };

        (client, helper)
    }

    /// The daemon's process ID as the helper reports it: this process, which
    /// serves its socket, once it takes pictures, which come only from the
    /// very process verified; another otherwise.
    fn daemon_pid(&self) -> u32 {
        if self.pictures.is_some() {
            std::process::id()
        } else {
            123
        }
    }

    /// This helper, answering a picture's frame request as `pictures` says.
    pub fn taking_pictures(mut self, pictures: Pictures) -> Self {
        self.pictures = Some(pictures);
        self
    }

    /// The op of each request it was sent, in the order they came, with
    /// `frame sent` where it finished sending a picture's frame.
    pub fn log(&self) -> Vec<String> {
        self.logged().clone()
    }

    fn note(&self, entry: &str) {
        self.logged().push(entry.to_owned());
    }

    fn logged(&self) -> MutexGuard<'_, Vec<String>> {
        logged(&self.log)
    }

    /// Answer the request `line` on `peer`, a connection to the input service
    /// when `input`, handing to `answer` what the stand-in does not answer
    /// itself. A late picture's peer is kept in `held`, with what it is
    /// still to be sent once the input comes.
    async fn respond(
        &self,
        mut peer: BufReader<UnixStream>,
        line: &str,
        input: bool,
        answer: &mut impl FnMut(&Value) -> Option<Value>,
        held: &mut Vec<(UnixStream, Vec<u8>)>,
    ) {
        // A peer that left without asking, as a client refusing the peer
        // does, is asked nothing.
        if line.is_empty() {
            return;
        }
        let request: Value = serde_json::from_str(line).unwrap();
        let requests = self.requests.fetch_add(1, Ordering::Relaxed) + 1;
        self.note(request["op"].as_str().unwrap_or_default());
        if request["op"] == "input" {
            for (peer, rest) in held.iter_mut().filter(|(_, rest)| !rest.is_empty()) {
                // The client may have given up on it already.
                let _ = peer.write_all(&std::mem::take(rest)).await;
                self.note("frame released");
            }
        }
        if request["op"] == "frame"
            && let Some(pictures) = self.pictures
        {
            return self.picture(pictures, peer.into_inner(), held).await;
        }

        let reply = if self.unavailable_after.is_some_and(|limit| requests > limit) {
            None
        } else if request["op"] == "hello" && input {
            Some(self.input_hello())
        } else if request["op"] == "hello" {
            Some(self.hello())
        } else if request["argv"][1] == "session-info" {
            let proofs = self.proofs.fetch_add(1, Ordering::Relaxed) + 1;
            Some(self.proof(proofs))
        } else if let Some(refusal) = self.elsewhere(&request) {
            Some(refusal)
        } else {
            answer(&request)
        };

        if let Some(reply) = reply {
            peer.get_mut()
                .write_all(format!("{reply}\n").as_bytes())
                .await
                .unwrap();
        }
    }

    /// Answer a picture's frame request on `peer` as `pictures` says, framed
    /// as the daemon frames one. A late answer keeps the peer in `held`; a
    /// cancel is waited for beside the client's other requests, as the
    /// daemon serves each connection on its own.
    async fn picture(
        &self,
        pictures: Pictures,
        mut peer: UnixStream,
        held: &mut Vec<(UnixStream, Vec<u8>)>,
    ) {
        let header = match pictures {
            Pictures::Late => return held.push((peer, Vec::new())),
            Pictures::Cancelled => {
                let log = Arc::clone(&self.log);
                tokio::spawn(async move {
                    let mut cancel = [0_u8; 1];
                    if !matches!(peer.read(&mut cancel).await, Ok(1)) {
                        return;
                    }
                    logged(&log).push("frame cancelled".to_owned());
                    let refusal = json!({"ok": false, "code": "cancelled", "uncertain": false,
                        "error": "The picture was no longer wanted"});
                    // The client may have given up on it already.
                    let _ = peer.write_all(&framed(&refusal, &[])).await;
                });
                return;
            }
            Pictures::Refused => json!({"ok": false, "code": "on-console", "uncertain": false,
                "error": "The helper is on the screen"}),
            Pictures::Taken | Pictures::AfterInput | Pictures::PixelsAfterInput => {
                json!({"ok": true, "width": 2, "height": 1, "scale": 1.0,
                    "session": self.session_proof(self.proofs()), "daemonPID": std::process::id()})
            }
        };
        let pixels: &[u8] = if header["ok"] == true {
            &[11, 22, 33, 44, 55, 66]
        } else {
            &[]
        };
        let mut framed = framed(&header, pixels);
        let later = match pictures {
            Pictures::AfterInput => framed.split_off(0),
            Pictures::PixelsAfterInput => framed.split_off(framed.len() - pixels.len()),
            Pictures::Taken | Pictures::Refused | Pictures::Late | Pictures::Cancelled => {
                Vec::new()
            }
        };
        // The client may have given up on it already.
        let sent = peer.write_all(&framed).await.is_ok();
        if !later.is_empty() {
            held.push((peer, later));
        } else if sent && pictures == Pictures::Taken {
            self.note("frame sent");
        }
    }

    /// How many times the client verified this helper: the identity proofs
    /// it was given.
    pub fn proofs(&self) -> usize {
        self.proofs.load(Ordering::Relaxed)
    }

    pub fn requests(&self) -> usize {
        self.requests.load(Ordering::Relaxed)
    }

    /// [`Helper::client`] kept along the real path of `directory`, without
    /// a link in it, as requests for the helper are only staged there.
    pub fn staging(directory: &Path, uid: u32) -> (Client, Self) {
        Self::client(&std::fs::canonicalize(directory).unwrap(), uid)
    }

    /// This helper, whose session changes after its first `proofs`
    /// identity proofs.
    pub fn moving_after(mut self, proofs: usize) -> Self {
        self.moves_after = Some(proofs);
        self
    }

    /// This helper closes connections after answering `requests` requests.
    pub fn unavailable_after(mut self, requests: usize) -> Self {
        self.unavailable_after = Some(requests);
        self
    }

    /// This helper as an older daemon serves it: one that cannot be told
    /// which session a read is for.
    pub fn older(mut self) -> Self {
        self.pins = false;
        self
    }

    /// This helper as a daemon that pins reads to their session but cannot
    /// crop one window or an app's part of a region.
    pub fn without_parts(mut self) -> Self {
        self.parts = false;
        self
    }

    /// This helper with an input service on `client`'s input socket, which
    /// says hello with `capabilities` and hands every other request to the
    /// test as the daemon's are.
    pub fn serving_input(mut self, client: &Client, capabilities: &[&str]) -> Self {
        self.input = Some(UnixListener::bind(&client.config.input_socket).unwrap());
        self.capabilities = capabilities.iter().map(|&name| name.to_owned()).collect();
        self
    }

    /// The session this helper is in now: the one its proofs name next.
    fn session(&self) -> u64 {
        let given = self.proofs.load(Ordering::Relaxed);
        if self.moves_after.is_some_and(|proofs| given >= proofs) {
            SESSION + 1
        } else {
            SESSION
        }
    }

    /// The refusal of a read pinned to a session other than this helper's.
    fn elsewhere(&self, request: &Value) -> Option<Value> {
        let pin = request.get("session").filter(|_| self.pins)?;
        let here = json!({"securitySessionId": self.session(), "displayId": 7});
        (*pin != here).then(|| {
            json!({"ok": false, "code": "session-changed", "uncertain": false,
                "error": "The helper changed since Atlas checked it; look again"})
        })
    }

    /// Run `operation` while serving it. `answer` is given each run other
    /// than the identity probe and returns the reply, or `None` to close the
    /// connection without one.
    pub async fn serve<T>(
        &self,
        operation: impl Future<Output = T>,
        mut answer: impl FnMut(&Value) -> Option<Value>,
    ) -> T {
        let helper = async {
            let mut held = Vec::new();
            loop {
                let (peer, input) = match &self.input {
                    Some(input) => tokio::select! {
                        accepted = self.listener.accept() => (accepted.unwrap().0, false),
                        accepted = input.accept() => (accepted.unwrap().0, true),
                    },
                    None => (self.listener.accept().await.unwrap().0, false),
                };
                let mut peer = BufReader::new(peer);
                let mut line = String::new();
                {
                    // The picture before an input is asked for while the
                    // input service's connection waits for its request: the
                    // daemon answers meanwhile, as the helper's two
                    // processes would.
                    let read = peer.read_line(&mut line);
                    tokio::pin!(read);
                    loop {
                        tokio::select! {
                            read = &mut read => {
                                read.unwrap();
                                break;
                            }
                            accepted = self.listener.accept(), if input && self.pictures.is_some() => {
                                let mut beside = BufReader::new(accepted.unwrap().0);
                                let mut request = String::new();
                                beside.read_line(&mut request).await.unwrap();
                                self.respond(beside, &request, false, &mut answer, &mut held).await;
                            }
                        }
                    }
                }
                self.respond(peer, &line, input, &mut answer, &mut held)
                    .await;
            }
        };

        let served = async {
            tokio::select! {
                output = operation => output,
                _ = helper => unreachable!("the helper serves until the operation ends"),
            }
        };

        tokio::time::timeout(LIMIT, served)
            .await
            .expect("the operation finished")
    }

    fn hello(&self) -> Value {
        let capabilities: &[&str] = match (self.pins, self.parts) {
            (true, true) => &[
                atlas_core::protocol::SESSION_PIN,
                atlas_core::protocol::APP_PARTS,
            ],
            (true, false) => &[atlas_core::protocol::SESSION_PIN],
            (false, _) => &[],
        };
        json!({"ok": true, "user": {"uid": self.uid, "home": HOME},
            "daemon": {"pid": self.daemon_pid(), "capabilities": capabilities},
            "session": {"managerName": "Aqua", "onConsole": false},
            "display": {"width": 100, "height": 100, "scale": 1}})
    }

    /// The identity probe's answer, the `count`th this helper gave.
    fn proof(&self, count: usize) -> Value {
        completed(&self.session_proof(count).to_string())
    }

    /// The session the `count`th identity proof names.
    fn session_proof(&self, count: usize) -> Value {
        let moved = self.moves_after.is_some_and(|proofs| count > proofs);
        let session = if moved { SESSION + 1 } else { SESSION };
        json!({"ok": true, "uid": self.uid, "hasGraphicAccess": true,
            "parentPID": self.daemon_pid(), "onConsole": false, "securitySessionID": session,
            "displayID": 7, "width": 100, "height": 100})
    }

    /// The input service's hello: ready in the session the next proof names.
    fn input_hello(&self) -> Value {
        let next = self.proofs.load(Ordering::Relaxed) + 1;
        json!({"ok": true, "protocolVersion": 1, "pid": 456, "ready": true,
            "accessibility": true, "session": self.session_proof(next),
            "capabilities": self.capabilities})
    }
}

/// `log`, whoever holds it now.
fn logged(log: &Mutex<Vec<String>>) -> MutexGuard<'_, Vec<String>> {
    log.lock().unwrap_or_else(PoisonError::into_inner)
}

/// `header` framed as the daemon frames a picture, its length first, then
/// `pixels`.
fn framed(header: &Value, pixels: &[u8]) -> Vec<u8> {
    let header = header.to_string();
    let mut framed = u32::try_from(header.len()).unwrap().to_be_bytes().to_vec();
    framed.extend_from_slice(header.as_bytes());
    framed.extend_from_slice(pixels);
    framed
}

//! A stand-in for the helper's daemon, answering a test client's socket as a
//! verified hidden helper would.
use crate::client::Client;
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use std::{
    future::Future,
    path::Path,
    sync::atomic::{AtomicUsize, Ordering},
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::UnixListener,
};

/// The helper's home, as its hello reports it.
const HOME: &str = "/Users/helper";

/// The security session the helper's identity proofs name.
const SESSION: u64 = 42;

/// The longest a test may take to finish against the stand-in.
const LIMIT: Duration = Duration::from_secs(15);

/// Local processes standing in for apps in the distinct helper account.
struct LocalProcesses;

impl super::ProcessLookup for LocalProcesses {
    fn instances(&self, _uid: u32) -> crate::error::Result<Vec<crate::process::Instance>> {
        Ok(crate::process::of_user(nix::unistd::geteuid().as_raw())?
            .into_iter()
            .map(|info| info.instance)
            .collect())
    }

    fn start(&self, pid: i32) -> crate::error::Result<crate::process::Start> {
        crate::process::start(pid)
    }
}

/// A run in the helper that exited with `code` after printing `output`.
pub(crate) fn exited(code: u8, output: &str) -> Value {
    json!({"ok": true, "exitCode": code, "timedOut": false, "signal": null,
        "events": [{"event": "output", "data": STANDARD.encode(output)}]})
}

/// A run in the helper that succeeded after printing `output`.
pub(crate) fn completed(output: &str) -> Value {
    exited(0, output)
}

/// The helper's socket, answering hello and the identity probe itself and
/// every other run through the test.
pub(crate) struct Helper {
    listener: UnixListener,
    uid: u32,
    /// How many identity proofs name the first session before the rest
    /// name another, as after the helper logged in again.
    moves_after: Option<usize>,
    /// How many identity proofs it gave.
    proofs: AtomicUsize,
    /// Whether it refuses a read meant for another session, as a current
    /// daemon does; an older one cannot be told the session.
    pins: bool,
}

impl Helper {
    /// A client of helper account `uid` and the stand-in serving it, both
    /// kept in `directory`. The client accepts the socket from the account
    /// running the test.
    pub(crate) fn client(directory: &Path, uid: u32) -> (Client, Self) {
        let mut client = super::client(directory);
        client.config.helper_uid = uid;
        client.transport.peer_uid = nix::unistd::getuid().as_raw();
        client.input_peer_uid = Some(nix::unistd::getuid().as_raw());
        client.process_lookup = Some(std::sync::Arc::new(LocalProcesses));

        let helper = Self {
            listener: UnixListener::bind(&client.transport.path).unwrap(),
            uid,
            moves_after: None,
            proofs: AtomicUsize::new(0),
            pins: true,
        };

        (client, helper)
    }

    /// How many times the client verified this helper: the identity proofs
    /// it was given.
    pub(crate) fn proofs(&self) -> usize {
        self.proofs.load(Ordering::Relaxed)
    }

    /// [`Helper::client`] kept along the real path of `directory`, without
    /// a link in it, as requests for the helper are only staged there.
    pub(crate) fn staging(directory: &Path, uid: u32) -> (Client, Self) {
        Self::client(&std::fs::canonicalize(directory).unwrap(), uid)
    }

    /// This helper, whose session changes after its first `proofs`
    /// identity proofs.
    pub(crate) fn moving_after(mut self, proofs: usize) -> Self {
        self.moves_after = Some(proofs);
        self
    }

    /// This helper as an older daemon serves it: one that cannot be told
    /// which session a read is for.
    pub(crate) fn older(mut self) -> Self {
        self.pins = false;
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
    pub(crate) async fn serve<T>(
        &self,
        operation: impl Future<Output = T>,
        mut answer: impl FnMut(&Value) -> Option<Value>,
    ) -> T {
        let helper = async {
            loop {
                let (peer, _) = self.listener.accept().await.unwrap();
                let mut peer = BufReader::new(peer);
                let mut line = String::new();
                peer.read_line(&mut line).await.unwrap();
                let request: Value = serde_json::from_str(&line).unwrap();

                let reply = if request["op"] == "hello" {
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
        let capabilities: &[&str] = if self.pins {
            &[crate::daemon::protocol::SESSION_PIN]
        } else {
            &[]
        };
        json!({"ok": true, "user": {"uid": self.uid, "home": HOME},
            "daemon": {"pid": 123, "capabilities": capabilities},
            "session": {"managerName": "Aqua", "onConsole": false},
            "display": {"width": 100, "height": 100, "scale": 1}})
    }

    /// The identity probe's answer, the `count`th this helper gave.
    fn proof(&self, count: usize) -> Value {
        let moved = self.moves_after.is_some_and(|proofs| count > proofs);
        let session = if moved { SESSION + 1 } else { SESSION };
        let proof = json!({"ok": true, "uid": self.uid, "hasGraphicAccess": true,
            "parentPID": 123, "onConsole": false, "securitySessionID": session,
            "displayID": 7, "width": 100, "height": 100});

        completed(&proof.to_string())
    }
}

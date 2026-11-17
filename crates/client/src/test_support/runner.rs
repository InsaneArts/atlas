//! Atlas VM as a test stands it in on a VM's control socket: its control
//! protocol as Atlas VM's control service answers it, line for line. Each
//! change is kept as asked before it is done and answered from its record
//! when asked again, one change is done at a time, and what a change never
//! asked came to is answered with neither an outcome nor a pending. A test
//! sets the VM's power, and how a start goes: how long it takes, whether it
//! is refused and why, and whether its reply is lost after it is done.
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    io::{BufRead, BufReader, Write},
    os::unix::{fs::DirBuilderExt, net::UnixListener},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, PoisonError,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

/// What Atlas VM's control service says while another change is done.
const BUSY: &str = "Another change is being done; ask again once it is.";

/// A change as the stand-in keeps it: what was asked, and what it came to.
#[derive(Clone, Debug)]
struct Kept {
    op: String,
    outcome: Option<Value>,
}

/// What the stand-in knows, and how it is told to answer.
#[derive(Debug, Default)]
struct Known {
    power: String,
    ledger: BTreeMap<String, Kept>,
    busy: bool,
    asked: Vec<Value>,
    /// How long a start takes, the VM starting meanwhile.
    takes: Duration,
    /// Why each start is refused; none lets it start.
    refuses: Option<String>,
    /// How many start replies are lost once the start is done.
    drops: usize,
    /// A start held as if still on its way, by its change, and for how
    /// long, before it is read.
    queued: Option<(String, Duration)>,
    /// Every change begun, by its ID, in order.
    begun: Vec<String>,
    /// How many power reads are answered at once before each later one is
    /// held, and for how long.
    slow_states: Option<(usize, Duration)>,
    /// How many power reads were asked.
    states: usize,
}

/// Atlas VM, stood in for on a socket.
pub struct StandInRunner {
    known: Arc<Mutex<Known>>,
    socket: PathBuf,
    done: Arc<AtomicBool>,
}

impl StandInRunner {
    /// Atlas VM listening at `socket`, its folder made, the VM's power
    /// `power`.
    #[must_use]
    pub fn at(socket: &Path, power: &str) -> Self {
        if let Some(folder) = socket.parent() {
            std::fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(folder)
                .expect("the VM's folder");
        }
        let listener = UnixListener::bind(socket).expect("the VM's control socket");
        let known = Arc::new(Mutex::new(Known {
            power: power.to_owned(),
            ..Known::default()
        }));
        let done = Arc::new(AtomicBool::new(false));
        std::thread::spawn({
            let (known, done) = (known.clone(), done.clone());
            move || {
                for stream in listener.incoming() {
                    if done.load(Ordering::SeqCst) {
                        return;
                    }
                    let Ok(stream) = stream else { continue };
                    let known = known.clone();
                    std::thread::spawn(move || serve(&known, stream));
                }
            }
        });
        Self {
            known,
            socket: socket.to_owned(),
            done,
        }
    }

    fn known(&self) -> std::sync::MutexGuard<'_, Known> {
        self.known.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The VM's power now, as Atlas VM would say it.
    #[must_use]
    pub fn power(&self) -> String {
        self.known().power.clone()
    }

    /// Say the VM's power is `power`.
    pub fn set_power(&self, power: &str) {
        power.clone_into(&mut self.known().power);
    }

    /// Take `takes` for each start, the VM starting meanwhile.
    pub fn taking(&self, takes: Duration) {
        self.known().takes = takes;
    }

    /// Refuse each start for `reason`, or let it start again.
    pub fn refusing(&self, reason: Option<&str>) {
        self.known().refuses = reason.map(str::to_owned);
    }

    /// Lose the reply of the next `count` starts, once each is done.
    pub fn dropping(&self, count: usize) {
        self.known().drops = count;
    }

    /// Hold the start `change`, when it comes, for `delay` before it is
    /// read, as a request still on its way.
    pub fn queueing(&self, change: &str, delay: Duration) {
        self.known().queued = Some((change.to_owned(), delay));
    }

    /// Answer the first `after` power reads at once and hold each later one
    /// for `delay`, as an Atlas VM slow to say.
    pub fn slow_states(&self, after: usize, delay: Duration) {
        self.known().slow_states = Some((after, delay));
    }

    /// Every change begun, by its ID, in order: each was done once.
    #[must_use]
    pub fn begun(&self) -> Vec<String> {
        self.known().begun.clone()
    }

    /// Keep the change `change` as the operation `op`, asked by an earlier
    /// command, and what it came to: none while it is being done.
    pub fn keep(&self, change: &str, op: &str, outcome: Option<Value>) {
        self.known().ledger.insert(
            change.to_owned(),
            Kept {
                op: op.to_owned(),
                outcome,
            },
        );
    }

    /// Every request of operation `op` it was asked, in order.
    #[must_use]
    pub fn asked_for(&self, op: &str) -> Vec<Value> {
        self.known()
            .asked
            .iter()
            .filter(|asked| asked["op"] == op)
            .cloned()
            .collect()
    }
}

impl Drop for StandInRunner {
    fn drop(&mut self) {
        self.done.store(true, Ordering::SeqCst);
        let _ = std::os::unix::net::UnixStream::connect(&self.socket);
    }
}

/// The run that answers, as Atlas VM names itself on every reply.
fn incarnation() -> Value {
    json!({"pid": std::process::id(), "startSeconds": 1_700_000_000, "startMicroseconds": 5,
        "id": "0b0e1a5e-7a1d-4e3c-9a55-1a7a5f0e0001"})
}

fn served(op: &str) -> Value {
    json!({"ok": true, "op": op, "incarnation": incarnation()})
}

fn refused(reason: &str) -> Value {
    json!({"ok": false, "incarnation": incarnation(), "error": reason})
}

/// Answer one request line on `stream`, as Atlas VM's control service does.
fn serve(known: &Mutex<Known>, stream: std::os::unix::net::UnixStream) {
    let mut reader = BufReader::new(&stream);
    let mut line = String::new();
    if reader.read_line(&mut line).is_err() {
        return;
    }
    let Ok(request) = serde_json::from_str::<Value>(&line) else {
        let _ = (&stream).write_all(
            format!(
                "{}\n",
                refused("That is not a request a VM's runner serves.")
            )
            .as_bytes(),
        );
        return;
    };
    let lock = || known.lock().unwrap_or_else(PoisonError::into_inner);
    lock().asked.push(request.clone());
    let op = request["op"].as_str().unwrap_or_default().to_owned();
    let change = request["change"].as_str().unwrap_or_default().to_owned();
    let queued = {
        let mut known = lock();
        match known.queued.take() {
            Some((held, delay)) if held == change && op == "start" => Some(delay),
            other => {
                known.queued = other;
                None
            }
        }
    };
    if let Some(delay) = queued {
        std::thread::sleep(delay);
    }
    let reply = match op.as_str() {
        "hello" => {
            let mut reply = served("hello");
            reply["hello"] = json!({"version": 1, "capabilities": ["window"]});
            reply
        }
        "state" => {
            let held = {
                let mut known = lock();
                known.states += 1;
                known
                    .slow_states
                    .filter(|(after, _)| known.states > *after)
                    .map(|(_, delay)| delay)
            };
            if let Some(delay) = held {
                std::thread::sleep(delay);
            }
            let mut reply = served("state");
            reply["power"] = json!(lock().power);
            reply["confirmed"] = json!(true);
            reply["window"] = json!("closed");
            reply
        }
        "outcome" => {
            let mut reply = served("outcome");
            reply["change"] = json!(change);
            let mut known = lock();
            if let Some(kept) = known.ledger.get(&change).cloned() {
                answered(&mut known, &change, &kept, &mut reply);
            }
            reply
        }
        "start" | "stop" => match change_asked(known, &op, &change) {
            Some(reply) => reply,
            // Its reply lost on its way, the change done.
            None => return,
        },
        _ => refused("That is not a request a VM's runner serves."),
    };
    let _ = (&stream).write_all(format!("{reply}\n").as_bytes());
}

/// `reply`, answering for `kept`: one left pending is looked at again first
/// and settled when the VM's power tells, unless a change is being done.
fn answered(known: &mut Known, change: &str, kept: &Kept, reply: &mut Value) {
    let mut outcome = kept.outcome.clone();
    if outcome.is_none() && !known.busy {
        outcome = seen(&kept.op, &known.power);
        if let Some(entry) = known.ledger.get_mut(change) {
            entry.outcome.clone_from(&outcome);
        }
    }
    reply["pending"] = json!(outcome.is_none());
    if let Some(outcome) = outcome {
        reply["outcome"] = outcome;
    }
}

/// What the change `op` came to with the VM's power `power`; none while
/// that cannot tell.
fn seen(op: &str, power: &str) -> Option<Value> {
    match (op, power) {
        ("start", "starting") | ("stop", "stopping") | (_, "unknown") => None,
        ("start", "running") | ("stop", "stopped" | "absent") => Some(json!({"state": "done"})),
        _ => Some(json!({"state": "refused", "reason": "It did not take effect."})),
    }
}

/// The reply to the change `op` with the ID `change`: answered from its
/// record when it was asked before, refused unkept while another is done,
/// and done otherwise. None when its reply is lost.
fn change_asked(known: &Mutex<Known>, op: &str, change: &str) -> Option<Value> {
    let lock = || known.lock().unwrap_or_else(PoisonError::into_inner);
    let mut reply = served(op);
    reply["change"] = json!(change);
    let takes = {
        let mut known = lock();
        if let Some(kept) = known.ledger.get(change).cloned() {
            if kept.op != op {
                return Some(refused("Another change was asked with this ID."));
            }
            answered(&mut known, change, &kept, &mut reply);
            return Some(reply);
        }
        if known.busy {
            return Some(refused(BUSY));
        }
        known.busy = true;
        known.begun.push(change.to_owned());
        known.ledger.insert(
            change.to_owned(),
            Kept {
                op: op.to_owned(),
                outcome: None,
            },
        );
        if op == "start" {
            known.power = "starting".to_owned();
        }
        known.takes
    };
    std::thread::sleep(takes);
    let mut known = lock();
    let outcome = match (op, known.refuses.clone()) {
        ("start", Some(reason)) => {
            known.power = "stopped".to_owned();
            json!({"state": "refused", "reason": reason})
        }
        ("start", None) => {
            known.power = "running".to_owned();
            json!({"state": "done"})
        }
        _ => {
            known.power = "stopped".to_owned();
            json!({"state": "done"})
        }
    };
    if let Some(entry) = known.ledger.get_mut(change) {
        entry.outcome = Some(outcome.clone());
    }
    known.busy = false;
    if op == "start" && known.drops > 0 {
        known.drops -= 1;
        return None;
    }
    reply["outcome"] = outcome;
    reply["pending"] = json!(false);
    Some(reply)
}

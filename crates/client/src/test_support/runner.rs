//! Atlas VM as a test stands it in on a VM's control socket: its control
//! protocol as Atlas VM's control service answers it, line for line. Each
//! change is kept as asked before it is done and answered from its record
//! when asked again, one change is done at a time, and what a change never
//! asked came to is answered with neither an outcome nor a pending. A test
//! sets the VM's power, and how a start goes: how long it takes, whether it
//! is refused and why, and whether its reply is lost after it is done. Its
//! window opens and closes as a window change asks, and goes with the VM
//! as it stops or starts, each reply naming the run that began the change.
//! A test runs Atlas VM again as another run, its window gone with the
//! earlier one, and says whether that run sees every earlier run's VM gone:
//! an earlier run's window change it left is settled only once it does.
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

/// A change as the stand-in keeps it: what was asked, by which run, and
/// what it came to.
#[derive(Clone, Debug)]
struct Kept {
    op: String,
    run: String,
    outcome: Option<Value>,
}

/// The run of Atlas VM a stand-in answers as first.
pub const FIRST_RUN: &str = "0b0e1a5e-7a1d-4e3c-9a55-1a7a5f0e0001";

/// What a test looks at as a window change comes.
type Looking = Arc<dyn Fn() + Send + Sync>;

/// What the stand-in knows, and how it is told to answer.
#[derive(Default)]
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
    /// The run answering now.
    run: String,
    /// Whether its window is open.
    window: bool,
    /// Why each window change is refused; none lets it be done.
    refuses_window: Option<String>,
    /// How many window replies are lost once the change is done.
    drops_window: usize,
    /// Whether every request is taken and never read, as an Atlas VM that
    /// does not answer.
    silent: bool,
    /// Whether it goes silent at the next window request, that one first.
    silent_at_window: bool,
    /// What is looked at as each window change comes, before it is read.
    on_window: Option<Looking>,
    /// Whether it is of a build before it could void a change.
    older: bool,
    /// What a state says of whether every earlier run's VM is gone; none
    /// from a build before it said so.
    earlier_gone: Option<bool>,
    /// Whether a state says the VM's power is not seen just now.
    blind: bool,
    /// Whether a stop leaves the VM stopping, its outcome pending until the
    /// VM's power tells.
    stalls_stop: bool,
    /// How long each outcome read is held before it is answered.
    slow_outcomes: Option<Duration>,
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
            run: FIRST_RUN.to_owned(),
            earlier_gone: Some(true),
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

    /// Refuse each window change for `reason`, or let it be done again.
    pub fn refusing_window(&self, reason: Option<&str>) {
        self.known().refuses_window = reason.map(str::to_owned);
    }

    /// Lose the reply of the next `count` window changes, once each is done.
    pub fn dropping_window(&self, count: usize) {
        self.known().drops_window = count;
    }

    /// Take every request and never read it, or answer again.
    pub fn silent(&self, silent: bool) {
        self.known().silent = silent;
    }

    /// Answer as an Atlas VM of a build before it could void a change.
    pub fn older(&self) {
        self.known().older = true;
    }

    /// Look with `look` as each window change comes, before it is read.
    pub fn on_window(&self, look: impl Fn() + Send + Sync + 'static) {
        self.known().on_window = Some(Arc::new(look));
    }

    /// Go silent at the next window request, that one first.
    pub fn silent_at_window(&self) {
        self.known().silent_at_window = true;
    }

    /// Close its window as the person does by hand: nothing is asked of
    /// Atlas VM.
    pub fn close_by_hand(&self) {
        self.known().window = false;
    }

    /// Whether its window is open.
    #[must_use]
    pub fn window_open(&self) -> bool {
        self.known().window
    }

    /// Run Atlas VM again as the run `run`: its earlier run's window goes
    /// with it, and what that run kept stays kept as that run's. It cannot
    /// yet tell that earlier run's VM gone.
    pub fn runs_again_as(&self, run: &str) {
        let mut known = self.known();
        run.clone_into(&mut known.run);
        known.window = false;
        known.earlier_gone = Some(false);
    }

    /// Say in each state whether every earlier run's VM is gone, or, with
    /// none, nothing of it, as a build before it said so.
    pub fn saying_earlier_gone(&self, gone: Option<bool>) {
        self.known().earlier_gone = gone;
    }

    /// Say each power as not seen just now, or as seen again.
    pub fn blind(&self, blind: bool) {
        self.known().blind = blind;
    }

    /// Leave the VM stopping at each stop, its outcome pending until the
    /// VM's power tells.
    pub fn stalling_stop(&self) {
        self.known().stalls_stop = true;
    }

    /// Hold each outcome read for `delay` before it is answered, as an Atlas
    /// VM slow to say.
    pub fn slow_outcomes(&self, delay: Duration) {
        self.known().slow_outcomes = Some(delay);
    }

    /// Every change begun, by its ID, in order: each was done once.
    #[must_use]
    pub fn begun(&self) -> Vec<String> {
        self.known().begun.clone()
    }

    /// Keep the change `change` as the operation `op`, asked by an earlier
    /// command, and what it came to: none while it is being done.
    pub fn keep(&self, change: &str, op: &str, outcome: Option<Value>) {
        let mut known = self.known();
        let run = known.run.clone();
        known.ledger.insert(
            change.to_owned(),
            Kept {
                op: op.to_owned(),
                run,
                outcome,
            },
        );
    }

    /// The operation of every request it was asked, in order.
    #[must_use]
    pub fn ops(&self) -> Vec<String> {
        self.known()
            .asked
            .iter()
            .map(|asked| asked["op"].as_str().unwrap_or_default().to_owned())
            .collect()
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

/// The run `run` that answers, as Atlas VM names itself on every reply.
fn incarnation(run: &str) -> Value {
    json!({"pid": std::process::id(), "startSeconds": 1_700_000_000, "startMicroseconds": 5,
        "id": run})
}

fn served(op: &str, run: &str) -> Value {
    json!({"ok": true, "op": op, "incarnation": incarnation(run)})
}

fn refused(reason: &str, run: &str) -> Value {
    json!({"ok": false, "incarnation": incarnation(run), "error": reason})
}

/// Answer one request line on `stream`, as Atlas VM's control service does.
fn serve(known: &Mutex<Known>, stream: std::os::unix::net::UnixStream) {
    let mut reader = BufReader::new(&stream);
    let mut line = String::new();
    if reader.read_line(&mut line).is_err() {
        return;
    }
    let lock = || known.lock().unwrap_or_else(PoisonError::into_inner);
    let run = lock().run.clone();
    let Ok(request) = serde_json::from_str::<Value>(&line) else {
        let _ = (&stream).write_all(
            format!(
                "{}\n",
                refused("That is not a request a VM's runner serves.", &run)
            )
            .as_bytes(),
        );
        return;
    };
    if lock().silent {
        return;
    }
    if request["op"] == "window" {
        let look = lock().on_window.clone();
        if let Some(look) = look {
            look();
        }
        let mut known = lock();
        if known.silent_at_window {
            known.silent_at_window = false;
            known.silent = true;
            return;
        }
    }
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
            let mut reply = served("hello", &run);
            let capabilities = if lock().older {
                json!(["window-input"])
            } else {
                json!(["window-input", "void"])
            };
            reply["hello"] = json!({"version": 1, "capabilities": capabilities});
            reply
        }
        "void" if !lock().older => voided(&mut lock(), &change, &run),
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
            let mut reply = served("state", &run);
            let known = lock();
            reply["power"] = json!(known.power);
            reply["confirmed"] = json!(!known.blind);
            reply["window"] = json!(if known.window { "open" } else { "closed" });
            if let Some(gone) = known.earlier_gone {
                reply["earlierGone"] = json!(gone);
            }
            reply
        }
        "outcome" => {
            let held = lock().slow_outcomes;
            if let Some(delay) = held {
                std::thread::sleep(delay);
            }
            let mut reply = served("outcome", &run);
            reply["change"] = json!(change);
            let mut known = lock();
            if let Some(kept) = known.ledger.get(&change).cloned() {
                answered(&mut known, &change, &kept, &mut reply);
            }
            reply
        }
        "start" | "stop" | "window" => match change_asked(known, &request, &change) {
            Some(reply) => reply,
            // Its reply lost on its way, the change done.
            None => return,
        },
        _ => refused("That is not a request a VM's runner serves.", &run),
    };
    let _ = (&stream).write_all(format!("{reply}\n").as_bytes());
}

/// The reply to a void of `change`, by the run `run`: kept refused when it
/// was never asked, so a request of it that comes later is refused as
/// another change asked with its ID; answered as what it came to when it was.
fn voided(known: &mut Known, change: &str, run: &str) -> Value {
    let mut reply = served("void", run);
    reply["change"] = json!(change);
    let kept = known
        .ledger
        .entry(change.to_owned())
        .or_insert_with(|| Kept {
            op: "void".to_owned(),
            run: run.to_owned(),
            outcome: Some(json!({"state": "refused",
            "reason": "Voided before it was asked; it is never done."})),
        });
    let kept = kept.clone();
    answered(known, change, &kept, &mut reply);
    reply
}

/// `reply`, answering for `kept`: one left pending is looked at again first
/// and settled when the VM's power tells, unless a change is being done.
/// An earlier run's window change is settled only once this run sees every
/// earlier run's VM gone: no window of theirs is left, so on is refused and
/// off holds.
fn answered(known: &mut Known, change: &str, kept: &Kept, reply: &mut Value) {
    let mut outcome = kept.outcome.clone();
    reply["by"] = json!(kept.run);
    if outcome.is_none() && !known.busy {
        outcome = match kept.op.as_str() {
            "window-on" | "window-off" if kept.run != known.run => recovered(&kept.op, known),
            op => seen(op, known),
        };
        if let Some(entry) = known.ledger.get_mut(change) {
            entry.outcome.clone_from(&outcome);
        }
    }
    reply["pending"] = json!(outcome.is_none());
    if let Some(outcome) = outcome {
        reply["outcome"] = outcome;
    }
}

/// What an earlier run's window change `op` came to, as Atlas VM's
/// recovery tells it: none until every earlier run's VM is seen gone.
fn recovered(op: &str, known: &Known) -> Option<Value> {
    if known.earlier_gone != Some(true) {
        return None;
    }
    Some(if op == "window-on" {
        json!({"state": "refused", "reason": "Its runner went; its window is closed."})
    } else {
        json!({"state": "done"})
    })
}

/// What the change `op` came to as `known` has the VM now; none while that
/// cannot tell.
fn seen(op: &str, known: &Known) -> Option<Value> {
    let done = json!({"state": "done"});
    match (op, known.window) {
        ("window-on", true) | ("window-off", false) => return Some(done),
        ("window-on" | "window-off", _) => {
            return Some(json!({"state": "refused", "reason": "It did not take effect."}));
        }
        _ => {}
    }
    match (op, known.power.as_str()) {
        ("start", "starting") | ("stop", "stopping") | (_, "unknown") => None,
        ("start", "running") | ("stop", "stopped" | "absent") => Some(json!({"state": "done"})),
        _ => Some(json!({"state": "refused", "reason": "It did not take effect."})),
    }
}

/// The reply to the change `request` with the ID `change`: answered from
/// its record when it was asked before, refused unkept while another is
/// done, and done otherwise. None when its reply is lost.
fn change_asked(known: &Mutex<Known>, request: &Value, change: &str) -> Option<Value> {
    let lock = || known.lock().unwrap_or_else(PoisonError::into_inner);
    let op = match (request["op"].as_str(), request["input"].as_bool()) {
        (Some("window"), Some(true)) => "window-on",
        (Some("window"), _) => "window-off",
        (Some(op), _) => op,
        (None, _) => "",
    };
    let run = lock().run.clone();
    let mut reply = served(request["op"].as_str().unwrap_or_default(), &run);
    reply["change"] = json!(change);
    let takes = {
        let mut known = lock();
        if let Some(kept) = known.ledger.get(change).cloned() {
            if kept.op != op {
                return Some(refused("Another change was asked with this ID.", &run));
            }
            answered(&mut known, change, &kept, &mut reply);
            return Some(reply);
        }
        if known.busy {
            return Some(refused(BUSY, &run));
        }
        known.busy = true;
        known.begun.push(change.to_owned());
        known.ledger.insert(
            change.to_owned(),
            Kept {
                op: op.to_owned(),
                run: run.clone(),
                outcome: None,
            },
        );
        reply["by"] = json!(run);
        if op == "start" || op == "stop" {
            // Its window goes as the VM starts or stops.
            known.window = false;
        }
        if op == "start" {
            known.power = "starting".to_owned();
        }
        if op == "stop" && known.stalls_stop {
            known.power = "stopping".to_owned();
            known.busy = false;
            reply["pending"] = json!(true);
            return Some(reply);
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
        ("window-on" | "window-off", _) => match known.refuses_window.clone() {
            Some(reason) => json!({"state": "refused", "reason": reason}),
            None => {
                known.window = op == "window-on";
                json!({"state": "done"})
            }
        },
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
    if op.starts_with("window") && known.drops_window > 0 {
        known.drops_window -= 1;
        return None;
    }
    reply["outcome"] = outcome;
    reply["pending"] = json!(false);
    Some(reply)
}

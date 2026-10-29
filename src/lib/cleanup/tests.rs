//! The cleanup's tests, by area, and the stand-in helper, places and
//! helpers they share.
use super::*;
use crate::{
    app_selection::Process,
    environment::{InstallState, PlanState, Undone},
    host_session::{Host, HostSession},
    task::{Claims, store::Store as Tasks},
};
use crate::{
    error::{Code, Error},
    task::record::CleanupState,
};
use run::{Hands, Quitted};
use scope::Adopter;
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    fs,
    os::unix::fs::{DirBuilderExt, OpenOptionsExt},
    path::PathBuf,
};

mod adoption;
mod copies;
mod installs;
mod jobs;
mod order;
mod quit;
mod sessions;

/// A stand-in for the helper that logs every call. It has no way to list a
/// folder: a cleanup is given none.
#[derive(Default)]
struct Fake {
    root: PathBuf,
    /// The apps it lists, each `(pid, name, executable)`.
    apps: RefCell<Vec<(u64, String, String)>>,
    /// What undoing each plan answers; a plan not here is rolled back.
    undo: RefCell<BTreeMap<Token, Result<Undone>>>,
    /// What the helper says of each install; one not here is missing.
    installed: RefCell<BTreeMap<Token, Value>>,
    /// What removing each install answers; one not here is removed.
    removals: RefCell<BTreeMap<Token, Value>>,
    /// A plan whose undo never answers, as a run killed while it waits.
    hangs: Option<Token>,
    /// Whether quitting runs out of time.
    slow: bool,
    /// Whether the person stops agents once a plan is undone.
    stop_after_undo: bool,
    /// How each collection's copies stand, by receipt; one not here is
    /// whole.
    intact: RefCell<BTreeMap<String, crate::collect::Intact>>,
    /// What the helper shows of each job; one not here runs.
    jobs: RefCell<BTreeMap<String, Value>>,
    /// What stopping each job answers; one not here ends.
    job_stops: RefCell<BTreeMap<String, Result<Value>>>,
    /// The jobs the helper's own records give the session.
    daemon_jobs: RefCell<Vec<plan::Job>>,
    /// Whether the helper's list of its jobs cannot be read.
    unlisted: Cell<bool>,
    /// Whether an earlier outcome is unknown.
    unknown: Cell<bool>,
    stopped: Cell<bool>,
    log: RefCell<Vec<String>>,
}

impl Fake {
    fn at(root: &Path) -> Self {
        Self {
            root: root.to_owned(),
            ..Self::default()
        }
    }

    fn logged(&self) -> Vec<String> {
        self.log.borrow().clone()
    }

    fn running(&self, pid: u64, name: &str, executable: &str) {
        self.apps
            .borrow_mut()
            .push((pid, name.to_owned(), executable.to_owned()));
    }
}

impl Hands for Fake {
    fn copies(&self) -> &Path {
        &self.root
    }

    async fn apps(&self) -> Result<Value> {
        let apps: Vec<Value> = self
            .apps
            .borrow()
            .iter()
            .map(|(pid, name, executable)| {
                json!({"pid": pid, "name": name, "executable": executable})
            })
            .collect();
        Ok(json!({"ok": true, "apps": apps}))
    }

    async fn plan_states(&self, ids: &[Token]) -> Result<Vec<PlanState>> {
        self.log.borrow_mut().push("plan-states".into());
        Ok(ids
            .iter()
            .map(|_| PlanState {
                phase: crate::environment::PlanPhase::Applied,
                targets: vec![".config/probe/settings.toml".into()],
            })
            .collect())
    }

    async fn installs(&self, ids: &[Token]) -> Result<Vec<InstallState>> {
        Ok(ids
            .iter()
            .map(|id| {
                let said = self
                    .installed
                    .borrow()
                    .get(id)
                    .cloned()
                    .unwrap_or_else(|| json!({"installId": id, "phase": "missing"}));
                serde_json::from_value(said).unwrap()
            })
            .collect())
    }

    async fn remove_install(&self, install: &Token) -> Result<Value> {
        self.log.borrow_mut().push(format!("remove {install}"));
        Ok(self.removals.borrow_mut().remove(install).unwrap_or_else(
            || json!({"ok": true, "outcome": "held", "heldPath": held_path(install)}),
        ))
    }

    async fn quit(&self, processes: &[Process], _: &[String], _: &[Token]) -> Result<Quitted> {
        let pids: Vec<String> = processes
            .iter()
            .map(|process| process.pid.to_string())
            .collect();
        self.log
            .borrow_mut()
            .push(format!("quit {}", pids.join(" ")));
        if self.slow {
            return Ok(Quitted::TimedOut(
                "Probe did not quit within 10 seconds".into(),
            ));
        }
        self.apps
            .borrow_mut()
            .retain(|(pid, ..)| !processes.iter().any(|process| process.pid == *pid));
        Ok(Quitted::Gone)
    }

    async fn undo_plan(&self, plan: &Token) -> Result<Undone> {
        self.log.borrow_mut().push(format!("undo {plan}"));
        if self.hangs.as_ref() == Some(plan) {
            std::future::pending::<()>().await;
        }
        if self.stop_after_undo {
            self.stopped.set(true);
        }
        self.undo
            .borrow_mut()
            .remove(plan)
            .unwrap_or(Ok(Undone::RolledBack))
    }

    fn intact(&self, collection: &plan::Collection) -> crate::collect::Intact {
        self.log
            .borrow_mut()
            .push(format!("check {}", collection.receipt));
        self.intact
            .borrow()
            .get(collection.receipt.as_str())
            .cloned()
            .unwrap_or_default()
    }

    async fn job(
        &self,
        id: &crate::daemon::protocol::JobId,
    ) -> Result<crate::daemon::protocol::Shown> {
        let shown = self
            .jobs
            .borrow()
            .get(id.as_str())
            .cloned()
            .unwrap_or_else(|| json!({"id": id, "state": "running"}));
        Ok(serde_json::from_value(shown).unwrap())
    }

    async fn jobs_of(&self, _: &Key, task: Option<&Key>) -> Result<Vec<plan::Job>> {
        if self.unlisted.get() {
            return Err(Error::new(Code::Transport, "The helper did not answer"));
        }
        Ok(self
            .daemon_jobs
            .borrow()
            .iter()
            .filter(|job| task.is_none_or(|task| job.task == *task))
            .cloned()
            .collect())
    }

    async fn stop_job(&self, _: &Key, id: &crate::daemon::protocol::JobId) -> Result<Value> {
        self.log.borrow_mut().push(format!("stop job {id}"));
        self.job_stops
            .borrow_mut()
            .remove(id.as_str())
            .unwrap_or_else(|| Ok(json!({"ok": true, "id": id, "state": "ended", "left": []})))
    }

    fn stopped(&self) -> bool {
        self.stopped.get()
    }

    fn unknown(&self) -> bool {
        self.unknown.get()
    }

    fn interrupted(&self) -> bool {
        false
    }
}

/// A temporary output folder and copy root, both private.
struct Place {
    _directory: tempfile::TempDir,
    output: PathBuf,
    root: PathBuf,
}

impl Place {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let base = fs::canonicalize(directory.path()).unwrap();
        let (output, root) = (base.join("output"), base.join("copies"));
        for folder in [&output, &root] {
            fs::DirBuilder::new().mode(0o700).create(folder).unwrap();
        }
        Self {
            _directory: directory,
            output,
            root,
        }
    }

    /// A copy folder labelled for `session` (or for nobody), holding a file
    /// opened with TextEdit.
    fn copy(&self, session: Option<&Key>) -> Token {
        let token = Token::new();
        let folder = self.root.join(token.as_str());
        fs::DirBuilder::new().mode(0o700).create(&folder).unwrap();
        fs::write(folder.join("Secret plan.rtf"), "hello").unwrap();
        fs::write(folder.join(".opened-with"), "TextEdit").unwrap();
        if let Some(session) = session {
            use std::io::Write;
            fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .mode(0o600)
                .open(folder.join(".session"))
                .unwrap()
                .write_all(format!("{session}\n").as_bytes())
                .unwrap();
        }
        token
    }

    fn folder(&self, token: &Token) -> PathBuf {
        self.root.join(token.as_str())
    }

    /// Claim `owned` for `task`, made.
    fn claim(&self, task: &Task, owned: Owned) {
        Claims::in_store(Tasks::at(&self.output), task.clone())
            .claim(&owned, State::Made)
            .unwrap();
    }

    fn loaded(&self, scope: &Scope) -> Loaded {
        Loaded::of(&self.output, scope).unwrap()
    }

    async fn apply(&self, hands: &Fake, scope: &Scope) -> Value {
        apply(
            hands,
            &self.output,
            scope,
            &self.loaded(scope),
            Actor::Agent,
            None,
        )
        .await
        .unwrap()
    }

    fn receipt(&self, reply: &Value) -> Receipt {
        let id = Token::parse(reply["receipt"].as_str().unwrap()).unwrap();
        Receipts::at(&self.output).load(&id).unwrap()
    }
}

fn session(id: &str) -> Key {
    Key::of(Host::Codex, id)
}

fn own(session: &Key) -> Scope {
    Scope {
        session: session.clone(),
        task: None,
        adopting: false,
        named: false,
    }
}

fn default_task(session: &Key) -> Task {
    Task::of(session, None).unwrap()
}

fn plan_claimed(place: &Place, task: &Task) -> Token {
    let id = Token::new();
    place.claim(task, Owned::Plan { id: id.clone() });
    std::thread::sleep(std::time::Duration::from_millis(2));
    id
}

/// What each step of `reply` does, in order.
fn kinds(reply: &Value) -> Vec<String> {
    reply["steps"]
        .as_array()
        .unwrap()
        .iter()
        .map(|step| step["do"].as_str().unwrap().to_owned())
        .collect()
}

fn states(reply: &Value) -> Vec<String> {
    reply["steps"]
        .as_array()
        .unwrap()
        .iter()
        .map(|step| step["state"].as_str().unwrap().to_owned())
        .collect()
}

/// The helper's word on install `id` of Probe, with `processes` running
/// from it.
fn installed(id: &Token, processes: &[(u64, &str)]) -> Value {
    let processes: Vec<Value> = processes
        .iter()
        .map(|(pid, executable)| json!({"pid": pid, "executable": executable}))
        .collect();
    json!({"installId": id, "phase": "installed", "app": "Probe",
        "path": "/Users/helper/Applications/Probe.app", "present": true, "unchanged": true,
        "verified": true, "root": {"device": 1, "inode": 2},
        "processes": processes})
}

/// Where the helper holds install `install` once it is moved aside.
fn held_path(install: &Token) -> String {
    format!("/Users/atlas/.config/atlas/installs/held/{install}/Probe.app")
}

/// Note in `task`'s record a job it started, still running.
fn job_started(place: &Place, task: &Task, name: &str) -> crate::daemon::protocol::JobId {
    use crate::{
        daemon::protocol::{Expected, JobState},
        task::record::JobEntry,
    };
    let id = crate::daemon::protocol::JobId::new();
    let entry = JobEntry {
        id: id.clone(),
        name: name.into(),
        started_at: 1,
        expected: Expected {
            boot: "boot".into(),
            pid: 4312,
            start: [1, 2],
            pgid: 4312,
        },
        state: JobState::Running.into(),
        how: None,
        ended_at: None,
        later: serde_json::Map::new(),
    };
    Tasks::at(&place.output)
        .update(task, 1, |record| {
            record.noted_job(entry);
            Ok(())
        })
        .unwrap();
    id
}

/// Note in `task`'s record a collection it made.
fn collected(place: &Place, task: &Task) -> crate::collect::ReceiptId {
    let receipt: crate::collect::ReceiptId =
        serde_json::from_value(json!(format!("{}-a1b2c3", crate::host_session::now_ms()))).unwrap();
    let entry = crate::task::record::OutputEntry {
        receipt: receipt.clone(),
        at: 1,
        files: 1,
        bytes: 3,
        later: serde_json::Map::new(),
    };
    Tasks::at(&place.output)
        .update(task, 1, |record| {
            record.noted_output(entry);
            Ok(())
        })
        .unwrap();
    receipt
}

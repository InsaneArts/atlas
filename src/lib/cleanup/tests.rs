use super::*;
use crate::{
    app_selection::Process,
    environment::{PlanState, Undone},
    host_session::{Host, HostSession},
    task::{Claims, store::Store as Tasks},
};
use crate::{error::Error, task::record::CleanupState};
use run::{Hands, Quitted};
use scope::Adopter;
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    fs,
    os::unix::fs::{DirBuilderExt, OpenOptionsExt},
    path::PathBuf,
};

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

    async fn installs(&self, ids: &[Token]) -> Result<Value> {
        let installs: Vec<Value> = ids
            .iter()
            .map(|id| {
                self.installed
                    .borrow()
                    .get(id)
                    .cloned()
                    .unwrap_or_else(|| json!({"installId": id, "phase": "missing"}))
            })
            .collect();
        Ok(json!({"ok": true, "installs": installs}))
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

    async fn job(&self, id: &crate::daemon::protocol::JobId) -> Result<Value> {
        Ok(self
            .jobs
            .borrow()
            .get(id.as_str())
            .cloned()
            .unwrap_or_else(|| json!({"id": id, "state": "running"})))
    }

    async fn jobs_of(&self, _: &Key, task: Option<&Key>) -> Result<Vec<plan::Job>> {
        if self.unlisted.get() {
            return Err(Error::new("transport", "The helper did not answer"));
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

#[tokio::test]
async fn steps_run_quit_then_plans_newest_first_then_installs_then_copies() {
    let place = Place::new();
    let key = session("order");
    let task = default_task(&key);
    let copy = place.copy(Some(&key));
    place.claim(&task, Owned::Copy { id: copy.clone() });
    let install = Token::new();
    place.claim(
        &task,
        Owned::Install {
            id: install.clone(),
            app: "Probe".into(),
        },
    );
    let older = plan_claimed(&place, &task);
    let newer = plan_claimed(&place, &task);
    let fake = Fake::at(&place.root);
    let executable = place
        .folder(&copy)
        .join("TextEdit.app/Contents/MacOS/TextEdit");
    fake.running(812, "TextEdit", &executable.to_string_lossy());
    fake.installed.borrow_mut().insert(
        install.clone(),
        installed(
            &install,
            &[(
                813,
                "/Users/helper/Applications/Probe.app/Contents/MacOS/Probe",
            )],
        ),
    );
    fake.running(
        813,
        "Probe",
        "/Users/helper/Applications/Probe.app/Contents/MacOS/Probe",
    );

    let reply = place.apply(&fake, &own(&key)).await;

    assert_eq!(
        kinds(&reply),
        [
            "quit",
            "undo-plan",
            "undo-plan",
            "remove-install",
            "remove-copy"
        ]
    );
    assert_eq!(reply["steps"][1]["planId"], newer.as_str());
    assert_eq!(reply["steps"][2]["planId"], older.as_str());
    assert_eq!(reply["steps"][3]["installId"], install.as_str());
    assert_eq!(reply["steps"][3]["app"], "Probe");
    let receipt = place.receipt(&reply);
    assert_eq!(
        receipt.steps[3].after,
        [1, 2, 3],
        "after the quit and both plans"
    );
    assert_eq!(
        fake.logged(),
        [
            "quit 813 812".to_owned(),
            format!("undo {newer}"),
            format!("undo {older}"),
            format!("remove {install}"),
        ]
    );
    assert_eq!(reply["state"], "complete");
    assert_eq!(reply["ok"], true);
    assert!(!place.folder(&copy).exists());
    let record = Tasks::at(&place.output).load(task.key()).unwrap();
    assert!(record.claims().all(|(_, state)| state == State::Gone));
    assert!(!record.holds_anything());
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

#[tokio::test]
async fn an_install_that_changed_is_kept_and_one_in_use_is_tried_again() {
    let place = Place::new();
    let key = session("installs");
    let task = default_task(&key);
    let (changed, busy) = (Token::new(), Token::new());
    for id in [&changed, &busy] {
        place.claim(
            &task,
            Owned::Install {
                id: id.clone(),
                app: "Probe".into(),
            },
        );
    }
    let fake = Fake::at(&place.root);
    fake.removals.borrow_mut().insert(
        changed.clone(),
        json!({"ok": true, "outcome": "kept", "reason": "changed",
            "sideEffects": {"kept": [{"path": "Library/Preferences/test.atlas.install-probe.plist",
                "existedBefore": false}]}}),
    );
    fake.removals
        .borrow_mut()
        .insert(busy.clone(), json!({"ok": true, "outcome": "inUse"}));

    let reply = place.apply(&fake, &own(&key)).await;
    assert_eq!(states(&reply), ["done", "kept", "failed"]);
    assert_eq!(reply["steps"][1]["reason"], "changed");
    assert_eq!(
        reply["steps"][1]["sideEffects"]["kept"][0]["existedBefore"],
        false
    );
    assert_eq!(
        (&reply["steps"][2]["reason"], &reply["steps"][2]["retry"]),
        (&json!("in-use"), &json!(true))
    );
    assert_eq!(reply["state"], "incomplete");
    let record = Tasks::at(&place.output).load(task.key()).unwrap();
    let state = |id: &Token| {
        record
            .claims()
            .find(|(owned, _)| {
                owned.same(&Owned::Install {
                    id: id.clone(),
                    app: String::new(),
                })
            })
            .map(|(_, state)| state)
    };
    assert_eq!(
        (state(&changed), state(&busy)),
        (Some(State::Kept), Some(State::Made))
    );
}

#[tokio::test]
async fn a_cleanup_cut_between_steps_resumes_from_its_receipt_without_listing_any_folder() {
    let place = Place::new();
    let key = session("cut");
    let task = default_task(&key);
    let copy = place.copy(Some(&key));
    place.claim(&task, Owned::Copy { id: copy.clone() });
    let plan = plan_claimed(&place, &task);
    // Folders the records do not name are never looked at, let alone
    // removed.
    let stranger = place.copy(Some(&key));

    let mut first = Fake::at(&place.root);
    first.hangs = Some(plan.clone());
    let executable = place
        .folder(&copy)
        .join("TextEdit.app/Contents/MacOS/TextEdit");
    first.running(812, "TextEdit", &executable.to_string_lossy());
    let scope = own(&key);
    let cut = tokio::time::timeout(
        std::time::Duration::from_millis(200),
        place.apply(&first, &scope),
    )
    .await;
    assert!(cut.is_err(), "the run was dropped while it waited");

    let note = Tasks::at(&place.output)
        .load(task.key())
        .unwrap()
        .unfinished_cleanup()
        .cloned()
        .unwrap();
    let written = Receipts::at(&place.output).load(&note.id).unwrap();
    assert_eq!(written.steps[0].state, StepState::Done);
    assert_eq!(written.steps[1].state, StepState::Started);

    let second = Fake::at(&place.root);
    let reply = place.apply(&second, &scope).await;
    assert_eq!(reply["receipt"], note.id.as_str(), "the same receipt");
    assert_eq!(second.logged(), [format!("undo {plan}")]);
    assert_eq!(reply["state"], "complete");
    assert!(place.folder(&stranger).exists());
}

#[tokio::test]
async fn a_removal_that_finished_before_the_crash_counts_as_done_not_as_a_failure() {
    let place = Place::new();
    let key = session("finished");
    let task = default_task(&key);
    let copy = place.copy(Some(&key));
    place.claim(&task, Owned::Copy { id: copy.clone() });
    let fake = Fake::at(&place.root);

    // The step was written started, then its folder went, then the run
    // died before it wrote what happened.
    let scope = own(&key);
    let loaded = place.loaded(&scope);
    let mut receipt = Receipt::new(key.clone(), None, Actor::Agent);
    plan::merge(&mut receipt.steps, &loaded.undoable());
    let removal = receipt.steps.len() - 1;
    receipt.steps[removal].set(StepState::Started, None);
    for step in &mut receipt.steps[..removal] {
        step.set(StepState::Done, None);
    }
    Receipts::at(&place.output).save(&mut receipt).unwrap();
    note(
        &Tasks::at(&place.output),
        &loaded.covered(&scope),
        &receipt,
        None,
    )
    .unwrap();
    fs::remove_dir_all(place.folder(&copy)).unwrap();

    let reply = place.apply(&fake, &scope).await;
    let step = &reply["steps"][removal];
    assert_eq!(step["state"], "done");
    assert_eq!(step["alreadyGone"], true);
    assert_eq!(reply["ok"], true);
}

#[tokio::test]
async fn a_step_waiting_on_a_failed_step_is_not_run() {
    let place = Place::new();
    let key = session("waiting");
    let task = default_task(&key);
    let copy = place.copy(Some(&key));
    place.claim(&task, Owned::Copy { id: copy.clone() });
    let plan = plan_claimed(&place, &task);
    let mut fake = Fake::at(&place.root);
    fake.slow = true;
    let executable = place
        .folder(&copy)
        .join("TextEdit.app/Contents/MacOS/TextEdit");
    fake.running(812, "TextEdit", &executable.to_string_lossy());

    let reply = place.apply(&fake, &own(&key)).await;

    assert_eq!(states(&reply), ["failed", "failed", "failed"]);
    assert_eq!(reply["steps"][0]["reason"], "timeout");
    assert_eq!(reply["steps"][1]["reason"], "needs-step");
    assert_eq!(reply["steps"][2]["reason"], "needs-step");
    assert_eq!(reply["steps"][1]["retry"], true);
    assert!(!fake.logged().contains(&format!("undo {plan}")));
    assert!(place.folder(&copy).exists());
    assert_eq!(
        (&reply["ok"], &reply["code"]),
        (&json!(false), &json!("cleanup"))
    );
    assert_eq!(reply["uncertain"], false);
}

#[tokio::test]
async fn an_uncertain_step_ends_the_run_and_is_never_taken_for_a_failure() {
    let place = Place::new();
    let key = session("uncertain");
    let task = default_task(&key);
    let older = plan_claimed(&place, &task);
    let newer = plan_claimed(&place, &task);
    let fake = Fake::at(&place.root);
    fake.undo.borrow_mut().insert(
        newer.clone(),
        Err(Error::new("transport", "The reply was lost").uncertain()),
    );

    let reply = place.apply(&fake, &own(&key)).await;

    assert_eq!(states(&reply), ["uncertain", "pending"]);
    assert_eq!(fake.logged(), [format!("undo {newer}")]);
    assert_eq!(
        (&reply["state"], &reply["uncertain"]),
        (&json!("uncertain"), &json!(true))
    );
    let record = Tasks::at(&place.output).load(task.key()).unwrap();
    assert!(record.claims().any(
        |(owned, state)| owned.same(&Owned::Plan { id: older.clone() }) && state == State::Made
    ));
    assert_eq!(
        record.unfinished_cleanup().map(|note| note.state),
        Some(CleanupState::Uncertain)
    );
}

#[tokio::test]
async fn a_stop_between_steps_blocks_the_rest() {
    let place = Place::new();
    let key = session("stopped");
    let task = default_task(&key);
    let copy = place.copy(Some(&key));
    place.claim(&task, Owned::Copy { id: copy.clone() });
    plan_claimed(&place, &task);
    let mut fake = Fake::at(&place.root);
    fake.stop_after_undo = true;

    let reply = place.apply(&fake, &own(&key)).await;

    assert_eq!(states(&reply), ["done", "done", "blocked"]);
    assert_eq!(reply["steps"][2]["reason"], "stopped");
    assert_eq!(reply["state"], "blocked");
    assert!(place.folder(&copy).exists());
}

#[tokio::test]
async fn kept_and_failed_are_reported_apart() {
    let place = Place::new();
    let key = session("kept");
    let task = default_task(&key);
    let plan = plan_claimed(&place, &task);
    let fake = Fake::at(&place.root);
    fake.undo.borrow_mut().insert(
        plan.clone(),
        Ok(Undone::Kept {
            targets: vec![".config/probe/settings.toml".into()],
            directories: 0,
        }),
    );

    let reply = place.apply(&fake, &own(&key)).await;
    assert_eq!(states(&reply), ["kept"]);
    assert_eq!(reply["steps"][0]["reason"], "kept-targets");
    assert_eq!(
        reply["steps"][0]["targets"],
        json!([".config/probe/settings.toml"])
    );
    assert_eq!(
        (&reply["ok"], &reply["state"]),
        (&json!(true), &json!("complete"))
    );
    let record = Tasks::at(&place.output).load(task.key()).unwrap();
    assert!(record.claims().all(|(_, state)| state == State::Kept));

    let other = session("failed");
    let task = default_task(&other);
    let copy = place.copy(Some(&other));
    place.claim(&task, Owned::Copy { id: copy.clone() });
    let fake = Fake::at(&place.root);
    // An app that runs by the copy's name keeps it in use.
    fake.running(
        9,
        "TextEdit",
        "/System/Applications/TextEdit.app/Contents/MacOS/TextEdit",
    );
    let reply = place.apply(&fake, &own(&other)).await;
    assert_eq!(states(&reply), ["done", "failed"]);
    assert_eq!(reply["steps"][1]["reason"], "in-use");
    assert_eq!(reply["state"], "incomplete");
    assert_eq!(fake.logged(), Vec::<String>::new(), "nothing ran from it");
}

#[tokio::test]
async fn new_work_after_an_unfinished_cleanup_is_added_to_the_same_receipt() {
    let place = Place::new();
    let key = session("more");
    let task = default_task(&key);
    let first = plan_claimed(&place, &task);
    let fake = Fake::at(&place.root);
    fake.undo
        .borrow_mut()
        .insert(first.clone(), Ok(Undone::Unfinished("Busy".into())));
    let scope = own(&key);
    let earlier = place.apply(&fake, &scope).await;
    assert_eq!(earlier["state"], "incomplete");

    let copy = place.copy(Some(&key));
    place.claim(&task, Owned::Copy { id: copy.clone() });
    let later = plan_claimed(&place, &task);
    let fake = Fake::at(&place.root);
    let reply = place.apply(&fake, &scope).await;

    assert_eq!(reply["receipt"], earlier["receipt"]);
    assert_eq!(
        kinds(&reply),
        ["quit", "undo-plan", "undo-plan", "remove-copy"]
    );
    assert_eq!(reply["steps"][1]["planId"], later.as_str(), "newest first");
    let receipt = place.receipt(&reply);
    let numbers: Vec<u32> = receipt.steps.iter().map(|step| step.n).collect();
    assert_eq!(numbers, [2, 3, 1, 4], "the first step keeps its number");
    assert_eq!(reply["state"], "complete");
}

#[tokio::test]
async fn apps_opened_by_name_are_listed_and_never_quit() {
    let place = Place::new();
    let key = session("launch");
    let task = default_task(&key);
    place.claim(
        &task,
        Owned::Launch {
            app: "TextEdit".into(),
        },
    );
    let fake = Fake::at(&place.root);
    fake.running(
        812,
        "TextEdit",
        "/System/Applications/TextEdit.app/Contents/MacOS/TextEdit",
    );

    let reply = place.apply(&fake, &own(&key)).await;
    assert_eq!(reply["steps"], json!([]));
    assert_eq!(
        reply["left"],
        json!([{"app": "TextEdit", "why": "opened by name"}])
    );
    assert!(fake.logged().is_empty());
}

#[tokio::test]
async fn an_app_opened_by_name_that_no_longer_runs_is_not_left() {
    let place = Place::new();
    let key = session("quit-since");
    let task = default_task(&key);
    let install = Token::new();
    for owned in [
        Owned::Launch {
            app: "Probe".into(),
        },
        Owned::Launch {
            app: "TextEdit".into(),
        },
        Owned::Install {
            id: install.clone(),
            app: "Probe".into(),
        },
    ] {
        place.claim(&task, owned);
    }
    let fake = Fake::at(&place.root);
    fake.running(
        812,
        "textedit",
        "/System/Applications/TextEdit.app/Contents/MacOS/TextEdit",
    );
    let left = json!([{"app": "TextEdit", "why": "opened by name"}]);

    let scope = own(&key);
    let preview = preview(&fake, &place.output, &scope, &place.loaded(&scope))
        .await
        .unwrap();
    assert_eq!(preview["left"], left, "Probe is not running");
    let reply = place.apply(&fake, &own(&key)).await;
    assert_eq!(reply["left"], left);
    let nothing = place.apply(&fake, &own(&key)).await;
    assert_eq!(nothing["steps"], json!([]));
    assert_eq!(nothing["left"], left, "said with nothing left to do too");
}

#[tokio::test]
async fn processes_are_chosen_by_their_folder_never_by_name() {
    let place = Place::new();
    let key = session("folder");
    let task = default_task(&key);
    let copy = place.copy(Some(&key));
    place.claim(&task, Owned::Copy { id: copy.clone() });
    let fake = Fake::at(&place.root);
    let inside = place.folder(&copy).join("Probe.app/Contents/MacOS/Probe");
    fake.running(812, "Probe", &inside.to_string_lossy());
    fake.running(900, "Probe", "/Applications/Probe.app/Contents/MacOS/Probe");
    let beside = format!("{}-other/Probe", place.folder(&copy).display());
    fake.running(901, "Other", &beside);

    let reply = place.apply(&fake, &own(&key)).await;
    assert_eq!(fake.logged(), ["quit 812"]);
    assert_eq!(states(&reply)[0], "done");
}

#[tokio::test]
async fn a_copy_labelled_for_another_session_is_kept_even_when_this_record_lists_it() {
    let place = Place::new();
    let (key, other) = (session("mine"), session("theirs"));
    let task = default_task(&key);
    let theirs = place.copy(Some(&other));
    place.claim(&task, Owned::Copy { id: theirs.clone() });
    let fake = Fake::at(&place.root);

    let reply = place.apply(&fake, &own(&key)).await;
    assert_eq!(states(&reply), ["done", "kept"]);
    assert_eq!(reply["steps"][1]["reason"], "another-session");
    assert!(place.folder(&theirs).exists());
    assert_eq!(reply["ok"], true);
}

#[tokio::test]
async fn a_session_key_never_widens_what_is_removed() {
    let place = Place::new();
    let (forger, victim) = (session("forger"), session("victim"));
    let victims = place.copy(Some(&victim));
    let unreadable = place.copy(None);
    fs::write(place.folder(&unreadable).join(".session"), "../x\n").unwrap();
    // A record that names another session's copies and one whose label
    // cannot be read, as a forged one would.
    let task = default_task(&forger);
    place.claim(
        &task,
        Owned::Copy {
            id: victims.clone(),
        },
    );
    place.claim(
        &task,
        Owned::Copy {
            id: unreadable.clone(),
        },
    );
    let fake = Fake::at(&place.root);

    let reply = place.apply(&fake, &own(&forger)).await;
    assert_eq!(states(&reply), ["done", "kept", "kept"]);
    assert!(place.folder(&victims).exists());
    assert!(place.folder(&unreadable).exists());
    assert_eq!(reply["steps"][2]["reason"], "label-unreadable");
}

#[tokio::test]
async fn a_preview_changes_nothing_and_takes_no_lock() {
    let place = Place::new();
    let key = session("preview");
    let task = default_task(&key);
    let copy = place.copy(Some(&key));
    place.claim(&task, Owned::Copy { id: copy.clone() });
    let plan = plan_claimed(&place, &task);
    let before = fs::read(place.output.join(format!("tasks/{}.json", task.key()))).unwrap();
    let fake = Fake::at(&place.root);
    let executable = place
        .folder(&copy)
        .join("TextEdit.app/Contents/MacOS/TextEdit");
    fake.running(812, "TextEdit", &executable.to_string_lossy());

    let scope = own(&key);
    let reply = preview(&fake, &place.output, &scope, &place.loaded(&scope))
        .await
        .unwrap();

    assert_eq!(reply["preview"], true);
    assert_eq!(kinds(&reply), ["quit", "undo-plan", "remove-copy"]);
    assert_eq!(
        reply["steps"][0]["apps"],
        json!([{"app": "TextEdit", "pid": 812}])
    );
    assert_eq!(reply["steps"][1]["planId"], plan.as_str());
    assert_eq!(reply["steps"][1]["planState"], "applied");
    assert_eq!(reply["steps"][2]["present"], true);
    assert!(reply["steps"][0].get("state").is_none(), "nothing ran yet");
    assert_eq!(fake.logged(), ["plan-states"], "nothing was quit or undone");
    assert!(place.folder(&copy).exists());
    assert!(!place.output.join("cleanup").exists(), "no receipt");
    assert_eq!(
        fs::read(place.output.join(format!("tasks/{}.json", task.key()))).unwrap(),
        before
    );
}

#[test]
fn adoption_is_refused_while_the_session_is_active_and_allowed_for_the_person() {
    let directory = tempfile::tempdir().unwrap();
    let client = crate::transaction_test::client(directory.path());
    let host = HostSession::new(Host::Codex, "neighbour").unwrap();
    crate::host_session::store::note(
        &client,
        &host,
        crate::host_session::store::Kind::Other,
        Some(&crate::doing::Doing::new("click")),
        &Ok(json!({"ok": true})),
    )
    .unwrap();
    let output = client.output_dir().unwrap();
    let record = Sessions::at(output).read(host.key()).unwrap();

    let error = may_adopt(Some(&record), &[]).unwrap_err();
    assert_eq!(error.code, "input");
    assert!(error.error.contains("may still be working"), "{error}");

    let mut ended = record.clone();
    ended.ended = Some(ended.last_seen);
    assert!(may_adopt(Some(&ended), &[]).is_ok());
    assert!(may_adopt(None, &[]).is_ok());

    let person = Scope::for_person(host.key());
    assert!(person.adopting && person.named && person.task.is_none());
}

#[tokio::test]
async fn an_adopted_session_is_noted_in_its_records_and_its_labels_are_checked_against_it() {
    let place = Place::new();
    let (old, new) = (session("ended"), session("adopter"));
    let task = default_task(&old);
    let copy = place.copy(Some(&old));
    place.claim(&task, Owned::Copy { id: copy.clone() });
    let scope = Scope {
        session: old.clone(),
        task: None,
        adopting: true,
        named: true,
    };
    let fake = Fake::at(&place.root);

    let reply = apply(
        &fake,
        &place.output,
        &scope,
        &place.loaded(&scope),
        Actor::Agent,
        Some(Adopter::Session(new.clone())),
    )
    .await
    .unwrap();

    assert_eq!(reply["ok"], true);
    assert!(!place.folder(&copy).exists());
    let record = Tasks::at(&place.output).load(task.key()).unwrap();
    assert_eq!(
        record.adopted_by.map(|adoption| adoption.key),
        Some(Some(new.clone()))
    );
    assert_eq!(place.receipt(&reply).adopted_by, Some(new));
    assert!(
        Tasks::at(&place.output)
            .of_session(&session("adopter"))
            .is_empty(),
        "nothing moves into the adopting session"
    );
}

#[tokio::test]
async fn a_receipt_names_no_file_on_this_mac() {
    let place = Place::new();
    let key = session("names");
    let task = default_task(&key);
    let copy = place.copy(Some(&key));
    place.claim(&task, Owned::Copy { id: copy.clone() });
    let plan = plan_claimed(&place, &task);
    let fake = Fake::at(&place.root);
    fake.undo.borrow_mut().insert(
        plan,
        Ok(Undone::Kept {
            targets: vec![".config/probe/settings.toml".into()],
            directories: 1,
        }),
    );

    let reply = place.apply(&fake, &own(&key)).await;
    let id = reply["receipt"].as_str().unwrap();
    let bytes = fs::read_to_string(place.output.join(format!("cleanup/{id}.json"))).unwrap();
    // Paths inside the helper's home are kept; nothing of this Mac is.
    assert!(bytes.contains(".config/probe/settings.toml"));
    for named in ["Secret", "rtf", "/copies", "/Users/"] {
        assert!(!bytes.contains(named), "{named} in {bytes}");
    }
}

#[tokio::test]
async fn a_cleanup_of_the_session_covers_every_task_and_what_its_record_lists() {
    let directory = tempfile::tempdir().unwrap();
    let client = crate::transaction_test::client(directory.path());
    let output = client.output_dir().unwrap().to_owned();
    let root = fs::canonicalize(directory.path()).unwrap().join("copies");
    fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
    let host = HostSession::new(Host::Codex, "whole").unwrap();
    let key = host.key().clone();

    // A copy only the session's own record lists, as a build without task
    // records left it.
    let recorded = Token::new();
    crate::host_session::store::note(
        &client,
        &host,
        crate::host_session::store::Kind::Open,
        Some(&crate::doing::Doing::new("open")),
        &Ok(json!({"ok": true, "copied": [{"to": format!("{}/{recorded}/a.rtf", handoff::ROOT)}]})),
    )
    .unwrap();
    let place = Place {
        _directory: tempfile::tempdir().unwrap(),
        output: output.clone(),
        root,
    };
    let named = [
        Task::of(&key, Some("one")).unwrap(),
        Task::of(&key, Some("two")).unwrap(),
    ];
    let default = default_task(&key);
    let plans: Vec<Token> = named
        .iter()
        .chain([&default])
        .map(|task| plan_claimed(&place, task))
        .collect();

    let fake = Fake::at(&place.root);
    let reply = place.apply(&fake, &own(&key)).await;

    let mut undone: Vec<String> = fake.logged();
    undone.sort();
    let mut expected: Vec<String> = plans.iter().map(|plan| format!("undo {plan}")).collect();
    expected.sort();
    assert_eq!(undone, expected, "each plan once");
    let copies: Vec<&Value> = reply["steps"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|step| step["do"] == "remove-copy")
        .collect();
    assert_eq!(copies.len(), 1);
    assert_eq!(copies[0]["alreadyGone"], true);
    assert_eq!(reply["state"], "complete");
}

#[tokio::test]
async fn a_cleanup_of_one_task_leaves_the_others_alone() {
    let place = Place::new();
    let key = session("split");
    let (probe, other) = (
        Task::of(&key, Some("probe")).unwrap(),
        Task::of(&key, Some("other")).unwrap(),
    );
    let mine = plan_claimed(&place, &probe);
    let theirs = plan_claimed(&place, &other);
    let scope = Scope {
        session: key.clone(),
        task: Some(probe.key().clone()),
        adopting: false,
        named: true,
    };
    let fake = Fake::at(&place.root);

    let reply = place.apply(&fake, &scope).await;
    assert_eq!(fake.logged(), [format!("undo {mine}")]);
    assert_eq!(reply["task"]["name"], "probe");
    let untouched = Tasks::at(&place.output).load(other.key()).unwrap();
    assert!(untouched.claims().any(
        |(owned, state)| owned.same(&Owned::Plan { id: theirs.clone() }) && state == State::Made
    ));
    assert!(untouched.cleanups.is_empty());
}

#[test]
fn a_key_that_names_no_record_is_said_and_no_session_is_a_usage_error() {
    let place = Place::new();
    let scope = Scope {
        session: session("nobody"),
        task: None,
        adopting: true,
        named: true,
    };
    assert_eq!(
        Loaded::of(&place.output, &scope)
            .err()
            .map(|error| error.code),
        Some("input")
    );

    let directory = tempfile::tempdir().unwrap();
    let mut client = crate::transaction_test::client(directory.path());
    let error = Scope::of(&client, None, None).unwrap_err();
    assert_eq!(error.code, "usage");
    assert!(error.error.contains("ATLAS_SESSION"), "{error}");

    let host = HostSession::new(Host::Codex, "named").unwrap();
    client.task = Task::resolve(Some(&host), Some(std::ffi::OsStr::new("probe"))).unwrap();
    client.host_session = Some(host.clone());
    let scope = Scope::of(&client, None, None).unwrap();
    assert_eq!(scope.task.as_ref(), client.task.as_ref().map(Task::key));
    assert!(!scope.adopting && !scope.named);
    let elsewhere = Scope::of(&client, Some(&session("other")), None).unwrap();
    assert_eq!((elsewhere.task, elsewhere.adopting), (None, true));
}

#[test]
fn a_task_atlas_task_names_before_it_has_a_record_has_nothing_to_clean_up() {
    let place = Place::new();
    let key = session("fresh");
    let scope = Scope {
        session: key.clone(),
        task: Some(Task::of(&key, Some("probe")).unwrap().key().clone()),
        adopting: false,
        named: false,
    };
    let loaded = Loaded::of(&place.output, &scope).unwrap();
    assert!(loaded.undoable().is_empty());
}

#[tokio::test]
async fn nothing_runs_from_a_foreign_copy_or_a_replaced_install_is_quit() {
    let place = Place::new();
    let (key, other) = (session("quitter"), session("neighbour"));
    let task = default_task(&key);
    let theirs = place.copy(Some(&other));
    place.claim(&task, Owned::Copy { id: theirs.clone() });
    let install = Token::new();
    place.claim(
        &task,
        Owned::Install {
            id: install.clone(),
            app: "Probe".into(),
        },
    );
    let fake = Fake::at(&place.root);
    let inside = place
        .folder(&theirs)
        .join("TextEdit.app/Contents/MacOS/TextEdit");
    fake.running(812, "TextEdit", &inside.to_string_lossy());
    // The helper no longer finds its own folder at the install's place:
    // what runs there is someone else's.
    let replaced = "/Users/helper/Applications/Probe.app/Contents/MacOS/Probe";
    fake.running(813, "Probe", replaced);
    fake.installed.borrow_mut().insert(
        install.clone(),
        json!({"installId": install, "phase": "installed", "app": "Probe", "verified": false}),
    );
    fake.removals.borrow_mut().insert(
        install.clone(),
        json!({"ok": true, "outcome": "kept", "reason": "replaced"}),
    );

    let reply = place.apply(&fake, &own(&key)).await;
    assert!(
        fake.logged().iter().all(|call| !call.starts_with("quit")),
        "{:?}",
        fake.logged()
    );
    assert_eq!(reply["steps"][0]["state"], "done");
    assert!(place.folder(&theirs).exists());
    assert_eq!(fake.apps.borrow().len(), 2, "both still run");
}

#[tokio::test]
async fn an_app_running_from_a_copy_macos_cannot_trace_is_left_never_quit() {
    let place = Place::new();
    let key = session("translocated");
    let task = default_task(&key);
    let install = Token::new();
    place.claim(
        &task,
        Owned::Install {
            id: install.clone(),
            app: "Probe".into(),
        },
    );
    let fake = Fake::at(&place.root);
    let mut answer = installed(&install, &[]);
    answer["unproven"] = json!([{"pid": 900, "executable": "/private/var/folders/x/AppTranslocation/1/d/Probe.app/Contents/MacOS/Probe"}]);
    fake.installed.borrow_mut().insert(install.clone(), answer);
    fake.removals
        .borrow_mut()
        .insert(install.clone(), json!({"ok": true, "outcome": "inUse"}));

    let reply = place.apply(&fake, &own(&key)).await;
    assert!(fake.logged().iter().all(|call| !call.starts_with("quit")));
    assert_eq!(
        reply["left"],
        json!([{"app": "Probe", "why": "runs from a copy macOS moved it to"}])
    );
    assert_eq!(reply["steps"][1]["reason"], "in-use");
}

#[tokio::test]
async fn new_work_after_a_failed_quit_waits_for_every_quit() {
    let place = Place::new();
    let key = session("rewire");
    let task = default_task(&key);
    let first = place.copy(Some(&key));
    place.claim(&task, Owned::Copy { id: first.clone() });
    let plan = plan_claimed(&place, &task);
    let mut slow = Fake::at(&place.root);
    slow.slow = true;
    let running = place
        .folder(&first)
        .join("TextEdit.app/Contents/MacOS/TextEdit");
    slow.running(812, "TextEdit", &running.to_string_lossy());
    let scope = own(&key);
    let earlier = place.apply(&slow, &scope).await;
    assert_eq!(states(&earlier), ["failed", "failed", "failed"]);

    let second = place.copy(Some(&key));
    place.claim(&task, Owned::Copy { id: second.clone() });
    let mut still_slow = Fake::at(&place.root);
    still_slow.slow = true;
    still_slow.running(812, "TextEdit", &running.to_string_lossy());
    let reply = place.apply(&still_slow, &scope).await;

    let receipt = place.receipt(&reply);
    let quits: Vec<u32> = receipt
        .steps
        .iter()
        .filter(|entry| matches!(entry.step, Step::Quit { .. }))
        .map(|entry| entry.n)
        .collect();
    assert_eq!(quits.len(), 2);
    for entry in receipt
        .steps
        .iter()
        .filter(|entry| !matches!(entry.step, Step::Quit { .. }))
    {
        assert_eq!(entry.after, quits, "{:?} waits for both quits", entry.step);
    }
    assert!(!still_slow.logged().contains(&format!("undo {plan}")));
}

/// Where the helper holds install `install` once it is moved aside.
fn held_path(install: &Token) -> String {
    format!("/Users/atlas/.config/atlas/installs/held/{install}/Probe.app")
}

#[tokio::test]
async fn an_install_is_moved_aside_in_the_helper_by_its_path_and_a_copy_is_removed() {
    let place = Place::new();
    let key = session("held");
    let task = default_task(&key);
    let copy = place.copy(Some(&key));
    place.claim(&task, Owned::Copy { id: copy.clone() });
    let install = Token::new();
    place.claim(
        &task,
        Owned::Install {
            id: install.clone(),
            app: "Probe".into(),
        },
    );
    let fake = Fake::at(&place.root);

    let reply = place.apply(&fake, &own(&key)).await;
    let held = held_path(&install);
    assert_eq!(reply["steps"][1]["held"], json!(held));
    assert!(reply["steps"][1].get("alreadyGone").is_none());
    assert_eq!(reply["steps"][2]["state"], "done");
    assert!(reply["steps"][2].get("held").is_none());
    assert!(!place.folder(&copy).exists(), "a copy is removed");
    let receipt = place.receipt(&reply);
    let kept: Vec<Option<&str>> = receipt
        .steps
        .iter()
        .map(|entry| entry.came_to.held.as_deref())
        .collect();
    assert_eq!(kept, [None, Some(held.as_str()), None]);
}

#[tokio::test]
async fn an_install_removed_by_hand_is_reported_already_gone() {
    let place = Place::new();
    let key = session("by-hand");
    let task = default_task(&key);
    let install = Token::new();
    place.claim(
        &task,
        Owned::Install {
            id: install.clone(),
            app: "Probe".into(),
        },
    );
    let fake = Fake::at(&place.root);
    fake.removals.borrow_mut().insert(
        install.clone(),
        json!({"ok": true, "installId": install, "outcome": "alreadyGone", "app": "Probe"}),
    );

    let reply = place.apply(&fake, &own(&key)).await;
    let step = &reply["steps"][1];
    assert_eq!(step["do"], "remove-install");
    assert_eq!(step["alreadyGone"], true, "{step}");
    assert!(step.get("held").is_none());
    assert_eq!(reply["ok"], true);
}

#[tokio::test]
async fn a_kept_step_is_settled_in_the_records_of_its_own_receipts_task() {
    let place = Place::new();
    let key = session("two-tasks");
    let (first, second) = (
        Task::of(&key, Some("first")).unwrap(),
        Task::of(&key, Some("second")).unwrap(),
    );
    let theirs = place.copy(Some(&session("elsewhere")));
    place.claim(&first, Owned::Copy { id: theirs.clone() });
    let scope_of = |task: &Task| Scope {
        session: key.clone(),
        task: Some(task.key().clone()),
        adopting: false,
        named: true,
    };
    let fake = Fake::at(&place.root);
    let kept = place.apply(&fake, &scope_of(&first)).await;
    assert_eq!(kept["steps"][1]["reason"], "another-session");

    // As a crash between the kept outcome and settling it leaves things.
    let mut receipt = place.receipt(&kept);
    for entry in &mut receipt.steps {
        entry.settled = false;
    }
    Receipts::at(&place.output).save(&mut receipt).unwrap();
    place.claim(&first, Owned::Copy { id: theirs.clone() });
    let later = plan_claimed(&place, &second);

    let fake = Fake::at(&place.root);
    place.apply(&fake, &scope_of(&second)).await;
    let first_record = Tasks::at(&place.output).load(first.key()).unwrap();
    assert!(
        first_record.claims().any(
            |(owned, state)| owned.same(&Owned::Copy { id: theirs.clone() })
                && state == State::Kept
        ),
        "settled where it was claimed"
    );
    let second_record = Tasks::at(&place.output).load(second.key()).unwrap();
    assert!(
        !second_record
            .claims()
            .any(|(owned, _)| owned.same(&Owned::Copy { id: theirs.clone() }))
    );
    assert!(second_record.claims().any(|(owned, state)| {
        owned.same(&Owned::Plan { id: later.clone() }) && state == State::Gone
    }));
    let settled = Receipts::at(&place.output).load(&receipt.id).unwrap();
    assert!(settled.steps.iter().all(|entry| entry.settled));
}

#[tokio::test]
async fn a_step_finished_before_its_claim_was_settled_is_settled_not_planned_again() {
    let place = Place::new();
    let key = session("settle");
    let task = default_task(&key);
    let copy = place.copy(Some(&key));
    place.claim(&task, Owned::Copy { id: copy.clone() });
    let scope = own(&key);
    let fake = Fake::at(&place.root);
    let first = place.apply(&fake, &scope).await;
    assert_eq!(first["state"], "complete");

    // As a crash between the receipt's last write and settling leaves it:
    // the receipt says done, the claim still says made.
    let mut receipt = place.receipt(&first);
    for entry in &mut receipt.steps {
        entry.settled = false;
    }
    Receipts::at(&place.output).save(&mut receipt).unwrap();
    place.claim(&task, Owned::Copy { id: copy.clone() });

    let fake = Fake::at(&place.root);
    let second = place.apply(&fake, &scope).await;
    assert_eq!(second["steps"], json!([]), "nothing planned again");
    assert!(fake.logged().is_empty());
    let record = Tasks::at(&place.output).load(task.key()).unwrap();
    assert!(record.claims().all(|(_, state)| state == State::Gone));
    let settled = Receipts::at(&place.output).load(&receipt.id).unwrap();
    assert!(settled.steps.iter().all(|entry| entry.settled));
}

#[tokio::test]
async fn what_a_finished_step_came_to_survives_a_later_run() {
    let place = Place::new();
    let key = session("kept-later");
    let task = default_task(&key);
    let copy = place.copy(Some(&key));
    place.claim(&task, Owned::Copy { id: copy.clone() });
    let plan = plan_claimed(&place, &task);
    let fake = Fake::at(&place.root);
    fake.undo.borrow_mut().insert(
        plan,
        Ok(Undone::Kept {
            targets: vec![".config/probe/settings.toml".into()],
            directories: 0,
        }),
    );
    fake.running(
        9,
        "TextEdit",
        "/System/Applications/TextEdit.app/Contents/MacOS/TextEdit",
    );
    let scope = own(&key);
    let first = place.apply(&fake, &scope).await;
    assert_eq!(first["state"], "incomplete");

    let later = Fake::at(&place.root);
    let second = place.apply(&later, &scope).await;
    assert_eq!(second["state"], "complete");
    assert_eq!(
        second["steps"][1]["targets"],
        json!([".config/probe/settings.toml"]),
        "{second}"
    );
}

#[test]
fn fresh_task_records_keep_a_session_without_its_own_record_from_being_adopted() {
    let task = default_task(&session("records-only"));
    let mut record = crate::task::record::Record::new(&task, now_ms());
    let error = may_adopt(None, std::slice::from_ref(&record)).unwrap_err();
    assert_eq!(error.code, "input");

    record.updated = now_ms() - 20 * 60 * 1000;
    assert!(may_adopt(None, std::slice::from_ref(&record)).is_ok());
    record.updated = now_ms();
    record.adopted_by = Some(Adoption {
        key: None,
        at: now_ms(),
    });
    assert!(
        may_adopt(None, std::slice::from_ref(&record)).is_ok(),
        "a cleanup's own notes never count"
    );
}

/// A client whose controller waits for control, and an agent session that
/// ended a while ago and left an app opened by name.
fn ended_neighbour(directory: &Path) -> (Client, HostSession) {
    use crate::host_session::store::{Kind, note};

    let mut client = crate::transaction_test::client(directory);
    client.controller = client
        .controller
        .waiting(std::time::Duration::from_secs(10));
    client.host_session = Some(HostSession::new(Host::Codex, "adopter").unwrap());
    let neighbour = HostSession::new(Host::Codex, "neighbour").unwrap();
    note(
        &client,
        &neighbour,
        Kind::Other,
        Some(&crate::doing::Doing::new("click")),
        &Ok(json!({"ok": true})),
    )
    .unwrap();
    let output = client.output_dir().unwrap().to_owned();
    let long_ago = now_ms() - 30 * 60 * 1000;
    Sessions::at(&output)
        .update(&neighbour, long_ago, &client.controller, |record| {
            record.last_seen = long_ago;
            record.ended = Some(long_ago);
        })
        .unwrap();
    Tasks::at(&output)
        .update(&default_task(neighbour.key()), long_ago, |record| {
            record.claim(
                &Owned::Launch {
                    app: "TextEdit".into(),
                },
                State::Made,
                long_ago,
            )
        })
        .unwrap();
    (client, neighbour)
}

#[tokio::test]
async fn an_agent_adopts_a_session_that_has_ended() {
    let directory = tempfile::tempdir().unwrap();
    let (client, neighbour) = ended_neighbour(directory.path());
    let scope = Scope::of(&client, Some(neighbour.key()), None).unwrap();

    let reply = apply_scope(&client, &scope).await.unwrap();
    assert_eq!((&reply["ok"], &reply["steps"]), (&json!(true), &json!([])));
    assert_eq!(reply["left"][0]["app"], "TextEdit");
}

#[tokio::test]
async fn a_session_that_acts_while_adoption_waits_for_control_is_not_adopted() {
    use crate::host_session::store::{Kind, note};

    let directory = tempfile::tempdir().unwrap();
    let (client, neighbour) = ended_neighbour(directory.path());
    let scope = Scope::of(&client, Some(neighbour.key()), None).unwrap();
    let held = client.controller.acquire(false).unwrap();
    let acting = client.clone();
    let waker = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(300));
        note(
            &acting,
            &neighbour,
            Kind::Other,
            Some(&crate::doing::Doing::new("click")),
            &Ok(json!({"ok": true})),
        )
        .unwrap();
        drop(held);
    });

    let error = apply_scope(&client, &scope).await.unwrap_err();
    waker.join().unwrap();
    assert_eq!(error.code, "input");
    assert!(error.error.contains("may still be working"), "{error}");
    let output = client.output_dir().unwrap();
    assert!(
        Receipts::at(output).of_session(&scope.session).is_empty(),
        "nothing recorded"
    );
}

/// Note in `task`'s record a job it started, still running.
fn job_started(place: &Place, task: &Task, name: &str) -> crate::daemon::protocol::JobId {
    use crate::{
        daemon::{job::JobState, protocol::Expected},
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

fn copy_of(source: &str, in_home: &str) -> crate::collect::Kept {
    crate::collect::Kept {
        source: source.into(),
        in_home: Some(in_home.into()),
        path: PathBuf::from("/nowhere/copy"),
        size: 3,
        sha256: "00".into(),
        read: None,
    }
}

#[tokio::test]
async fn outputs_are_checked_first_and_jobs_stop_after_quits_before_plans() {
    let place = Place::new();
    let key = session("jobs-order");
    let task = default_task(&key);
    let copy = place.copy(Some(&key));
    place.claim(&task, Owned::Copy { id: copy.clone() });
    let plan = plan_claimed(&place, &task);
    let job = job_started(&place, &task, "web");
    let receipt = collected(&place, &task);
    let fake = Fake::at(&place.root);

    let reply = place.apply(&fake, &own(&key)).await;
    assert_eq!(reply["ok"], true, "{reply}");
    assert_eq!(
        kinds(&reply),
        [
            "check-outputs",
            "quit",
            "stop-job",
            "undo-plan",
            "remove-copy"
        ]
    );
    assert_eq!(reply["steps"][2]["name"], "web");
    let log = fake.logged();
    let at = |what: &str| log.iter().position(|line| line == what).unwrap();
    assert!(at(&format!("check {receipt}")) < at(&format!("stop job {job}")));
    assert!(at(&format!("stop job {job}")) < at(&format!("undo {plan}")));

    let receipt = place.receipt(&reply);
    let stop = receipt
        .steps
        .iter()
        .find(|entry| matches!(entry.step, Step::StopJob { .. }))
        .unwrap();
    let undo = receipt
        .steps
        .iter()
        .find(|entry| matches!(entry.step, Step::UndoPlan { .. }))
        .unwrap();
    assert!(
        undo.after.contains(&stop.n),
        "a plan waits for the job's stop"
    );
}

#[tokio::test]
async fn a_job_that_already_ended_is_done_without_a_second_signal() {
    let place = Place::new();
    let key = session("ended-job");
    let task = default_task(&key);
    let job = job_started(&place, &task, "worker");
    let fake = Fake::at(&place.root);
    fake.jobs.borrow_mut().insert(
        job.to_string(),
        json!({"id": job, "state": "ended", "how": "stopped", "left": []}),
    );

    let reply = place.apply(&fake, &own(&key)).await;
    assert_eq!(reply["ok"], true, "{reply}");
    assert_eq!(states(&reply), ["done"]);
    assert!(
        !fake
            .logged()
            .iter()
            .any(|line| line.starts_with("stop job")),
        "nothing was sent again"
    );
}

#[tokio::test]
async fn a_job_step_says_what_left_and_why_it_could_not_stop() {
    let place = Place::new();
    let key = session("job-left");
    let task = default_task(&key);
    let left = job_started(&place, &task, "server");
    let stubborn = job_started(&place, &task, "stubborn");
    let foreign = job_started(&place, &task, "setuid");
    let fake = Fake::at(&place.root);
    fake.job_stops.borrow_mut().insert(
        left.to_string(),
        Ok(json!({"ok": true, "state": "ended", "left": [{"pid": 77, "name": "sleep"}]})),
    );
    fake.job_stops.borrow_mut().insert(
        stubborn.to_string(),
        Ok(json!({"ok": false, "state": "stopping", "reason": "still-stopping"})),
    );
    fake.job_stops.borrow_mut().insert(
        foreign.to_string(),
        Err(Error::new("job", "Part of the job runs as another account")
            .with("reason", "other-account")),
    );

    let reply = place.apply(&fake, &own(&key)).await;
    assert_eq!(reply["ok"], false);
    assert_eq!(reply["code"], "cleanup");
    let step = |n: usize| &reply["steps"][n];
    assert_eq!(step(0)["state"], "done");
    assert_eq!(step(0)["left"], json!([{"pid": 77, "name": "sleep"}]));
    assert_eq!(
        (&step(1)["state"], &step(1)["reason"], &step(1)["retry"]),
        (&json!("failed"), &json!("still-stopping"), &json!(true))
    );
    assert_eq!(
        (&step(2)["state"], &step(2)["reason"]),
        (&json!("failed"), &json!("other-account"))
    );
    assert!(step(2).get("retry").is_none(), "nothing more is tried");
}

#[tokio::test]
async fn a_job_whose_helper_cannot_be_reached_is_blocked_and_a_lost_stop_is_uncertain() {
    let place = Place::new();
    let key = session("job-blocked");
    let task = default_task(&key);
    let unreachable = job_started(&place, &task, "a");
    let fake = Fake::at(&place.root);
    fake.job_stops.borrow_mut().insert(
        unreachable.to_string(),
        Err(Error::new("daemon", "The helper is running an older Atlas")),
    );
    let reply = place.apply(&fake, &own(&key)).await;
    assert_eq!(reply["state"], "blocked", "{reply}");
    assert_eq!(reply["steps"][0]["reason"], "daemon");

    let place = Place::new();
    let task = default_task(&key);
    let lost = job_started(&place, &task, "b");
    let fake = Fake::at(&place.root);
    fake.job_stops.borrow_mut().insert(
        lost.to_string(),
        Err(Error::new("transport", "Helper exceeded the request deadline").uncertain()),
    );
    let reply = place.apply(&fake, &own(&key)).await;
    assert_eq!(reply["state"], "uncertain", "{reply}");
    assert_eq!(reply["uncertain"], true);
}

#[tokio::test]
async fn a_copy_that_is_missing_blocks_removing_its_source_and_nothing_else() {
    let place = Place::new();
    let key = session("not-kept");
    let task = default_task(&key);
    let touching = plan_claimed(&place, &task);
    let receipt = collected(&place, &task);
    let fake = Fake::at(&place.root);
    // The fake's plans copy .config/probe/settings.toml into the helper.
    fake.intact.borrow_mut().insert(
        receipt.to_string(),
        crate::collect::Intact {
            missing: vec![copy_of(
                "/Users/atlas/.config/probe/settings.toml",
                ".config/probe/settings.toml",
            )],
            ..Default::default()
        },
    );

    let reply = place.apply(&fake, &own(&key)).await;
    assert_eq!(reply["ok"], false, "{reply}");
    assert_eq!(kinds(&reply), ["check-outputs", "undo-plan"]);
    assert_eq!(reply["steps"][0]["state"], "failed");
    assert_eq!(reply["steps"][0]["reason"], "output-not-kept");
    assert_eq!(
        reply["steps"][0]["notKept"][0]["source"],
        "/Users/atlas/.config/probe/settings.toml"
    );
    assert_eq!(
        (&reply["steps"][1]["state"], &reply["steps"][1]["reason"]),
        (&json!("blocked"), &json!("output-not-kept"))
    );
    assert!(
        !fake.logged().contains(&format!("undo {touching}")),
        "the source was kept"
    );
    assert!(reply["error"].as_str().unwrap().contains("atlas collect"));

    // Collected again, the file is kept by its newer copy.
    let again = collected(&place, &task);
    fake.intact.borrow_mut().insert(
        again.to_string(),
        crate::collect::Intact {
            kept: vec![copy_of(
                "/Users/atlas/.config/probe/settings.toml",
                ".config/probe/settings.toml",
            )],
            ..Default::default()
        },
    );
    let reply = place.apply(&fake, &own(&key)).await;
    assert_eq!(reply["ok"], true, "{reply}");
    assert!(fake.logged().contains(&format!("undo {touching}")));
}

#[tokio::test]
async fn a_copy_of_a_file_no_step_removes_lets_every_step_run() {
    let place = Place::new();
    let key = session("elsewhere");
    let task = default_task(&key);
    let plan = plan_claimed(&place, &task);
    let receipt = collected(&place, &task);
    let fake = Fake::at(&place.root);
    fake.intact.borrow_mut().insert(
        receipt.to_string(),
        crate::collect::Intact {
            changed: vec![copy_of(
                "/Users/atlas/atlas-work/report.pdf",
                "atlas-work/report.pdf",
            )],
            ..Default::default()
        },
    );
    let reply = place.apply(&fake, &own(&key)).await;
    assert_eq!(states(&reply), ["failed", "done"], "{reply}");
    assert!(fake.logged().contains(&format!("undo {plan}")));
    assert_eq!(reply["state"], "incomplete");
}

#[tokio::test]
async fn a_receipt_that_is_gone_keeps_every_source_and_job_stops_go_on() {
    let place = Place::new();
    let key = session("receipt-gone");
    let task = default_task(&key);
    let plan = plan_claimed(&place, &task);
    let job = job_started(&place, &task, "web");
    let receipt = collected(&place, &task);
    let fake = Fake::at(&place.root);
    fake.intact.borrow_mut().insert(
        receipt.to_string(),
        crate::collect::Intact {
            unknown: true,
            ..Default::default()
        },
    );
    let reply = place.apply(&fake, &own(&key)).await;
    assert_eq!(kinds(&reply), ["check-outputs", "stop-job", "undo-plan"]);
    assert_eq!(states(&reply), ["failed", "done", "blocked"], "{reply}");
    assert_eq!(reply["steps"][2]["reason"], "output-not-kept");
    assert!(
        reply["steps"][0]["error"]
            .as_str()
            .unwrap()
            .contains("receipt")
    );
    assert!(fake.logged().contains(&format!("stop job {job}")));
    assert!(
        !fake.logged().contains(&format!("undo {plan}")),
        "nothing that may be the only copy's source is rolled back"
    );
}

/// Note in `task`'s record a collection made at `at` milliseconds.
fn collected_at(place: &Place, task: &Task, at: u64) -> crate::collect::ReceiptId {
    let receipt = crate::collect::ReceiptId::parse(&format!("{at}-a1b2c3")).unwrap();
    Tasks::at(&place.output)
        .update(task, 1, |record| {
            record.noted_output(crate::task::record::OutputEntry {
                receipt: receipt.clone(),
                at,
                files: 1,
                bytes: 3,
                later: serde_json::Map::new(),
            });
            Ok(())
        })
        .unwrap();
    receipt
}

/// One version of the probe's settings file, read from `started` to
/// `finished` and kept with `sha256`.
fn version(sha256: &str, read: Option<(u64, u64)>) -> crate::collect::Kept {
    crate::collect::Kept {
        sha256: sha256.into(),
        read: read.map(|(started, finished)| crate::collect::ReadSpan { started, finished }),
        ..copy_of(
            "/Users/atlas/.config/probe/settings.toml",
            ".config/probe/settings.toml",
        )
    }
}

/// Clean up a task that applied a plan writing the probe's settings and
/// made two collections of them: `first` at 1000 and `second` at 2000,
/// each copy `kept` or lost. Whether the plan was undone, and the reply.
async fn two_versions(
    first: crate::collect::Kept,
    first_kept: bool,
    second: crate::collect::Kept,
    second_kept: bool,
) -> (bool, Value) {
    let place = Place::new();
    let key = session("versions");
    let task = default_task(&key);
    let plan = plan_claimed(&place, &task);
    let fake = Fake::at(&place.root);
    for (at, file, kept) in [(1_000, first, first_kept), (2_000, second, second_kept)] {
        let receipt = collected_at(&place, &task, at);
        let intact = if kept {
            crate::collect::Intact {
                kept: vec![file],
                ..Default::default()
            }
        } else {
            crate::collect::Intact {
                missing: vec![file],
                ..Default::default()
            }
        };
        fake.intact.borrow_mut().insert(receipt.to_string(), intact);
    }
    let reply = place.apply(&fake, &own(&key)).await;
    (fake.logged().contains(&format!("undo {plan}")), reply)
}

#[tokio::test]
async fn the_version_read_after_every_other_is_the_one_that_counts() {
    // Read one after the other: the later bytes count.
    let (undone, reply) = two_versions(
        version("aa", Some((1_000, 1_100))),
        true,
        version("bb", Some((2_000, 2_100))),
        false,
    )
    .await;
    assert!(
        !undone,
        "an older version never stands in for the newest: {reply}"
    );
    assert_eq!(states(&reply), ["failed", "blocked"]);

    // The older copy lost, the newest kept: the file is kept.
    let (undone, reply) = two_versions(
        version("aa", Some((1_000, 1_100))),
        false,
        version("bb", Some((2_000, 2_100))),
        true,
    )
    .await;
    assert!(undone, "{reply}");
    assert_eq!(reply["ok"], true);
}

#[tokio::test]
async fn the_same_bytes_kept_in_any_collection_keep_the_file() {
    let (undone, reply) = two_versions(
        version("aa", Some((1_000, 1_100))),
        true,
        version("aa", Some((2_000, 2_100))),
        false,
    )
    .await;
    assert!(undone, "{reply}");
    assert_eq!(reply["ok"], true);
}

#[tokio::test]
async fn versions_read_while_each_other_was_read_are_all_kept() {
    // The first collection began earlier, but read the file while the
    // second did: either may hold its latest state.
    for (lost_first, lost_second) in [(true, false), (false, true)] {
        let (undone, reply) = two_versions(
            version("aa", Some((1_000, 2_500))),
            !lost_first,
            version("bb", Some((2_000, 2_100))),
            !lost_second,
        )
        .await;
        assert!(!undone, "{reply}");
        assert_eq!(states(&reply), ["failed", "blocked"], "{reply}");
    }
    let (undone, reply) = two_versions(
        version("aa", Some((1_000, 2_500))),
        true,
        version("bb", Some((2_000, 2_100))),
        true,
    )
    .await;
    assert!(undone, "both versions are kept: {reply}");
}

#[tokio::test]
async fn a_clock_that_moved_backwards_orders_nothing() {
    // Read "later" by a clock that went back: neither version is known to
    // be the newest, so losing either keeps the file.
    for second in [
        version("bb", Some((2_100, 2_000))),
        version("bb", Some((500, 600))),
        version("bb", None),
    ] {
        let (undone, reply) =
            two_versions(version("aa", Some((1_000, 1_100))), false, second, true).await;
        assert!(!undone, "{reply}");
        assert_eq!(states(&reply), ["failed", "blocked"], "{reply}");
    }
}

#[tokio::test]
async fn a_job_only_the_helpers_record_knows_is_stopped_too() {
    let place = Place::new();
    let key = session("unnoted");
    let task = default_task(&key);
    place.claim(&task, Owned::Plan { id: Token::new() });
    let fake = Fake::at(&place.root);
    let id = crate::daemon::protocol::JobId::new();
    fake.daemon_jobs.borrow_mut().push(plan::Job {
        id: id.clone(),
        task: task.key().clone(),
        name: "worker".into(),
    });
    let reply = place.apply(&fake, &own(&key)).await;
    assert_eq!(kinds(&reply), ["stop-job", "undo-plan"], "{reply}");
    assert_eq!(reply["steps"][0]["name"], "worker");
    assert!(fake.logged().contains(&format!("stop job {id}")));
}

#[tokio::test]
async fn a_source_named_in_another_case_is_kept_all_the_same() {
    let place = Place::new();
    let key = session("case");
    let task = default_task(&key);
    let plan = plan_claimed(&place, &task);
    let receipt = collected(&place, &task);
    let fake = Fake::at(&place.root);
    fake.intact.borrow_mut().insert(
        receipt.to_string(),
        crate::collect::Intact {
            missing: vec![copy_of(
                "/Users/atlas/.Config/Probe/settings.toml",
                ".Config/Probe/settings.toml",
            )],
            ..Default::default()
        },
    );
    let reply = place.apply(&fake, &own(&key)).await;
    assert_eq!(reply["steps"][1]["state"], "blocked", "{reply}");
    assert!(!fake.logged().contains(&format!("undo {plan}")));
}

#[tokio::test]
async fn a_job_list_that_cannot_be_read_holds_every_removal_back_until_it_can() {
    let place = Place::new();
    let key = session("unlisted");
    let task = default_task(&key);
    let plan = plan_claimed(&place, &task);
    let known = job_started(&place, &task, "web");
    let fake = Fake::at(&place.root);
    fake.unlisted.set(true);
    let unknown = crate::daemon::protocol::JobId::new();
    fake.daemon_jobs.borrow_mut().push(plan::Job {
        id: unknown.clone(),
        task: task.key().clone(),
        name: "worker".into(),
    });

    let reply = place.apply(&fake, &own(&key)).await;
    assert_eq!(reply["ok"], false, "{reply}");
    assert_eq!(kinds(&reply), ["find-jobs", "stop-job", "undo-plan"]);
    assert_eq!(states(&reply), ["failed", "done", "failed"]);
    assert_eq!(reply["steps"][0]["reason"], "daemon");
    assert!(
        reply["steps"][0]["error"]
            .as_str()
            .unwrap()
            .contains("could not be read"),
        "{reply}"
    );
    assert_eq!(reply["steps"][2]["reason"], "needs-step");
    assert!(
        fake.logged().contains(&format!("stop job {known}")),
        "a known job still stops"
    );
    assert!(
        !fake.logged().contains(&format!("undo {plan}")),
        "nothing was removed"
    );

    // Readable again: the job it alone knows is stopped, then the rest runs.
    fake.unlisted.set(false);
    let reply = place.apply(&fake, &own(&key)).await;
    assert_eq!(reply["ok"], true, "{reply}");
    assert!(fake.logged().contains(&format!("stop job {unknown}")));
    assert!(fake.logged().contains(&format!("undo {plan}")));
}

#[tokio::test]
async fn while_an_outcome_is_unknown_only_jobs_stop_and_copies_are_checked() {
    let place = Place::new();
    let key = session("unknown-outcome");
    let task = default_task(&key);
    let copy = place.copy(Some(&key));
    place.claim(&task, Owned::Copy { id: copy.clone() });
    let plan = plan_claimed(&place, &task);
    let job = job_started(&place, &task, "web");
    let receipt = collected(&place, &task);
    let fake = Fake::at(&place.root);
    fake.unknown.set(true);

    let reply = place.apply(&fake, &own(&key)).await;
    assert_eq!(reply["ok"], false, "{reply}");
    assert_eq!(reply["state"], "incomplete");
    assert_eq!(reply["uncertain"], false);
    assert_eq!(
        kinds(&reply),
        [
            "check-outputs",
            "quit",
            "stop-job",
            "undo-plan",
            "remove-copy"
        ]
    );
    assert_eq!(
        states(&reply),
        ["done", "blocked", "done", "blocked", "blocked"]
    );
    for n in [1, 3, 4] {
        assert_eq!(reply["steps"][n]["reason"], "uncertain", "{reply}");
    }
    assert!(
        reply["error"]
            .as_str()
            .unwrap()
            .contains("outcome is unknown")
    );
    let log = fake.logged();
    assert!(log.contains(&format!("check {receipt}")));
    assert!(log.contains(&format!("stop job {job}")));
    assert!(!log.contains(&format!("undo {plan}")), "nothing is undone");
    assert!(place.folder(&copy).exists(), "nothing is removed");

    // Recovered, the rest runs.
    fake.unknown.set(false);
    let reply = place.apply(&fake, &own(&key)).await;
    assert_eq!(reply["ok"], true, "{reply}");
    assert!(fake.logged().contains(&format!("undo {plan}")));
}

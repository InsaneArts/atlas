use super::*;
use crate::{
    app_selection::Process,
    environment::{PlanState, Undone},
    host_session::{Host, HostSession},
    task::{Claims, store::Store as Tasks},
};
use run::Hands;
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
    /// A plan whose undo never answers, as a run killed while it waits.
    hangs: Option<Token>,
    /// Whether quitting runs out of time.
    slow: bool,
    /// Whether the person stops agents once a plan is undone.
    stop_after_undo: bool,
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
                state: "applied",
                targets: vec![".config/probe/settings.toml".into()],
            })
            .collect())
    }

    async fn quit(&self, processes: &[Process], _: &[String]) -> Result<()> {
        let pids: Vec<String> = processes
            .iter()
            .map(|process| process.pid.to_string())
            .collect();
        self.log
            .borrow_mut()
            .push(format!("quit {}", pids.join(" ")));
        if self.slow {
            return Err(Error::new(
                "wait-failed",
                "Probe did not quit within 10 seconds",
            ));
        }
        self.apps
            .borrow_mut()
            .retain(|(pid, ..)| !processes.iter().any(|process| process.pid == *pid));
        Ok(())
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

    fn stopped(&self) -> bool {
        self.stopped.get()
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
async fn steps_run_quit_then_plans_newest_first_then_copies() {
    let place = Place::new();
    let key = session("order");
    let task = default_task(&key);
    let copy = place.copy(Some(&key));
    place.claim(&task, Owned::Copy { id: copy.clone() });
    let older = plan_claimed(&place, &task);
    let newer = plan_claimed(&place, &task);
    let fake = Fake::at(&place.root);
    let executable = place
        .folder(&copy)
        .join("TextEdit.app/Contents/MacOS/TextEdit");
    fake.running(812, "TextEdit", &executable.to_string_lossy());

    let reply = place.apply(&fake, &own(&key)).await;

    assert_eq!(
        kinds(&reply),
        ["quit", "undo-plan", "undo-plan", "remove-copy"]
    );
    assert_eq!(reply["steps"][1]["planId"], newer.as_str());
    assert_eq!(reply["steps"][2]["planId"], older.as_str());
    assert_eq!(
        fake.logged(),
        [
            "quit 812".to_owned(),
            format!("undo {newer}"),
            format!("undo {older}")
        ]
    );
    assert_eq!(reply["state"], "complete");
    assert_eq!(reply["ok"], true);
    assert!(!place.folder(&copy).exists());
    let record = Tasks::at(&place.output).load(task.key()).unwrap();
    assert!(record.claims().all(|(_, state)| state == State::Gone));
    assert!(!record.holds_anything());
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
    let mut receipt = Receipt::new(key.clone(), None, "agent", None);
    merge(&mut receipt.steps, &loaded.undoable());
    let removal = receipt.steps.len() - 1;
    receipt.steps[removal].set(StepState::Started, None);
    for step in &mut receipt.steps[..removal] {
        step.set(StepState::Done, None);
    }
    Receipts::at(&place.output).save(&mut receipt).unwrap();
    note(&place.output, &loaded.covered(&scope), &receipt, None).unwrap();
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

    let error = may_adopt(Some(&record)).unwrap_err();
    assert_eq!(error.code, "input");
    assert!(error.error.contains("may still be working"), "{error}");

    let mut ended = record.clone();
    ended.ended = Some(ended.last_seen);
    assert!(may_adopt(Some(&ended)).is_ok());
    assert!(may_adopt(None).is_ok());

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
        Some(Some(new.clone())),
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
async fn a_receipt_names_no_files() {
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
    for named in ["Secret", "rtf", "settings.toml", "/", "copies"] {
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

use super::*;
use crate::vm::Vms;
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    sync::Mutex,
};

/// Where a stand-in run is killed, as `kill` would: at a numbered call to
/// its means, or at the first call of one by name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kill {
    Never,
    At(usize),
    Named(&'static str),
}

/// The Keychain's enrolments, as a test keeps them.
#[derive(Default)]
struct Keychain {
    kept: Mutex<BTreeMap<String, (Token, [u8; 32])>>,
}

impl Enrolments for Keychain {
    fn keep(&self, vm: &DesktopId, enrolling: &Enrolling) -> Result<()> {
        self.kept.lock().unwrap().insert(
            vm.as_str().to_owned(),
            (enrolling.change.clone(), *enrolling.secret.as_bytes()),
        );
        Ok(())
    }

    fn kept(&self, vm: &DesktopId) -> Result<Option<Enrolling>> {
        Ok(self
            .kept
            .lock()
            .unwrap()
            .get(vm.as_str())
            .map(|(change, secret)| Enrolling {
                change: change.clone(),
                secret: PairingSecret::from_bytes(*secret),
            }))
    }

    fn forget(&self, vm: &DesktopId) -> Result<bool> {
        Ok(self.kept.lock().unwrap().remove(vm.as_str()).is_some())
    }
}

/// Atlas VM, launchd, the keeper and the guest, as one VM's add meets them.
struct Vm {
    agent: bool,
    keeper: bool,
    power: Power,
    /// Every change Atlas VM was asked, by its ID: its name and what it
    /// came to, none while pending.
    ledger: BTreeMap<String, (String, Option<Outcome>)>,
    progress: Option<Progress>,
    pairing: Pairing,
    remote_login: &'static str,
    admin: bool,
    /// Whether going on with the install closes the guest again.
    recloses: bool,
    refuses_install: bool,
    refuses_create: bool,
    made: u32,
    resumed: u32,
    /// Each install asked, by the secret it carried.
    secrets: Vec<String>,
    secret_after_pairing: bool,
    wrong_secret: bool,
    keeper_saw_listed: bool,
    /// What the station did that the guest's closing turns on, in order.
    events: Vec<&'static str>,
}

impl Vm {
    fn new() -> Self {
        Self {
            agent: false,
            keeper: false,
            power: Power::Absent,
            ledger: BTreeMap::new(),
            progress: None,
            pairing: Pairing::Unpaired,
            remote_login: "off",
            admin: false,
            recloses: true,
            refuses_install: false,
            refuses_create: false,
            made: 0,
            resumed: 0,
            secrets: Vec::new(),
            secret_after_pairing: false,
            wrong_secret: false,
            keeper_saw_listed: false,
            events: Vec::new(),
        }
    }

    /// The change `change`, named `op`: what it came to before, or what
    /// `make` makes of it now.
    fn once(
        &mut self,
        change: &Token,
        op: &str,
        make: impl FnOnce(&mut Self) -> Option<Outcome>,
    ) -> Option<Outcome> {
        if let Some((_, Some(outcome))) = self.ledger.get(change.as_str()) {
            return Some(outcome.clone());
        }
        let outcome = make(self);
        self.ledger
            .insert(change.as_str().to_owned(), (op.to_owned(), outcome.clone()));
        outcome
    }

    fn apply(&mut self, operation: &Operation, keychain: &Keychain, id: &DesktopId) -> Reply {
        let outcome = match operation {
            Operation::Create(change) => self.once(change, "create", |vm| {
                if vm.power != Power::Absent {
                    return Some(Outcome::Refused {
                        reason: "The VM is made already.".to_owned(),
                    });
                }
                if vm.refuses_create {
                    return Some(Outcome::Refused {
                        reason: "This Mac needs 70 GB free to make a VM.".to_owned(),
                    });
                }
                vm.power = Power::Stopped;
                vm.made += 1;
                Some(Outcome::Done)
            }),
            Operation::Start(change) => self.once(change, "start", |vm| {
                vm.power = Power::Running;
                Some(Outcome::Done)
            }),
            Operation::Install { change, install } => {
                if self.pairing == Pairing::Paired {
                    self.secret_after_pairing = true;
                }
                let kept = keychain.kept(id).unwrap();
                if kept.is_none_or(|kept| {
                    kept.change != install.enrolment
                        || atlas_core::name::hex(kept.secret.as_bytes()) != install.secret()
                }) {
                    self.wrong_secret = true;
                }
                self.secrets.push(install.secret().to_owned());
                let refuses = self.refuses_install;
                self.once(change, "install", |vm| {
                    if refuses {
                        return Some(Outcome::Refused {
                            reason: "The staged Atlas is not signed as Atlas.".to_owned(),
                        });
                    }
                    vm.installing(change);
                    None
                })
            }
            Operation::InstallResume(install) => {
                self.resumed += 1;
                self.events.push("reclosed");
                if self.recloses {
                    self.admin = false;
                    self.remote_login = "off";
                }
                self.ledger
                    .get(install.as_str())
                    .and_then(|(_, outcome)| outcome.clone())
            }
            Operation::Paired { change, .. } => self.once(change, "paired", |vm| {
                vm.events.push("paired");
                Some(Outcome::Done)
            }),
            Operation::Closed { change, install } => self.once(change, "closed", |vm| {
                vm.events.push("closed");
                if let Some((_, outcome)) = vm.ledger.get_mut(install.as_str()) {
                    *outcome = Some(Outcome::Done);
                }
                Some(Outcome::Done)
            }),
            other => panic!("an add never asks {other:?}"),
        };
        reply(json!({
            "ok": true,
            "op": operation.name(),
            "change": operation.change(),
            "outcome": outcome.map(|outcome| match outcome {
                Outcome::Done => json!({"state": "done"}),
                Outcome::Refused { reason } => json!({"state": "refused", "reason": reason}),
            }),
            "pending": false,
        }))
    }

    /// An install asked again goes one phase further, as far as its close.
    fn installing(&mut self, change: &Token) {
        let progress = self.progress.get_or_insert_with(|| Progress {
            change: change.clone(),
            done: None,
            paired: false,
            failure: None,
            refused: None,
        });
        progress.done = Some(progress.done.map_or(0, |done| (done + 1).min(6)));
    }
}

fn reply(value: Value) -> Reply {
    serde_json::from_value(value).unwrap()
}

/// Another add, made while one waits for the VM's hold.
type Meanwhile = Box<dyn FnOnce(&World)>;

/// A home of its own, its settings file, and everything an add meets.
struct World {
    home: tempfile::TempDir,
    vm: RefCell<Vm>,
    keychain: Keychain,
    kill: Cell<Kill>,
    calls: Cell<usize>,
    killed: Cell<bool>,
    allows: Cell<bool>,
    grants: Cell<u32>,
    foreign_key: Cell<bool>,
    program: RefCell<PathBuf>,
    /// What another add does while this one waits for the VM's hold.
    meanwhile: RefCell<Option<Meanwhile>>,
}

impl World {
    fn new() -> Self {
        Self {
            home: tempfile::tempdir().unwrap(),
            vm: RefCell::new(Vm::new()),
            keychain: Keychain::default(),
            kill: Cell::new(Kill::Never),
            calls: Cell::new(0),
            killed: Cell::new(false),
            allows: Cell::new(true),
            grants: Cell::new(0),
            foreign_key: Cell::new(false),
            program: RefCell::new(PathBuf::from(
                "/Applications/Atlas.app/Contents/MacOS/atlas",
            )),
            meanwhile: RefCell::new(None),
        }
    }

    fn lifecycle(&self) -> Lifecycle {
        Lifecycle::under(self.home.path())
    }

    fn settings(&self) -> PathBuf {
        self.home.path().join("atlas.toml")
    }

    /// Ask for the add of `name`, as the person would, killed as `kill`
    /// says; what it said is kept.
    fn add(&self, name: Option<&str>, kill: Kill) -> (Result<Value>, Vec<String>) {
        self.add_to(name, &self.settings(), kill)
    }

    fn add_to(
        &self,
        name: Option<&str>,
        settings: &Path,
        kill: Kill,
    ) -> (Result<Value>, Vec<String>) {
        self.kill.set(kill);
        self.calls.set(0);
        self.killed.set(false);
        let said = RefCell::new(Vec::new());
        let added = add_with(&self.lifecycle(), self, Asked { name, settings }, &|line| {
            said.borrow_mut().push(line.to_owned())
        });
        (added, said.into_inner())
    }

    /// The only record kept.
    fn record(&self) -> Record {
        let records = self.lifecycle().records().unwrap();
        assert_eq!(records.len(), 1, "{records:?}");
        records.into_iter().next().unwrap()
    }

    fn names(&self) -> Vec<String> {
        let folder = self
            .home
            .path()
            .join("Library/Application Support/Atlas/Station/lifecycle/names");
        let mut names: Vec<String> = std::fs::read_dir(folder)
            .into_iter()
            .flatten()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    /// One call to the means, named `name`: killed here, or after.
    fn point(&self, name: &'static str) -> Result<()> {
        let at = self.calls.get() + 1;
        self.calls.set(at);
        let kill = match self.kill.get() {
            Kill::At(point) => point == at,
            Kill::Named(named) => named == name,
            Kill::Never => false,
        };
        if kill {
            self.killed.set(true);
        }
        if self.killed.get() {
            return Err(Error::new(Code::Interrupted, "Killed."));
        }
        Ok(())
    }
}

impl Means for World {
    fn grant(&self, _operation: &grant::Operation) -> Result<()> {
        self.point("grant")?;
        if !self.allows.get() {
            return Err(Error::new(
                Code::Denied,
                "The person did not allow it in Atlas.app; nothing was done.",
            ));
        }
        self.grants.set(self.grants.get() + 1);
        Ok(())
    }

    fn provenance(&self) -> Result<Provenance> {
        self.point("provenance")?;
        Ok(Provenance {
            program: self.program.borrow().clone(),
            requirement: "identifier \"com.insanearts.atlas\"".to_owned(),
        })
    }

    fn preflight(&self) -> Result<()> {
        self.point("preflight")?;
        if self.foreign_key.get() {
            return Err(Error::new(
                Code::Identity,
                "The station's key was made by Atlas at another path, so no VM is made.",
            ));
        }
        Ok(())
    }

    fn hold<'a>(&self, vm: &DesktopId, admitted: &'a Admitted) -> Result<Option<StationHold<'a>>> {
        self.point("hold")?;
        let meanwhile = self.meanwhile.borrow_mut().take();
        if let Some(other) = meanwhile {
            other(self);
            self.kill.set(Kill::Never);
            self.killed.set(false);
        }
        let vms = Vms::under(self.home.path(), nix::unistd::getuid().as_raw());
        crate::vm::hold_in(&vms, vm, admitted)
    }

    fn agent(&self, _hold: &StationHold<'_>) -> Result<Value> {
        self.point("agent")?;
        self.vm.borrow_mut().agent = true;
        Ok(json!({"ok": true, "agent": "loaded", "pid": 1}))
    }

    fn keeper(&self, hold: &StationHold<'_>, settings: &Path) -> Result<Value> {
        self.point("keeper")?;
        let listed = listed_as(settings_at(settings)?.as_ref(), hold.desktop());
        let mut vm = self.vm.borrow_mut();
        vm.keeper = true;
        vm.keeper_saw_listed = listed;
        Ok(json!({"ok": true, "keeper": "loaded", "pid": 2}))
    }

    fn read(&self, _vm: &DesktopId, operation: &Operation) -> Result<Reply> {
        self.point("read")?;
        let vm = self.vm.borrow();
        if !vm.agent {
            return Err(Error::new(
                Code::Transport,
                "The VM's runner did not answer.",
            ));
        }
        Ok(match operation {
            Operation::Hello => reply(json!({"ok": true, "op": "hello", "hello": {
                "version": 1,
                "capabilities": ["changes", "outcomes", "power", "window-input", "install",
                    "install-resume"],
            }})),
            Operation::State => {
                reply(json!({"ok": true, "op": "state", "power": vm.power, "confirmed": true}))
            }
            other => panic!("an add never reads {other:?}"),
        })
    }

    fn change(
        &self,
        vm: &DesktopId,
        record: &mut Record,
        keep: &dyn Fn(&Record) -> Result<()>,
        operation: &Operation,
    ) -> Result<Reply> {
        self.point("send")?;
        record.sending(operation)?;
        keep(record)?;
        self.point("kept")?;
        let reply = self.vm.borrow_mut().apply(operation, &self.keychain, vm);
        self.point("answered")?;
        Ok(reply)
    }

    fn progress(&self, _vm: &DesktopId) -> Result<Option<Progress>> {
        self.point("progress")?;
        Ok(self.vm.borrow().progress.clone())
    }

    fn bundle(&self) -> Result<PathBuf> {
        Ok(PathBuf::from("/Applications/Atlas.app"))
    }

    fn enrolments(&self) -> &dyn Enrolments {
        &self.keychain
    }

    fn pairing(&self, vm: &DesktopId) -> Result<Pairing> {
        self.point("pairing")?;
        let mut state = self.vm.borrow_mut();
        // The keeper pairs once it runs and the guest's daemon answers,
        // and deletes the enrolment once paired.
        if state.keeper
            && state.pairing == Pairing::Unpaired
            && state.progress.as_ref().is_some_and(Progress::closing)
        {
            state.pairing = Pairing::Paired;
            self.keychain.forget(vm)?;
        }
        Ok(state.pairing)
    }

    fn posture(&self, _selected: &Selected, _vm: &DesktopId) -> Result<Posture> {
        self.point("posture")?;
        let mut vm = self.vm.borrow_mut();
        if vm.pairing != Pairing::Paired {
            return Err(Error::new(Code::Transport, "work is not paired yet"));
        }
        vm.events.push("posture");
        Ok(Posture {
            remote_login: vm.remote_login.to_owned(),
            admin: Some(vm.admin),
        })
    }

    fn wait(&self, _how_long: Duration) {}
}

/// Every step's word is kept, and nothing is sent twice that took effect.
fn added_once(world: &World) {
    let record = world.record();
    assert_eq!(record.state, State::Created);
    for step in [
        Step::Agent,
        Step::Created,
        Step::Started,
        Step::Enrolled,
        Step::Installed,
        Step::Listed,
        Step::Keeper,
        Step::Paired,
        Step::Closed,
    ] {
        assert!(record.has(step), "{step:?} in {:?}", record.steps);
    }
    let vm = world.vm.borrow();
    assert_eq!(vm.made, 1);
    assert!(
        !vm.secret_after_pairing,
        "the secret was sent after the pairing"
    );
    assert!(!vm.wrong_secret, "an install carried another secret");
    assert!(
        vm.keeper_saw_listed,
        "the keeper's settings did not list the VM"
    );
    let reports: Vec<_> = vm
        .events
        .iter()
        .filter(|event| matches!(**event, "paired" | "closed"))
        .collect();
    assert_eq!(reports, [&"paired", &"closed"]);
    let posture = vm.events.iter().position(|event| *event == "posture");
    let closed = vm.events.iter().position(|event| *event == "closed");
    assert!(posture.is_some() && posture < closed, "{:?}", vm.events);
    let settings = Settings::load(&world.settings()).unwrap();
    assert_eq!(settings.desktops().len(), 1);
    assert!(listed_as(Some(&settings), &record.id));
    assert!(world.keychain.kept(&record.id).unwrap().is_none());
}

#[test]
fn an_add_goes_through_every_step_to_created_and_says_each_as_a_plain_line() {
    let world = World::new();

    let (added, said) = world.add(Some("work"), Kill::Never);

    let added = added.unwrap();
    assert_eq!(added["added"]["name"], "work");
    assert_eq!(added["resumed"], false);
    added_once(&world);
    assert!(said.iter().all(|line| !line.contains('\n')), "{said:?}");
    for word in [
        "Asking you in Atlas.app",
        "Making the VM",
        "40 GB",
        "Putting Atlas inside the VM",
        "only Stop reaches it",
        "closed to all but Atlas",
        "is added",
    ] {
        assert!(
            said.iter().any(|line| line.contains(word)),
            "{word} in {said:?}"
        );
    }
    let record = world.record();
    assert!(record.failure.is_none());
    let install = record.last_sent("install").unwrap();
    assert_eq!(
        install.enrolment.as_ref(),
        record.last_sent("paired").unwrap().enrolment.as_ref()
    );
    assert!(!install.request.contains(&world.vm.borrow().secrets[0]));
}

#[test]
fn an_add_killed_at_any_point_goes_on_from_there_when_asked_again_never_sending_the_secret_after_the_pairing()
 {
    let clean = World::new();
    clean.add(Some("work"), Kill::Never).0.unwrap();
    let points = clean.calls.get();
    assert!(points > 20, "{points}");

    for at in 1..=points {
        let world = World::new();
        let (killed, _) = world.add(Some("work"), Kill::At(at));
        assert!(killed.is_err(), "killed at {at}");

        let (again, _) = world.add(Some("work"), Kill::Never);

        let again = again.unwrap_or_else(|error| panic!("killed at {at}: {error:?}"));
        assert_eq!(again["added"]["name"], "work", "killed at {at}");
        added_once(&world);
        assert_eq!(world.names(), ["work"], "killed at {at}");
    }
}

#[test]
fn nothing_is_reserved_recorded_or_made_without_the_persons_grant() {
    let world = World::new();
    world.allows.set(false);

    let (refused, _) = world.add(Some("work"), Kill::Never);

    assert_eq!(refused.unwrap_err().code, Code::Denied);
    assert!(world.names().is_empty());
    assert!(
        !world.lifecycle().kept_any().unwrap() || world.lifecycle().records().unwrap().is_empty()
    );
    assert!(!world.settings().exists());
    assert!(!world.vm.borrow().agent);
}

#[test]
fn an_add_is_refused_while_an_uninstalls_vm_phase_is_under_way_and_goes_on_once_it_commits() {
    let world = World::new();
    let change = atlas_core::files::Token::new();
    // An uninstall cut short in its VM phase.
    drop(world.lifecycle().uninstalling(&change, &[]).unwrap());

    let (refused, _) = world.add(Some("work"), Kill::Never);

    let refused = refused.unwrap_err();
    assert_eq!(refused.code, Code::Busy);
    assert!(refused.error.contains("being uninstalled"), "{refused:?}");
    assert!(world.names().is_empty());
    assert!(world.lifecycle().records().unwrap().is_empty());
    assert!(!world.settings().exists());
    assert!(!world.vm.borrow().agent);

    world
        .lifecycle()
        .resuming(&change)
        .unwrap()
        .commit()
        .unwrap();
    let (added, _) = world.add(Some("work"), Kill::Never);

    assert_eq!(added.unwrap()["added"]["name"], "work");
    added_once(&world);
}

#[test]
fn an_add_is_refused_between_an_uninstalls_vm_phase_commit_and_its_end() {
    let world = World::new();
    let closed = world
        .lifecycle()
        .uninstalling(&atlas_core::files::Token::new(), &[])
        .unwrap()
        .commit()
        .unwrap();

    let (refused, _) = world.add(Some("work"), Kill::Never);

    assert!(
        refused.unwrap_err().error.contains("being uninstalled"),
        "kept out while what Atlas shares goes"
    );
    assert!(world.names().is_empty());
    drop(closed);
    let (added, _) = world.add(Some("work"), Kill::Never);
    assert_eq!(added.unwrap()["added"]["name"], "work");
}

#[test]
fn a_station_key_another_atlas_made_stops_the_add_before_anything_is_made() {
    let world = World::new();
    world.foreign_key.set(true);

    let (refused, _) = world.add(Some("work"), Kill::Never);

    assert_eq!(refused.unwrap_err().code, Code::Identity);
    assert!(world.names().is_empty());
    assert!(world.lifecycle().records().unwrap().is_empty());
    assert!(!world.settings().exists());
    let vm = world.vm.borrow();
    assert!(!vm.agent && vm.ledger.is_empty());
}

#[test]
fn a_name_in_use_is_refused_before_the_person_is_asked() {
    let world = World::new();
    world.add(Some("work"), Kill::Never).0.unwrap();
    Change::AddVm {
        name: "office".to_owned(),
        id: DesktopId::parse("0123456789abcdef01234567").unwrap(),
    }
    .apply(&world.settings())
    .unwrap();
    let grants = world.grants.get();

    let (added, _) = world.add(Some("work"), Kill::Never);
    let (listed, _) = world.add(Some("office"), Kill::Never);

    assert!(added.unwrap_err().error.contains("already a VM"));
    assert!(listed.unwrap_err().error.contains("already list"));
    assert_eq!(world.grants.get(), grants);
}

#[test]
fn a_guest_not_closed_is_closed_again_without_its_secret_before_atlas_vm_is_told_it_is() {
    let world = World::new();
    world.vm.borrow_mut().admin = true;

    world.add(Some("work"), Kill::Never).0.unwrap();

    added_once(&world);
    let vm = world.vm.borrow();
    assert_eq!(vm.resumed, 1);
    assert_eq!(
        vm.events,
        ["posture", "reclosed", "posture", "paired", "closed"]
    );
}

#[test]
fn an_add_killed_after_the_pairing_goes_on_without_the_secret_and_closes_an_open_guest_again() {
    let world = World::new();
    world.vm.borrow_mut().remote_login = "on";
    world
        .add(Some("work"), Kill::Named("posture"))
        .0
        .unwrap_err();
    assert_eq!(world.vm.borrow().pairing, Pairing::Paired);
    let installs = world.vm.borrow().secrets.len();

    world.add(Some("work"), Kill::Never).0.unwrap();

    added_once(&world);
    let vm = world.vm.borrow();
    assert_eq!(vm.secrets.len(), installs, "an install was asked again");
    assert_eq!(vm.resumed, 1);
}

#[test]
fn a_guest_that_stays_open_stops_the_add_before_atlas_vm_hears_it_paired_or_closed() {
    let world = World::new();
    {
        let mut vm = world.vm.borrow_mut();
        vm.admin = true;
        vm.recloses = false;
    }

    let (stopped, _) = world.add(Some("work"), Kill::Never);

    let stopped = stopped.unwrap_err();
    assert!(stopped.error.contains("not closed"), "{}", stopped.error);
    let record = world.record();
    assert_eq!(record.state, State::Adding);
    assert!(!record.has(Step::Paired) && !record.has(Step::Closed));
    assert!(record.failure.unwrap().error.contains("not closed"));
    let vm = world.vm.borrow();
    assert_eq!(vm.resumed, RECLOSING);
    assert!(
        !vm.events
            .iter()
            .any(|event| matches!(*event, "paired" | "closed"))
    );
}

#[test]
fn an_install_atlas_vm_refuses_is_kept_with_its_reason_and_said_again_when_asked_again() {
    let world = World::new();
    world.vm.borrow_mut().refuses_install = true;

    let (refused, _) = world.add(Some("work"), Kill::Never);
    let (again, _) = world.add(Some("work"), Kill::Never);

    for refused in [refused.unwrap_err(), again.unwrap_err()] {
        assert!(
            refused.error.contains("not signed as Atlas"),
            "{}",
            refused.error
        );
    }
    let record = world.record();
    assert_eq!(record.state, State::Adding);
    assert!(
        record
            .failure
            .unwrap()
            .error
            .contains("not signed as Atlas")
    );
    let installs: Vec<_> = record
        .operations
        .iter()
        .filter(|sent| sent.op == "install")
        .collect();
    assert_eq!(installs.len(), 1, "one install, asked again as itself");
    assert!(!world.settings().exists(), "never listed");
}

#[test]
fn an_unnamed_add_asked_again_resumes_the_one_unnamed_add_still_adding_never_a_named_one() {
    let world = World::new();
    world.add(Some("work"), Kill::Named("agent")).0.unwrap_err();
    world.add(None, Kill::Named("agent")).0.unwrap_err();
    let unnamed = world
        .lifecycle()
        .records()
        .unwrap()
        .into_iter()
        .find(|record| !record.named)
        .unwrap();

    let (again, said) = world.add(None, Kill::Named("agent"));

    again.unwrap_err();
    assert!(
        said.iter()
            .any(|line| line.contains("Going on adding the VM \u{201c}vm\u{201d}"))
    );
    assert_eq!(unnamed.name, "vm");
    assert_eq!(world.names(), ["vm", "work"]);
    assert_eq!(world.lifecycle().records().unwrap().len(), 2);
}

#[test]
fn an_add_begun_for_other_settings_or_by_another_atlas_is_refused() {
    let world = World::new();
    world.add(Some("work"), Kill::Named("agent")).0.unwrap_err();
    let other = world.home.path().join("other.toml");

    let (elsewhere, _) = world.add_to(Some("work"), &other, Kill::Never);
    *world.program.borrow_mut() = PathBuf::from("/Users/someone/Atlas.app/Contents/MacOS/atlas");
    let (another, _) = world.add(Some("work"), Kill::Never);

    assert!(elsewhere.unwrap_err().error.contains("--config"));
    assert_eq!(another.unwrap_err().code, Code::Identity);
    assert!(!world.vm.borrow().agent);
}

#[test]
fn a_guest_said_closed_needs_remote_login_off_and_its_account_no_administrator() {
    let posture = |remote: &str, admin: Value| {
        Posture::of(&json!({"remoteLogin": remote, "account": {"admin": admin}}))
    };

    assert_eq!(posture("off", json!(false)).open(), None);
    assert!(posture("on", json!(false)).open().is_some());
    assert!(posture("off", json!(true)).open().is_some());
    assert!(posture("off", Value::Null).open().is_some());
    assert!(posture("unknown", json!(false)).open().is_some());
}

#[test]
fn an_add_that_waited_for_the_hold_goes_on_from_what_another_add_kept_meanwhile() {
    let world = World::new();
    world.add(Some("work"), Kill::Named("agent")).0.unwrap_err();
    *world.meanwhile.borrow_mut() = Some(Box::new(|world: &World| {
        let (other, _) = world.add(Some("work"), Kill::Named("keeper"));
        other.unwrap_err();
    }));

    let (waited, _) = world.add(Some("work"), Kill::Never);

    waited.unwrap();
    added_once(&world);
    let record = world.record();
    let creates = record
        .operations
        .iter()
        .filter(|sent| sent.op == "create")
        .count();
    assert_eq!(creates, 1, "what the other add kept was not written over");
}

#[test]
fn a_make_refused_for_good_is_asked_anew_once_its_cause_is_gone() {
    let world = World::new();
    world.vm.borrow_mut().refuses_create = true;

    let (refused, _) = world.add(Some("work"), Kill::Never);
    let (still, _) = world.add(Some("work"), Kill::Never);
    world.vm.borrow_mut().refuses_create = false;
    let (again, _) = world.add(Some("work"), Kill::Never);

    for refused in [refused.unwrap_err(), still.unwrap_err()] {
        assert!(refused.error.contains("70 GB"), "{}", refused.error);
    }
    again.unwrap();
    added_once(&world);
    let creates = world
        .record()
        .operations
        .iter()
        .filter(|sent| sent.op == "create")
        .count();
    assert_eq!(
        creates, 3,
        "each make refused for good was followed by a new one"
    );
}

#[test]
fn a_name_in_use_says_what_to_do_instead() {
    let world = World::new();
    world.add(Some("work"), Kill::Never).0.unwrap();

    let refused = world.add(Some("work"), Kill::Never).0.unwrap_err();

    assert_eq!(
        json!(refused)["next"],
        "Use another name, or remove work first."
    );
}

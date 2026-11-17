//! Adding a VM desktop, `atlas desktop add vm`: the person's grant through
//! Atlas.app first, then the VM's name reserved and its lifecycle record
//! written, and each step made under the VM's station-operation lock, every
//! change to Atlas VM kept in the record before it is sent. Cut short
//! anywhere, the add asked again goes on from what is there, judged by
//! launchd's jobs, Atlas VM's answers, its install's progress, the pairing
//! record and the settings, never by the record's steps alone; once the
//! guest paired, its secret is never asked for again.
mod bootstrap;
mod real;

use bootstrap::Progress;

use super::{
    StationHold,
    lifecycle::{
        Lifecycle, Provenance,
        barrier::Admitted,
        grant,
        record::{Record, State, Step},
    },
    runner::{Hello, Install, Operation, Outcome, Power, Reply},
};
use crate::station::enrolment::{Enrolling, Enrolments};
use atlas_core::{
    config::{
        desktops::{Chosen, Selected, Settings},
        write::Change,
    },
    desktop::DesktopId,
    error::{Code, Error, Result},
    files::Token,
};
use atlas_session::keys::PairingSecret;
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

/// How often, and how many times, something is looked at before the add
/// stops and says to ask again.
#[derive(Clone, Copy, Debug)]
struct Patience {
    every: Duration,
    tries: u32,
}

impl Patience {
    const fn of(seconds: u64, tries: u32) -> Self {
        Self {
            every: Duration::from_secs(seconds),
            tries,
        }
    }
}

/// launchd loading the runner's or the keeper's job.
const LOADING: Patience = Patience::of(2, 30);
/// Atlas VM answering on its socket once its job is loaded.
const ANSWERING: Patience = Patience::of(2, 90);
/// Atlas VM telling the VM's power.
const SEEING: Patience = Patience::of(2, 60);
/// Making the VM, macOS downloaded and installed into it.
const MAKING: Patience = Patience::of(10, 1080);
/// Starting the VM, and seeing it run.
const STARTING: Patience = Patience::of(5, 120);
/// Putting Atlas inside the guest, as far as its close.
const INSTALLING: Patience = Patience::of(5, 720);
/// The keeper pairing the guest.
const PAIRING: Patience = Patience::of(2, 300);
/// The guest's own word, read through the keeper.
const READING: Patience = Patience::of(2, 60);
/// Atlas VM taking the station's word.
const REPORTING: Patience = Patience::of(2, 60);
/// Atlas VM refusing a request for now, busy with another or recovering.
const BUSY: u32 = 24;
/// How many times the guest is closed again before the add gives up.
const RECLOSING: u32 = 3;

/// What the add asks of the system, given apart so tests stand in.
trait Means {
    /// Ask the person to allow `operation` through Atlas.app, and use the
    /// grant they gave.
    fn grant(&self, operation: &grant::Operation) -> Result<()>;
    /// This Atlas, as a VM's Keychain items are made by it.
    fn provenance(&self) -> Result<Provenance>;
    /// Check the station's key is one the keeper can read.
    fn preflight(&self) -> Result<()>;
    /// The VM `vm`'s station-operation lock, past the barrier `admitted`;
    /// none while another holds it.
    fn hold<'a>(&self, vm: &DesktopId, admitted: &'a Admitted) -> Result<Option<StationHold<'a>>>;
    /// Write and start the VM's runner's agent: what it came to.
    fn agent(&self, hold: &StationHold<'_>) -> Result<Value>;
    /// Write and start the VM's keeper's agent, reading `settings`.
    fn keeper(&self, hold: &StationHold<'_>, settings: &Path) -> Result<Value>;
    /// Ask Atlas VM the read `operation` of `vm`.
    fn read(&self, vm: &DesktopId, operation: &Operation) -> Result<Reply>;
    /// Ask Atlas VM the change `operation` of `vm`, once it is kept in
    /// `record` through `keep`.
    fn change(
        &self,
        vm: &DesktopId,
        record: &mut Record,
        keep: &dyn Fn(&Record) -> Result<()>,
        operation: &Operation,
    ) -> Result<Reply>;
    /// How far Atlas VM's install in `vm`'s guest got; none before it began.
    fn progress(&self, vm: &DesktopId) -> Result<Option<Progress>>;
    /// The Atlas.app this Atlas runs from, which the guest is given.
    fn bundle(&self) -> Result<PathBuf>;
    /// Where enrolments are kept until their pairing.
    fn enrolments(&self) -> &dyn Enrolments;
    /// How `vm`'s pairing stands, as its record says.
    fn pairing(&self, vm: &DesktopId) -> Result<Pairing>;
    /// What `vm`'s guest says of itself over its session, through its
    /// keeper, as `selected` names it.
    fn posture(&self, selected: &Selected, vm: &DesktopId) -> Result<Posture>;
    /// Let `how_long` go by.
    fn wait(&self, how_long: Duration);
}

/// How a VM's pairing stands, as the station's record of it says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Pairing {
    /// Not yet, or cut short before the guest acknowledged it.
    Unpaired,
    Paired,
    /// Revoked: its keys are refused from now on.
    Revoked,
}

/// Whether a guest is closed to all but Atlas, as its own hello says.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Posture {
    /// `on`, `off` or `unknown`.
    remote_login: String,
    /// Whether its account is an administrator; none when it cannot tell.
    admin: Option<bool>,
}

impl Posture {
    /// As the guest's `hello` says it.
    fn of(hello: &Value) -> Self {
        Self {
            remote_login: hello["remoteLogin"]
                .as_str()
                .unwrap_or("unknown")
                .to_owned(),
            admin: hello["account"]["admin"].as_bool(),
        }
    }

    /// Why the guest is not closed; none once Remote Login is off and its
    /// account no administrator.
    fn open(&self) -> Option<&'static str> {
        match (self.remote_login.as_str(), self.admin) {
            ("off", Some(false)) => None,
            ("on", _) => Some("Remote Login is still on in the guest"),
            ("off", Some(true)) => Some("the guest's account is still an administrator"),
            _ => Some("the guest cannot tell whether it is closed"),
        }
    }
}

/// What the person asked to add: a VM, named or not, to the settings file
/// `settings`.
#[derive(Clone, Copy, Debug)]
pub struct Asked<'a> {
    pub name: Option<&'a str>,
    pub settings: &'a Path,
}

/// Add the VM `asked` names, or go on with its add, once the person allows
/// it in Atlas.app, saying each step's progress to `say` as a plain line.
///
/// # Errors
///
/// Refuses without the person's grant, a name in use, settings that
/// cannot take the VM, a station key or an add another Atlas made, and an
/// uninstall under way, each before anything is made; fails, the reason
/// kept in the VM's record, when a step cannot be made, and stops, to be
/// asked again, when one takes too long.
pub fn add(asked: Asked<'_>, say: &dyn Fn(&str)) -> Result<Value> {
    add_with(&Lifecycle::own()?, &real::Real, asked, say)
}

fn add_with(
    lifecycle: &Lifecycle,
    means: &impl Means,
    asked: Asked<'_>,
    say: &dyn Fn(&str),
) -> Result<Value> {
    let settings = atlas_core::config::resolve_path(asked.settings)?;
    let current = settings_at(&settings)?;
    let names: Vec<&str> = current
        .iter()
        .flat_map(Settings::desktops)
        .map(|listed| listed.name.as_str())
        .collect();
    let prospect = lifecycle.prospect(asked.name, &names)?;
    let operation = grant::Operation {
        kind: grant::Kind::Add,
        vm: prospect.id.clone(),
        name: prospect.name.clone(),
        settings: settings.clone(),
    };
    say(&format!(
        "Asking you in Atlas.app to allow adding the VM \u{201c}{}\u{201d}.",
        prospect.name
    ));
    means.grant(&operation)?;
    let admitted = lifecycle.admit()?;
    let own = means.provenance()?;
    begun(lifecycle, &prospect.id, &own, &settings)?;
    if !listed_as(current.as_ref(), &prospect.id) {
        Change::AddVm {
            name: prospect.name.clone(),
            id: prospect.id.clone(),
        }
        .check(&settings)?;
    }
    means.preflight()?;
    let hold = means
        .hold(&prospect.id, &admitted)?
        .ok_or_else(|| Error::new(Code::Busy, "Another station operation holds this VM."))?;
    let reservation = lifecycle.claim(&prospect)?;
    // Read again under the hold: another add of this VM may have gone on
    // since, and what it kept is never written over.
    let record = match begun(lifecycle, &prospect.id, &own, &settings)? {
        Some(record) => record,
        None => Record::adding(&reservation, settings, own),
    };
    lifecycle.keep_record(&record)?;
    if reservation.resumed {
        say(&format!(
            "Going on adding the VM \u{201c}{}\u{201d}.",
            record.name
        ));
    }
    let mut adding = Adding {
        lifecycle,
        means,
        hold: &hold,
        record,
        say,
        hello: None,
    };
    match adding.steps() {
        Ok(()) => {
            let record = &adding.record;
            say(&format!("The VM \u{201c}{}\u{201d} is added.", record.name));
            Ok(json!({
                "ok": true,
                "added": {"name": record.name, "id": record.id, "kind": "vm"},
                "resumed": reservation.resumed,
                "settings": atlas_core::config::tilde(&record.settings),
            }))
        }
        Err(error) => {
            adding.record.failed(&error);
            // Its reason is kept when it can be; the failure is said either way.
            let _ = lifecycle.keep_record(&adding.record);
            Err(error)
        }
    }
}

/// The record of the add of `vm` kept before, when there is one: only one
/// this Atlas began, `own`, for the settings file `settings`, and still
/// being added.
fn begun(
    lifecycle: &Lifecycle,
    vm: &DesktopId,
    own: &Provenance,
    settings: &Path,
) -> Result<Option<Record>> {
    let Some(record) = lifecycle.record(vm)? else {
        return Ok(None);
    };
    record.provenance.check(own)?;
    if record.settings != settings {
        return Err(Error::new(
            Code::Config,
            format!(
                "The VM \u{201c}{}\u{201d} is being added to the settings at {}; run it again with \
                 --config {} to go on.",
                record.name,
                record.settings.display(),
                record.settings.display()
            ),
        ));
    }
    if record.state != State::Adding {
        return Err(Error::new(
            Code::Config,
            format!(
                "The VM \u{201c}{}\u{201d} is no longer being added.",
                record.name
            ),
        ));
    }
    Ok(Some(record))
}

/// The settings at `path`; none when there is no file yet.
pub(super) fn settings_at(path: &Path) -> Result<Option<Settings>> {
    match std::fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(Error::writing(path, error)),
        Ok(_) => Settings::load(path).map(Some),
    }
}

/// Whether `settings` list the VM `vm`.
fn listed_as(settings: Option<&Settings>, vm: &DesktopId) -> bool {
    settings.is_some_and(|settings| {
        settings
            .desktops()
            .iter()
            .any(|listed| matches!(&listed.desktop, Chosen::Vm(config) if config.id == *vm))
    })
}

/// An add under way, under the VM's station-operation lock.
struct Adding<'a, M> {
    lifecycle: &'a Lifecycle,
    means: &'a M,
    hold: &'a StationHold<'a>,
    record: Record,
    say: &'a dyn Fn(&str),
    /// What Atlas VM declared, once it answered.
    hello: Option<Hello>,
}

impl<M: Means> Adding<'_, M> {
    fn steps(&mut self) -> Result<()> {
        self.agent()?;
        self.answering()?;
        self.made()?;
        self.started()?;
        let enrolment = self.enrolled()?;
        let install = self.installed(&enrolment)?;
        self.listed()?;
        self.kept_by_keeper()?;
        self.paired()?;
        if !self.record.has(Step::Closed) {
            self.closed(&install)?;
            self.reported(Step::Paired, |change| Operation::Paired {
                change,
                install: install.clone(),
                enrolment: enrolment.clone(),
            })?;
            self.reported(Step::Closed, |change| Operation::Closed {
                change,
                install: install.clone(),
            })?;
        }
        self.record.state = State::Created;
        self.record.failure = None;
        self.keep()
    }

    fn vm(&self) -> DesktopId {
        self.record.id.clone()
    }

    fn say(&self, line: &str) {
        (self.say)(line);
    }

    fn keep(&self) -> Result<()> {
        self.lifecycle.keep_record(&self.record)
    }

    fn done(&mut self, step: Step) -> Result<()> {
        self.record.done(step);
        self.keep()
    }

    /// What `look` finds, looked at `patience` says; `late` once it found
    /// nothing in that time.
    fn until<T>(
        &self,
        patience: Patience,
        late: &str,
        mut look: impl FnMut() -> Result<Option<T>>,
    ) -> Result<T> {
        for at in 0..patience.tries {
            if at > 0 {
                self.means.wait(patience.every);
            }
            if let Some(found) = look()? {
                return Ok(found);
            }
        }
        Err(Error::new(
            Code::NotReady,
            format!("{late} Run atlas desktop add vm again to go on."),
        ))
    }

    /// Ask Atlas VM the change `operation` once: what it came to, none
    /// while it is pending or the runner did not answer whole. A request
    /// it does not serve now is asked again a while, then refused.
    fn ask(&mut self, operation: &Operation) -> Result<Option<Outcome>> {
        let (vm, lifecycle) = (self.vm(), self.lifecycle);
        let keep = |record: &Record| lifecycle.keep_record(record);
        for at in 0..BUSY {
            if at > 0 {
                self.means.wait(ANSWERING.every);
            }
            match self.means.change(&vm, &mut self.record, &keep, operation) {
                Ok(reply) if reply.ok => return Ok(reply.outcome),
                Ok(reply) if at + 1 == BUSY => {
                    return Err(Error::new(
                        Code::State,
                        format!(
                            "Atlas VM did not take the VM's {}: {}",
                            operation.name(),
                            reply.error.as_deref().unwrap_or("it gave no reason")
                        ),
                    ));
                }
                Ok(_) => {}
                // Busy with it still, or gone and back: it is asked again.
                Err(error) if error.code == Code::Transport => return Ok(None),
                Err(error) => return Err(error),
            }
        }
        Ok(None)
    }

    /// Ask the change `operation` until it settles, as the very same
    /// request each time, `patience` apart.
    fn settle(&mut self, operation: &Operation, patience: Patience) -> Result<Outcome> {
        for at in 0..patience.tries {
            if at > 0 {
                self.means.wait(patience.every);
            }
            if let Some(outcome) = self.ask(operation)? {
                return Ok(outcome);
            }
        }
        Err(Error::new(
            Code::NotReady,
            format!(
                "Atlas VM has not finished the VM's {} yet. Run atlas desktop add vm again to go \
                 on.",
                operation.name()
            ),
        ))
    }

    /// The VM's power as Atlas VM sees it just now.
    fn power(&self) -> Result<Power> {
        let vm = self.vm();
        self.until(SEEING, "Atlas VM cannot tell the VM's power yet.", || {
            Ok(self
                .means
                .read(&vm, &Operation::State)
                .ok()
                .filter(|reply| reply.ok && reply.confirmed == Some(true))
                .and_then(|reply| reply.power))
        })
    }

    /// The runner's agent, judged by what launchd loaded.
    fn agent(&mut self) -> Result<()> {
        self.say("Starting Atlas VM, which runs the VM, under launchd.");
        self.until(LOADING, "launchd has not loaded Atlas VM's job.", || {
            loaded(&self.means.agent(self.hold)?, "agent")
        })?;
        self.done(Step::Agent)
    }

    /// Atlas VM answering on its socket, and what it declares.
    fn answering(&mut self) -> Result<()> {
        let vm = self.vm();
        let hello = self.until(ANSWERING, "Atlas VM is not answering yet.", || {
            Ok(self
                .means
                .read(&vm, &Operation::Hello)
                .ok()
                .filter(|reply| reply.ok)
                .and_then(|reply| reply.hello))
        })?;
        self.hello = Some(hello);
        Ok(())
    }

    /// The VM made, once: the make asked before asked again; a new one for
    /// a VM seen absent, first or once an earlier make settled refused.
    fn made(&mut self) -> Result<()> {
        if self.record.has(Step::Created) {
            return Ok(());
        }
        let mut kept = self
            .record
            .last_sent("create")
            .map(|sent| sent.change.clone());
        if kept.is_none() && self.power()? != Power::Absent {
            return Err(Error::new(
                Code::State,
                "A VM is already made in this VM's folder, though this add never asked for one; \
                 nothing more was done.",
            ));
        }
        self.say(
            "Making the VM: Atlas VM first checks this Mac has room (40 GB free, 70 GB when \
             macOS must be downloaded). It can take a while.",
        );
        loop {
            let earlier = kept.is_some();
            let change = kept.take().unwrap_or_default();
            match self.settle(&Operation::Create(change), MAKING)? {
                Outcome::Done => return self.done(Step::Created),
                // An earlier make refused for good, its cause maybe gone
                // since, and nothing made: a new make is asked.
                Outcome::Refused { .. } if earlier && self.power()? == Power::Absent => {}
                Outcome::Refused { reason } => return Err(refused("making the VM", &reason)),
            }
        }
    }

    /// The VM running: a start asked before asked again, and a new one
    /// when that one is settled and the VM is not running.
    fn started(&mut self) -> Result<()> {
        if self.power()? == Power::Running {
            return self.done(Step::Started);
        }
        self.say("Starting the VM.");
        let mut kept = self
            .record
            .last_sent("start")
            .map(|sent| sent.change.clone());
        let mut fresh = kept.is_none();
        loop {
            let change = kept.take().unwrap_or_default();
            let running = match self.settle(&Operation::Start(change), STARTING)? {
                Outcome::Done => self.running()?,
                Outcome::Refused { reason } if fresh => {
                    return Err(refused("starting the VM", &reason));
                }
                // An earlier start's: a new one is asked.
                Outcome::Refused { .. } => false,
            };
            if running {
                return self.done(Step::Started);
            }
            if fresh {
                return Err(Error::new(
                    Code::State,
                    "The VM did not keep running once started; nothing more was done.",
                ));
            }
            fresh = true;
        }
    }

    /// Whether the VM runs, once it is no longer starting.
    fn running(&self) -> Result<bool> {
        self.until(STARTING, "The VM is not running yet.", || {
            Ok(match self.power()? {
                Power::Running => Some(true),
                Power::Starting => None,
                _ => Some(false),
            })
        })
    }

    /// The enrolment the guest's install is given: the install's, once it
    /// was asked; else the one kept in the Keychain, or a new one kept
    /// there first.
    fn enrolled(&mut self) -> Result<Token> {
        if let Some(sent) = self.record.last_sent("install") {
            return sent.enrolment.clone().ok_or_else(|| {
                Error::new(
                    Code::State,
                    "The VM's record keeps an install with no enrolment.",
                )
            });
        }
        let vm = self.vm();
        let enrolments = self.means.enrolments();
        let enrolment = match enrolments.kept(&vm)? {
            Some(kept) => kept.change,
            None => {
                let enrolling = Enrolling {
                    change: Token::new(),
                    secret: PairingSecret::generate()?,
                };
                enrolments.keep(&vm, &enrolling)?;
                enrolling.change
            }
        };
        self.done(Step::Enrolled)?;
        Ok(enrolment)
    }

    /// Atlas inside the guest as far as its close: the install asked with
    /// the bundle this Atlas runs from, and asked again, the secret read
    /// back from the Keychain, while it is pending. Once the guest paired
    /// the secret is gone, and the install is never asked with it again.
    fn installed(&mut self, enrolment: &Token) -> Result<Token> {
        let vm = self.vm();
        let kept = self
            .record
            .last_sent("install")
            .map(|sent| sent.change.clone());
        if let Some(change) = &kept {
            let progress = self.means.progress(&vm)?;
            if progress.is_some_and(|progress| progress.change == *change && progress.closing()) {
                self.done(Step::Installed)?;
                return Ok(change.clone());
            }
        }
        if self.means.pairing(&vm)? == Pairing::Paired {
            return Err(Error::new(
                Code::State,
                "The VM's guest paired before its install reached its close, and its secret is \
                 gone; remove the VM and add it again.",
            ));
        }
        let secret = self
            .means
            .enrolments()
            .kept(&vm)?
            .filter(|kept| kept.change == *enrolment)
            .ok_or_else(|| {
                Error::new(
                    Code::Identity,
                    "The VM's enrolment is no longer in the Keychain, though its guest has not \
                     paired; remove the VM and add it again.",
                )
            })?;
        let change = kept.unwrap_or_default();
        let operation = Operation::Install {
            change: change.clone(),
            install: Install::new(
                self.means.bundle()?,
                enrolment.clone(),
                atlas_core::name::hex(secret.secret.as_bytes()),
            ),
        };
        self.say("Putting Atlas inside the VM.");
        let mut said = (None, None);
        for at in 0..INSTALLING.tries {
            if at > 0 {
                self.means.wait(INSTALLING.every);
            }
            match self.ask(&operation)? {
                Some(Outcome::Refused { reason }) => {
                    return Err(refused("putting Atlas inside the VM", &reason));
                }
                Some(Outcome::Done) => return self.installed_at(change),
                None => {}
            }
            let Some(progress) = self
                .means
                .progress(&vm)?
                .filter(|progress| progress.change == change)
            else {
                continue;
            };
            if progress.closing() {
                return self.installed_at(change);
            }
            let now = (progress.done, progress.failure.clone());
            if now != said {
                if now.0 != said.0
                    && let Some(phase) = progress.said()
                {
                    self.say(&format!("Atlas in the VM: {phase}."));
                }
                if let Some(failure) = &now.1 {
                    self.say(&format!("Atlas in the VM is waiting: {failure}"));
                }
                said = now;
            }
        }
        Err(Error::new(
            Code::NotReady,
            "Atlas is not yet in the VM. Run atlas desktop add vm again to go on.",
        ))
    }

    fn installed_at(&mut self, change: Token) -> Result<Token> {
        self.done(Step::Installed)?;
        Ok(change)
    }

    /// The VM's provisional entry in the settings, its record still adding:
    /// the keeper resolves its VM from the settings it pins.
    fn listed(&mut self) -> Result<()> {
        if !listed_as(
            settings_at(&self.record.settings)?.as_ref(),
            &self.record.id,
        ) {
            self.say(&format!(
                "Listing \u{201c}{}\u{201d} in {}; until it is added, only Stop reaches it.",
                self.record.name,
                atlas_core::config::tilde(&self.record.settings)
            ));
            Change::AddVm {
                name: self.record.name.clone(),
                id: self.vm(),
            }
            .apply(&self.record.settings)?;
        }
        self.done(Step::Listed)
    }

    /// The VM's keeper, judged by what launchd loaded.
    fn kept_by_keeper(&mut self) -> Result<()> {
        self.say("Starting the VM's session keeper under launchd.");
        let settings = self.record.settings.clone();
        self.until(LOADING, "launchd has not loaded the keeper's job.", || {
            loaded(&self.means.keeper(self.hold, &settings)?, "keeper")
        })?;
        self.done(Step::Keeper)
    }

    /// The guest paired with the station, as its pairing record says.
    fn paired(&mut self) -> Result<()> {
        let vm = self.vm();
        self.say("Waiting for the keeper to pair the VM's guest.");
        self.until(PAIRING, "The VM's guest has not paired yet.", || match self
            .means
            .pairing(&vm)?
        {
            Pairing::Paired => Ok(Some(())),
            Pairing::Revoked => Err(Error::new(
                Code::Identity,
                "The VM's pairing was revoked; remove the VM and add it again.",
            )),
            Pairing::Unpaired => Ok(None),
        })
    }

    /// The guest closed, as it says itself over its session: Remote Login
    /// off and its account no administrator; when it is not, the install
    /// `install` is gone on with, without its secret, and it is looked at
    /// again.
    fn closed(&mut self, install: &Token) -> Result<()> {
        let selected = self.selected()?;
        let vm = self.vm();
        self.say("Checking the VM's guest is closed to all but Atlas.");
        for round in 0..=RECLOSING {
            let mut last = String::new();
            let posture = self.until(
                READING,
                "The VM's guest is not answering yet.",
                || match self.means.posture(&selected, &vm) {
                    Ok(posture) => Ok(Some(posture)),
                    Err(error) => {
                        last = error.error;
                        Ok(None)
                    }
                },
            );
            let posture = posture.map_err(|error| {
                if last.is_empty() {
                    error
                } else {
                    Error::new(
                        error.code,
                        format!("{} ({last})", error.error.trim_end_matches('.')),
                    )
                }
            })?;
            let Some(why) = posture.open() else {
                return Ok(());
            };
            if round == RECLOSING {
                return Err(Error::new(
                    Code::State,
                    format!(
                        "The VM's guest is not closed: {why}. Nothing more was done; remove {} \
                         and add it again.",
                        self.record.name
                    ),
                ));
            }
            self.say(&format!(
                "The VM's guest is not closed yet ({why}); asking Atlas VM to close it again."
            ));
            self.reclose(install)?;
        }
        Ok(())
    }

    /// Ask Atlas VM to go on with the install `install` without its secret.
    fn reclose(&mut self, install: &Token) -> Result<()> {
        if !self
            .hello
            .as_ref()
            .is_some_and(|hello| hello.can("install-resume"))
        {
            return Err(Error::new(
                Code::Update,
                "This Atlas VM cannot close the guest again; update Atlas, then run atlas \
                 desktop add vm again.",
            ));
        }
        match self.ask(&Operation::InstallResume(install.clone()))? {
            Some(Outcome::Refused { reason }) => {
                Err(refused("putting Atlas inside the VM", &reason))
            }
            Some(Outcome::Done) | None => {
                self.means.wait(READING.every);
                Ok(())
            }
        }
    }

    /// Tell Atlas VM what the station saw, as the change `make` gives,
    /// once: kept as `step` once it took it.
    fn reported(&mut self, step: Step, make: impl Fn(Token) -> Operation) -> Result<()> {
        if self.record.has(step) {
            return Ok(());
        }
        let name = make(Token::new()).name();
        let change = self
            .record
            .last_sent(name)
            .map_or_else(Token::new, |sent| sent.change.clone());
        match self.settle(&make(change), REPORTING)? {
            Outcome::Done => self.done(step),
            Outcome::Refused { reason } => Err(Error::new(
                Code::State,
                format!("Atlas VM did not take the station's word ({name}): {reason}"),
            )),
        }
    }

    /// The VM as its settings entry names it, for its keeper.
    fn selected(&self) -> Result<Selected> {
        let settings = Settings::load(&self.record.settings)?;
        let listed = settings
            .desktops()
            .iter()
            .find(|listed| matches!(&listed.desktop, Chosen::Vm(config) if config.id == self.record.id))
            .ok_or_else(|| {
                Error::new(
                    Code::Config,
                    format!(
                        "The settings no longer list \u{201c}{}\u{201d}; run atlas desktop add \
                         vm again to go on.",
                        self.record.name
                    ),
                )
            })?;
        Ok(settings.selected(listed))
    }
}

/// `answer`, an agent's install, as loaded (`Some`), not yet (`None`), or
/// refused; `what` names its field.
fn loaded(answer: &Value, what: &str) -> Result<Option<()>> {
    if answer["ok"] == false {
        return Err(Error::new(
            Code::State,
            answer["error"]
                .as_str()
                .unwrap_or("launchd's job could not be written.")
                .to_owned(),
        ));
    }
    Ok((answer[what] == "loaded").then_some(()))
}

/// Atlas VM's refusal of `what`, with its `reason`, kept and said.
fn refused(what: &str, reason: &str) -> Error {
    Error::new(Code::State, format!("Atlas VM refused {what}: {reason}")).with(
        "next",
        json!("Nothing more was done; the reason is kept. Remove the VM to try again."),
    )
}

#[cfg(test)]
#[path = "add_tests.rs"]
mod tests;

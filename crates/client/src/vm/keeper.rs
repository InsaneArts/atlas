//! A VM's session keeper: the one long-lived owner of the station's session
//! with the VM's guest, so a command's exit never ends it. Started only by
//! its own launchd agent, alone under its lock and never while the VM is
//! being removed, it trusts Atlas VM before each dial, pairs once, holds one
//! generation's control for every Atlas process that asks through it, and,
//! that generation lost or silent, dials again with backoff.
//!
//! It never infers the guest's state from silence: on each new generation it
//! reads the guest's own Stop report, and while the station holds the
//! person's Stop it says Stop until the guest answers it applied. It never
//! resumes by itself.
//!
//! It carries each command's changes on lanes of the generation that
//! command proved ([`change`]), and tells the guest to forget what a change
//! came to only once the station holds no claim on it: at each hello, and
//! every [`WATCH`] for the changes it carried. A change whose command ended
//! without its answer is recovered from what the guest says it came to
//! ([`recover`]). A command's pictures and file reads go on picture lanes
//! of the generation it proved, what the guest sends passed back only as
//! fast as the command takes it ([`picture`]).
//!
//! While the person's maintenance is kept for the VM, it carries no agent's
//! request, and a hold cancels every agent's lane it carries before it is
//! answered ([`held`]).
//!
//! Asked, it brings its guest's Atlas to the very build it runs itself,
//! only while the station's Stop holds agents, and carries that update on
//! in every generation until it is done ([`update`]).
//!
//! An update of Atlas replaces its program while it runs; it does nothing
//! on its own then. The update hands it over ([`super::handover`]): asked,
//! it keeps the station's Stop for the update, says it, leaves its
//! generation and ends, and launchd starts the new build's keeper in its
//! place, which finishes a handover cut short.
mod agent;
mod change;
mod clients;
mod held;
mod lanes;
mod observe;
mod picture;
mod program;
mod recover;
mod stage;
mod update;

pub use agent::{install_keeper, install_keeper_held};
pub(crate) use clients::{CAPABILITIES, PROTOCOL, RESUME_KEY, carries};
pub(crate) use held::served_while_held;
pub(crate) use picture::carries_bulk;
use program::Program;
pub(crate) use stage::deadline as stage_deadline;
pub(crate) use update::unsettled as update_unsettled;

use super::{
    Vms,
    station::{Facts, Refused},
    try_lock,
};
use crate::station::{
    Station,
    enrolment::{Enrolments, vm_of},
    pairing::{Pairing, pair},
};
use atlas_control::{Controller, StopRevision, StopWhy};
use atlas_core::{
    config::desktops::{Chosen, Selection, Settings},
    desktop::DesktopId,
    error::{Code, Error, Result},
    files::LockFile,
    session::key::PeerKey,
};
use atlas_session::{
    compatibility::{Declared, STOP_REPORT, Usable, judge},
    control::Outcome,
    generation::Generation,
    keys::Keypair,
    lane::LaneKind,
    pairing::{Enrolment, StationRecords},
    secure::Purpose,
    station::{
        Desktop, Dial, Dialing, Peers, Raw,
        control::{self, Answer, Control, Event},
    },
};
use serde_json::{Value, json};
use std::{
    future::Future,
    path::{Path, PathBuf},
    pin::Pin,
    sync::{Arc, Mutex, MutexGuard, PoisonError},
    time::Duration,
};
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};

/// The first wait before dialling again, doubled after each attempt that
/// went nowhere.
const FIRST: Duration = Duration::from_secs(1);
/// The longest wait before dialling again.
const MOST: Duration = Duration::from_secs(30);
/// How long a Stop's answer is waited for before it is said again: the
/// guest waits up to ten seconds for work to end before answering.
const ANSWER: Duration = Duration::from_secs(30);
/// How often the station's Stop is read again while a generation lives, and
/// a Stop not confirmed is said again.
const WATCH: Duration = Duration::from_secs(1);
/// How often a leftover enrolment is deleted again until it is gone.
const FORGETTING: Duration = Duration::from_secs(10);
/// How long the guest's own report is waited for on a new generation.
const REPORT: Duration = Duration::from_secs(10);
/// The longest reply line a read or a change carries.
const LINE: u64 = 4 << 20;

/// The label of `vm`'s keeper's own launchd agent.
#[must_use]
pub fn label(vm: &DesktopId) -> String {
    Vms::keeper_label(vm)
}

/// The Resume capability `vm`'s keeper made at its start, which the
/// person's own surface, the viewer service, reads in the station folder to
/// send with the person's Resume. No agent command reads it.
///
/// # Errors
///
/// Fails when no keeper of the VM keeps one, or it is not this account's.
pub fn resume_capability(vm: &DesktopId) -> Result<String> {
    let path = Station::own()?.path_of(vm).join(RESUME_KEY);
    let kept = atlas_core::files::read_own(&path, 256).ok_or_else(|| {
        Error::new(
            Code::Stopped,
            "The VM's session keeper is not running, so agents cannot resume there yet.",
        )
    })?;
    String::from_utf8(kept)
        .map(|kept| kept.trim().to_owned())
        .map_err(|_| Error::new(Code::State, "The VM's Resume capability is damaged."))
}

/// The private folder keepers listen in: in this account's own folder
/// outside its home, whose path is short whatever the home's, so a socket's
/// address, which holds 104 bytes, always fits.
///
/// # Errors
///
/// Fails when the system does not name that folder.
pub(crate) fn sockets() -> Result<PathBuf> {
    atlas_core::own_folder::sockets()
        .ok_or_else(|| Error::new(Code::State, "This account's own folder cannot be found."))
}

/// The socket `vm`'s keeper listens on.
///
/// # Errors
///
/// As [`sockets`].
pub(crate) fn socket_of(vm: &DesktopId) -> Result<PathBuf> {
    Ok(sockets()?.join(format!("{}.keeper", vm.as_str())))
}

/// The VM keyed `key` (`vm-<id>`), which the settings at `settings` list,
/// and the name they list it by.
///
/// # Errors
///
/// Refuses a key that names no VM, or one the settings do not list as a VM
/// of that identifier; fails when the settings cannot be read.
pub fn listed(key: &str, settings: &Path) -> Result<(DesktopId, String)> {
    let vm = vm_of(key)?;
    let settings = Settings::load(settings)?;
    let listed = settings.resolve(&Selection {
        parent: Some(key.to_owned()),
        ..Selection::default()
    })?;
    match &listed.desktop {
        Chosen::Vm(config) if config.id == vm => Ok((vm, listed.name.clone())),
        _ => Err(Error::new(
            Code::ConfigInvalid,
            "The settings list no VM by that key; its keeper does not start.",
        )),
    }
}

/// Serve as the keeper of the VM keyed `key` in the settings at `settings`,
/// until it is ended. The caller proves first that launchd started it as
/// its own job. A start that fails is kept in the VM's station folder, when
/// there is one, for the next status to say why ([`start_failure`]); one
/// refused by another keeper's lock or the VM's removal is not, and a start
/// that succeeds forgets it.
///
/// # Errors
///
/// Refuses anything but a VM the settings list, a second keeper, and a VM
/// being removed; fails when its own signing cannot be read, or its folder,
/// key, capability or socket cannot be made.
pub async fn keep(key: &str, settings: &Path) -> Result<()> {
    // First, before an update may have touched its file.
    let program = Program::own();
    let failures = vm_of(key)
        .ok()
        .and_then(|vm| Some(Station::own().ok()?.path_of(&vm)));
    let remember = |error: Error| {
        if let Some(folder) = &failures {
            remember_failure(folder, &error);
        }
        error
    };
    if !program.signed() {
        return Err(remember(Error::new(
            Code::Identity,
            "The keeper's own signing cannot be read; it does not start.",
        )));
    }
    let (vm, name) = listed(key, settings).map_err(remember)?;
    let vms = Vms::own()?;
    let _held = hold(&vms, &vm)?;
    let (keeper, listener, capability) = started(vm, name, program).await.map_err(remember)?;
    forget_failure(&keeper.folder);
    let serving = clients::serve(
        listener,
        keeper.clone(),
        clients::Grant::new(capability),
        Arc::new(clients::Atlas),
    );
    tokio::select! {
        () = keeper.run() => Ok(()),
        () = serving => Ok(()),
        () = keeper.ended.notified() => Ok(()),
    }
}

/// The keeper of `vm`, listed as `name`, whose program is `program`, made
/// under its lock: its folder, key and Resume capability, a handover cut
/// short finished, and its socket bound.
async fn started(
    vm: DesktopId,
    name: String,
    program: Program,
) -> Result<(Arc<Keeper>, tokio::net::UnixListener, String)> {
    let station = Station::own()?;
    let folder = station.folder(&vm)?;
    let own = {
        let station = station.clone();
        blocking(move || station.key()).await?
    };
    let capability = clients::capability(&folder)?;
    atlas_core::files::private_folder(&sockets()?, Code::State)?;
    let keeper = Arc::new(Keeper {
        records: Arc::new(station.pairing(&vm)?),
        enrolments: Arc::new(crate::station::enrolment::Keychain),
        reach: Arc::new(Socket {
            vm: vm.clone(),
            name: name.clone(),
        }),
        name,
        stop: Controller::new(&atlas_core::desktop::Desktop::from(atlas_core::vm::Vm {
            id: vm.clone(),
        }))?,
        own: Arc::new(own),
        folder,
        program,
        instance: atlas_core::process::Instance::current(),
        vm,
        turn: tokio::sync::Mutex::new(()),
        applied: Mutex::default(),
        forgotten: std::sync::atomic::AtomicBool::new(false),
        handing: std::sync::atomic::AtomicBool::new(false),
        ended: tokio::sync::Notify::new(),
        observing: observe::room(),
        recovering: tokio::sync::Mutex::new(()),
        kept_back: change::KeptBack::default(),
        progress: Mutex::new(picture::PROGRESS),
        shared: Shared::default(),
        lifecycle: super::lifecycle::Lifecycle::own()?,
        lanes: lanes::Lanes::default(),
        owner: tokio::sync::Mutex::default(),
        owner_waking: tokio::sync::Notify::new(),
        update_writes: Arc::default(),
    });
    keeper.finish_handover()?;
    held::at_start(&keeper).await;
    let listener = clients::bind(&socket_of(&keeper.vm)?)?;
    Ok((keeper, listener, capability))
}

/// Where a keeper's last start failure is kept, in its station folder.
const START_FAILURE: &str = "start-failure.json";

/// Keep `error`, why a keeper did not start, in its station `folder`, when
/// the folder is there: it is never made for this, so a VM being removed
/// keeps none.
fn remember_failure(folder: &Path, error: &Error) {
    if std::fs::symlink_metadata(folder).is_ok_and(|folder| folder.is_dir())
        && let Ok(said) = serde_json::to_vec(&json!({"code": error.code, "error": error.error}))
    {
        let _ = atlas_core::files::write_atomic(&folder.join(START_FAILURE), &said, 0o600);
    }
}

fn forget_failure(folder: &Path) {
    let _ = std::fs::remove_file(folder.join(START_FAILURE));
}

/// Why the keeper whose station folder is `folder` last failed to start;
/// none since it last started, or when it never failed.
pub(crate) fn start_failure(folder: &Path) -> Option<String> {
    let kept = atlas_core::files::read_own(&folder.join(START_FAILURE), 4096)?;
    serde_json::from_slice::<Value>(&kept).ok()?["error"]
        .as_str()
        .map(str::to_owned)
}

/// The keeper's lock for `vm`, held for its life, taken only while the VM
/// is not being removed.
pub(super) fn hold(vms: &Vms, vm: &DesktopId) -> Result<LockFile> {
    let Some(held) = try_lock(&vms.keeper_lock(vm))? else {
        return Err(Error::new(
            Code::Busy,
            "Another keeper serves this VM; this one does not start.",
        ));
    };
    // A removal record that cannot be read is never taken for none.
    match std::fs::symlink_metadata(vms.removal(vm)) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(held),
        Ok(_) => Err(Error::new(
            Code::State,
            "This VM is being removed; its keeper does not start.",
        )),
        Err(error) => Err(Error::writing(&vms.removal(vm), error)),
    }
}

/// A new connection to the guest for one purpose, trusted first.
type Reaching<'a> = Pin<Box<dyn Future<Output = std::result::Result<Raw, Refused>> + Send + 'a>>;

/// How the keeper reaches its VM's guest, given apart so tests stand in.
trait Reach: Send + Sync {
    /// A new trusted connection for `purpose`.
    fn reach(&self, purpose: Purpose) -> Reaching<'_>;

    /// The facts trust is judged on now; none when they cannot be told.
    fn facts(&self) -> Pin<Box<dyn Future<Output = Option<Facts>> + Send + '_>>;

    /// The VM's state as its runner, Atlas VM, says it now, the whole
    /// exchange within `within`.
    fn runner(
        &self,
        within: Duration,
    ) -> Pin<Box<dyn Future<Output = Result<super::runner::Reply>> + Send + '_>>;
}

/// The VM's station socket, as Atlas VM serves it, and the VM's name.
struct Socket {
    vm: DesktopId,
    name: String,
}

impl Reach for Socket {
    fn reach(&self, purpose: Purpose) -> Reaching<'_> {
        Box::pin(super::station::dial(&self.vm, &self.name, purpose))
    }

    fn facts(&self) -> Pin<Box<dyn Future<Output = Option<Facts>> + Send + '_>> {
        let vm = self.vm.clone();
        Box::pin(async move {
            tokio::task::spawn_blocking(move || super::station::facts(&vm).ok())
                .await
                .ok()
                .flatten()
        })
    }

    fn runner(
        &self,
        within: Duration,
    ) -> Pin<Box<dyn Future<Output = Result<super::runner::Reply>> + Send + '_>> {
        let vm = self.vm.clone();
        Box::pin(runner_state(move || super::runner::Runner::of(&vm), within))
    }
}

/// One purpose of the keeper's reach, as a session dial, keeping the last
/// refusal for trust so the keeper can judge it apart from a failure.
struct Dialled {
    reach: Arc<dyn Reach>,
    purpose: Purpose,
    name: String,
    untrusted: Mutex<Option<(super::Untrusted, Facts)>>,
}

impl Dialled {
    fn new(keeper: &Keeper, purpose: Purpose) -> Self {
        Self {
            reach: keeper.reach.clone(),
            purpose,
            name: keeper.name.clone(),
            untrusted: Mutex::new(None),
        }
    }

    fn untrusted(&self) -> Option<(super::Untrusted, Facts)> {
        self.untrusted
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
    }
}

impl Dial for Dialled {
    fn dial(&self) -> Dialing<'_> {
        Box::pin(async move {
            self.reach.reach(self.purpose).await.map_err(|refused| {
                if let Refused::Untrusted { why, facts } = &refused {
                    *self
                        .untrusted
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner) = Some((*why, facts.clone()));
                }
                std::io::Error::other(refused.into_error(&self.name))
            })
        })
    }
}

/// The keeper of one VM.
struct Keeper {
    vm: DesktopId,
    /// The VM's name, as the settings list it, for what is said of it.
    name: String,
    own: Arc<Keypair>,
    reach: Arc<dyn Reach>,
    records: Arc<dyn StationRecords>,
    enrolments: Arc<dyn Enrolments>,
    /// The person's Stop as the station keeps it.
    stop: Controller,
    /// Taken to read the station's Stop before saying it, and by a Resume
    /// from saying it until it lifted that Stop, so neither comes between.
    turn: tokio::sync::Mutex<()>,
    /// The station's Stop the live generation's guest applied.
    applied: Mutex<Applied>,
    /// Whether the enrolment is known to be gone from the Keychain.
    forgotten: std::sync::atomic::AtomicBool,
    /// The VM's private station folder.
    folder: PathBuf,
    /// Its own program, as it started.
    program: Program,
    /// This very process, told apart from any later keeper given its PID.
    instance: Option<atlas_core::process::Instance>,
    /// Whether it handed its session over to the next keeper: it dials no
    /// more.
    handing: std::sync::atomic::AtomicBool,
    /// Told once it handed over, so it ends.
    ended: tokio::sync::Notify,
    /// Room for the observations that run at once.
    observing: tokio::sync::Semaphore,
    /// Held through each recovery of the changes no command holds.
    recovering: tokio::sync::Mutex<()>,
    /// The changes whose final reply a test build kept back.
    kept_back: change::KeptBack,
    /// How long a picture's client is given to take each chunk.
    progress: Mutex<Duration>,
    shared: Shared,
    /// The station's lifecycle records, which keep the person's maintenance
    /// of the VM.
    lifecycle: super::lifecycle::Lifecycle,
    /// The agents' lanes it carries now, and what holds them.
    lanes: lanes::Lanes,
    /// The update of its guest it owns, taken by one at a time.
    owner: tokio::sync::Mutex<update::Owner>,
    /// Told when a client begins an update, so it is carried on at once.
    owner_waking: tokio::sync::Notify,
    /// Held through each write of the update's record, to its end: one
    /// at a time, and none of a generation that ended after the next's.
    update_writes: Arc<update::Writes>,
}

/// The live generation, and what the keeper says of its session.
#[derive(Default)]
struct Shared(Mutex<State>);

#[derive(Clone, Default)]
struct State {
    session: Option<Arc<Session>>,
    /// The generation last live, and what its guest declared.
    generation: Option<Generation>,
    declared: Option<Declared>,
    /// Why no generation is live.
    why: Option<String>,
    /// The guest's own report of its Stop, work and build, as last read,
    /// named by the generation it was read in.
    report: Option<Value>,
}

impl State {
    /// The guest's report, when it was read in `generation`.
    fn reported(&self, generation: Generation) -> Option<&Value> {
        self.report
            .as_ref()
            .filter(|report| report["generation"] == json!(generation))
    }
}

impl Shared {
    fn state(&self) -> MutexGuard<'_, State> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn live(&self) -> Option<Arc<Session>> {
        self.state().session.clone()
    }
}

/// A live generation: its control, its lanes, and the guest key its
/// handshake pinned.
struct Session {
    control: Control,
    desktop: Desktop,
    guest: PeerKey,
    /// The changes carried in it, settled once the station holds no claim
    /// on them.
    carried: change::Carried,
    /// Set once the keeper left it, for what it relays to end with it.
    retired: tokio::sync::watch::Sender<bool>,
}

impl Session {
    /// Leave it for good: its control retired, and everything relayed in it
    /// told to end.
    fn retire(&self) {
        self.control.retire();
        self.retired.send_replace(true);
    }

    /// Completes once the keeper left it.
    async fn ended(&self) {
        let mut retired = self.retired.subscribe();
        let _ = retired.wait_for(|retired| *retired).await;
    }
}

/// How an attempt at a generation ended.
enum Ended {
    /// It was live until it was lost or fell silent, as `why` says.
    Lost(String),
    /// It never went live, as `why` says.
    Failed(String),
    /// Atlas VM was not trusted, judged on `facts`.
    Untrusted(String, Facts),
}

impl Keeper {
    /// Hold the VM's session and clean up after its pairing; never returns.
    async fn run(&self) {
        tokio::join!(self.sessions(), self.cleaned());
    }

    /// Delete the enrolment a pairing kept and left, every [`FORGETTING`]
    /// until it is gone, whether or not a session is live: one a keeper that
    /// ended before it deleted it, or a deletion that failed, left. Only once
    /// paired: before, the pairing still needs it.
    async fn cleaned(&self) {
        while !self.forgotten.load(std::sync::atomic::Ordering::SeqCst) {
            let records = self.records.clone();
            if let Ok(Some(Enrolment::Paired(_))) = blocking(move || records.read()).await {
                self.forget_enrolment().await;
            }
            tokio::time::sleep(FORGETTING).await;
        }
    }

    /// Hold the VM's session, dialling again with backoff whenever it is
    /// lost; never returns.
    async fn sessions(&self) {
        let mut backoff = FIRST;
        loop {
            if self.handing.load(std::sync::atomic::Ordering::SeqCst) {
                return std::future::pending().await;
            }
            let wait = match self.generation().await {
                Ended::Lost(why) => {
                    self.unlive(why);
                    FIRST
                }
                Ended::Failed(why) => {
                    self.unlive(why);
                    backoff
                }
                Ended::Untrusted(why, facts) => {
                    self.unlive(why);
                    // Never dialled again until what it was judged on changes.
                    loop {
                        tokio::time::sleep(backoff).await;
                        backoff = (backoff * 2).min(MOST);
                        if self.reach.facts().await.as_ref() != Some(&facts) {
                            break;
                        }
                    }
                    continue;
                }
            };
            tokio::time::sleep(wait).await;
            backoff = (wait * 2).min(MOST);
        }
    }

    /// How long a picture's client is given to take each chunk the guest
    /// sends.
    fn progress(&self) -> Duration {
        *self.progress.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn unlive(&self, why: String) {
        let mut state = self.shared.state();
        state.session = None;
        state.why = Some(why);
    }

    /// One generation: paired first if need be, then opened and held until
    /// it is lost or falls silent.
    async fn generation(&self) -> Ended {
        let guest = match self.guest().await {
            Ok(guest) => guest,
            Err(ended) => return ended,
        };
        let stream = match self.reach.reach(Purpose::Control).await {
            Ok(stream) => stream,
            Err(refused) => return self.refused(refused),
        };
        // Of the changes the guest's hello names, only those the station
        // holds no claim on are settled: a marker that cannot be read now
        // claims every one.
        let claims = self.claimed().await;
        let claimed = |change: &atlas_core::files::Token| {
            claims
                .as_ref()
                .is_none_or(|claims| claims.iter().any(|claim| claim == change.as_str()))
        };
        let (control, mut events) =
            match control::open(stream, &self.own, guest, &self.vm, &claimed).await {
                Ok(opened) => opened,
                Err(error) => return Ended::Failed(error.error),
            };
        // What the hello named and could not settle yet is looked at again
        // each tick, as the changes carried in the generation are.
        let carried = change::Carried::of(
            control
                .unsettled()
                .iter()
                .filter(|change| claimed(change))
                .cloned(),
        );
        let peers = Peers {
            desktop: self.vm.clone(),
            own: self.own.clone(),
            key: guest,
        };
        let lanes = Dialled::new(self, Purpose::Lane);
        let desktop = Desktop::new(
            peers,
            control.generation(),
            control.outbox().clone(),
            Box::new(lanes),
        );
        let session = Arc::new(Session {
            control,
            desktop,
            guest,
            carried,
            retired: tokio::sync::watch::Sender::new(false),
        });
        {
            let mut state = self.shared.state();
            state.generation = Some(session.control.generation());
            state.declared = Some(session.control.declared().clone());
            state.session = Some(session.clone());
            state.why = None;
        }
        self.applied().revision = None;
        let work = async {
            let report = report(&session).await;
            self.shared.state().report = Some(report);
            tokio::join!(
                self.reconciled(&session),
                self.recovering(&session),
                self.updating(&session)
            );
        };
        tokio::pin!(work);
        let ended = loop {
            tokio::select! {
                event = events.recv() => match event {
                    Some(Event::Lost(why)) => break Ended::Lost(why.error),
                    None => break Ended::Lost("The control connection ended.".to_owned()),
                    // The guest lapses the generation itself: it is given up
                    // here and dialled again, nothing inferred of its state.
                    Some(Event::Silent) => {
                        break Ended::Lost("The guest fell silent.".to_owned());
                    }
                    // A Resume answered after its own caller stopped waiting:
                    // the guest may run, so the Stop is reconciled again.
                    Some(Event::Applied { outcome: Outcome::Resumed, .. }) => self.resumed(),
                    Some(_) => {}
                },
                () = &mut work => {}
            }
        };
        // Left for good, whatever a client still holds of it: no heartbeat of
        // it renews the guest's lease from here.
        session.retire();
        ended
    }

    /// The guest's key, pairing first when it is not paired and an
    /// enrolment is kept.
    async fn guest(&self) -> std::result::Result<PeerKey, Ended> {
        let records = self.records.clone();
        let kept = blocking(move || records.read())
            .await
            .map_err(|error| Ended::Failed(error.error))?;
        if let Some(Enrolment::Paired(paired)) = kept {
            return Ok(paired.guest);
        }
        let dial = Dialled::new(self, Purpose::Pairing);
        let pairing = Pairing {
            vm: &self.vm,
            name: &self.name,
            dial: &dial,
            own: &self.own,
            enrolments: self.enrolments.clone(),
            records: self.records.clone(),
        };
        match pair(&pairing).await {
            Ok(paired) => {
                self.forgotten
                    .store(true, std::sync::atomic::Ordering::SeqCst);
                Ok(paired.guest)
            }
            // A dial refused for trust is judged as any other dial.
            Err(error) => Err(match dial.untrusted() {
                Some((why, facts)) => self.refused(Refused::Untrusted { why, facts }),
                None => Ended::Failed(error.error),
            }),
        }
    }

    fn refused(&self, refused: Refused) -> Ended {
        match refused {
            Refused::Untrusted { why, facts } => {
                Ended::Untrusted(format!("{} was not dialled: {why}.", self.name), facts)
            }
            Refused::Failed(error) => Ended::Failed(error.error),
        }
    }

    /// What the keeper says of itself and its session.
    fn hello(&self) -> Value {
        let state = self.shared.state().clone();
        json!({
            "ok": true,
            "op": "hello",
            "keeper": {
                "protocol": clients::PROTOCOL,
                "capabilities": clients::CAPABILITIES,
                "build": env!("CARGO_PKG_VERSION"),
            },
            "guest": state.declared.as_ref().map(|declared| json!({
                "protocol": declared.protocol,
                "capabilities": declared.capabilities,
                "version": declared.version,
                "build": state.generation.and_then(|generation| state.reported(generation))
                    .map_or(Value::Null, |report| report["build"].clone()),
            })),
            "generation": state.generation,
            "live": state.session.is_some(),
            "why": state.why,
            "report": state.report,
            "instance": self.instance,
            "program": self.program.said(),
        })
    }

    /// Finish a handover an update recorded whose keeper did not live to
    /// keep its Stop: kept now, when none is. This keeper holds the VM's
    /// lock, so the one handing over is gone; the update that recorded it
    /// forgets the record once it sees this one serve.
    ///
    /// # Errors
    ///
    /// Fails when the Stop cannot be kept.
    fn finish_handover(&self) -> Result<()> {
        // A record that cannot be read holds as one that is there.
        let recorded = !matches!(super::handover::Record::kept(&self.folder), Ok(None));
        if recorded {
            self.stop.note_stop_unless_noted(StopWhy::Update)?;
        }
        Ok(())
    }

    /// Leave the live generation for good and dial no more: the session is
    /// the next keeper's.
    fn hand_over(&self) {
        self.handing
            .store(true, std::sync::atomic::Ordering::SeqCst);
        if let Some(session) = self.shared.live() {
            session.retire();
        }
        self.unlive("This keeper handed the session over to the next.".to_owned());
    }

    /// The refusal of what the guest of `session` cannot serve, lacking one
    /// of `needs`: what it lacks in `lacks`' words, given the VM's name, and
    /// what to do next in the one advice's; another protocol's own refusal.
    /// None when it serves them all. Nothing is sent to judge it.
    fn unserved(
        &self,
        session: &Session,
        needs: &[&str],
        lacks: impl FnOnce(&str) -> String,
    ) -> Option<Error> {
        let declared = session.control.declared();
        match judge(declared, needs) {
            Usable::Usable => None,
            Usable::MissingCapabilities { .. } => {
                let next = super::update::advice(&self.name, &self.update_facts(session));
                Some(Error::new(Code::Update, lacks(&self.name)).with("next", json!(next)))
            }
            incompatible @ Usable::Incompatible { .. } => incompatible.refusal(),
        }
    }

    /// What the guest of `session` is judged on: what it declared there,
    /// its build and version as it reported them in that very generation,
    /// its declared version while it reported none, beside this keeper's own
    /// build. A report of another generation is never taken: its build is
    /// then not known.
    fn update_facts(&self, session: &Session) -> super::update::Facts {
        let declared = session.control.declared();
        let report = self
            .shared
            .state()
            .reported(session.control.generation())
            .cloned()
            .unwrap_or_default();
        let version = report["version"].as_str().unwrap_or(&declared.version);
        super::update::Facts {
            declared: declared.capabilities.clone(),
            guest: super::update::Build::new(report["build"].as_str(), Some(version)),
            own: self.program.build(),
        }
    }
}

/// The guest's own report of its Stop and work, and the build and version
/// its daemon says it is, read on a lane of the generation and named by it;
/// its Stop is unknown, and its build and version none, when the guest does
/// not report them or the read fails.
async fn report(session: &Session) -> Value {
    let mut report = reported(session).await;
    report["generation"] = json!(session.control.generation());
    report
}

/// [`report`], before it is named by its generation.
async fn reported(session: &Session) -> Value {
    let unknown = json!({"stop": {"state": "unknown"}, "work": {"state": "unknown"},
        "build": null, "version": null});
    let declared = session.control.declared();
    if judge(declared, &[STOP_REPORT]).refusal().is_some() {
        return unknown;
    }
    let mut lines = Vec::new();
    let hello = json!({"op": "hello"});
    let read = read(session, LaneKind::Read, &hello, REPORT, |line| {
        lines.push(line);
        true
    });
    if read.await.is_err() {
        return unknown;
    }
    lines
        .last()
        .and_then(|line| serde_json::from_str::<Value>(line).ok())
        .filter(|hello| hello["ok"] == true)
        .map_or(unknown, |hello| {
            let daemon = &hello["daemon"];
            let said = |field: &str| {
                daemon[field]
                    .as_str()
                    .map_or(Value::Null, |said| json!(said))
            };
            json!({"stop": hello["stop"], "work": hello["work"],
                "build": said("build"), "version": said("version")})
        })
}

/// What of the station's Stop the live generation's guest applied: the
/// revision it confirmed, and how many Resumes it took since, so a
/// confirmation that comes after a Resume never counts for the Stop it lifted.
#[derive(Debug, Default)]
struct Applied {
    revision: Option<StopRevision>,
    resumes: u64,
}

impl Keeper {
    fn applied(&self) -> MutexGuard<'_, Applied> {
        self.applied.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Hold the guest to the station's Stop for the generation's life: read
    /// it every [`WATCH`], under the Resume's turn, and whenever its revision
    /// is not the one the guest applied, say Stop until the guest answers it
    /// applied. A Stop it could not keep, or an answer that does not come, is
    /// said again; a guest that does not acknowledge is told once for each.
    /// A marker that cannot be read is said, never confirmed. A Resume that
    /// lifts the marker never has a Stop said between its saying and its
    /// lift.
    async fn reconciled(&self, session: &Session) {
        loop {
            let said = {
                let _turn = self.turn.lock().await;
                let stop = self.stop.clone();
                let now = blocking(move || stop.stop_revision()).await;
                let mut applied = self.applied();
                let wanted = match now {
                    Ok(None) => {
                        applied.revision = None;
                        None
                    }
                    Ok(Some(revision)) if applied.revision.as_ref() == Some(&revision) => None,
                    Ok(Some(revision)) => Some(Some(revision)),
                    Err(_) => Some(None),
                };
                let resumes = applied.resumes;
                drop(applied);
                wanted.map(|revision| (revision, resumes, session.control.stop()))
            };
            if let Some((revision, resumes, Ok(asked))) = said {
                let confirmed = asked.seq().is_none()
                    || matches!(
                        tokio::time::timeout(ANSWER, asked.answer()).await,
                        Ok(Answer::Applied(Outcome::Stopped))
                    );
                let mut applied = self.applied();
                if confirmed && applied.resumes == resumes {
                    applied.revision = revision;
                }
            }
            self.settling(session).await;
            tokio::time::sleep(WATCH).await;
        }
    }

    /// A Resume the guest resumed: what the guest applied of the Stop no
    /// longer holds, so whatever Stop remains is said again.
    fn resumed(&self) {
        let mut applied = self.applied();
        applied.resumes += 1;
        applied.revision = None;
    }

    /// Delete the enrolment a pairing left, once, while it is not known to
    /// be gone: a failure is tried again later.
    async fn forget_enrolment(&self) {
        use std::sync::atomic::Ordering::SeqCst;
        if self.forgotten.load(SeqCst) {
            return;
        }
        let (enrolments, vm) = (self.enrolments.clone(), self.vm.clone());
        if blocking(move || enrolments.forget(&vm)).await.is_ok() {
            self.forgotten.store(true, SeqCst);
        }
    }
}

/// Carry `request` on a lane of `kind` of `session`, handing each reply
/// line to `line` as it comes until its final reply or `line` says to stop,
/// within `deadline`.
///
/// # Errors
///
/// Fails when the lane cannot be opened, fails before its final reply, or
/// the deadline passes first.
async fn read(
    session: &Session,
    kind: LaneKind,
    request: &Value,
    deadline: Duration,
    line: impl FnMut(String) -> bool,
) -> Result<()> {
    read_until(
        session,
        kind,
        request,
        deadline,
        std::future::pending(),
        line,
    )
    .await
}

/// [`read`], given up once `stop` gives why first, its lane still opening
/// included: nothing is said on a lane given up before it opened, and one
/// given up once open is cancelled on the guest; refused for that.
///
/// # Errors
///
/// As [`read`], and what `stop` gives.
async fn read_until(
    session: &Session,
    kind: LaneKind,
    request: &Value,
    deadline: Duration,
    stop: impl Future<Output = Error>,
    line: impl FnMut(String) -> bool,
) -> Result<()> {
    let until = tokio::time::Instant::now() + deadline;
    let late = || {
        Error::new(
            Code::Transport,
            "The guest did not answer the read in time.",
        )
    };
    tokio::pin!(stop);
    let opened = tokio::select! {
        opened = tokio::time::timeout_at(until, session.desktop.open(kind)) => {
            opened.map_err(|_| late())??
        }
        why = &mut stop => return Err(why),
    };
    let lane = opened.lane;
    let carried = tokio::select! {
        carried = tokio::time::timeout_at(until, carry(opened.stream, request, line)) => {
            carried.unwrap_or_else(|_| Err(late()))
        }
        why = &mut stop => Err(why),
    };
    // A read given up, by its reader or for a failure, is cancelled on the
    // guest too.
    if !matches!(carried, Ok(true)) {
        session.desktop.cancel(lane);
    }
    carried.map(drop)
}

/// Say `request` on the opened lane `stream` and hand each reply line to
/// `line` until the final one, which ends it: the guest says nothing after
/// its final reply, and may end the lane without finishing it, so nothing is
/// read past it. Whether the final reply came, rather than `line` saying to
/// stop first.
async fn carry<S: AsyncRead + AsyncWrite + Unpin>(
    mut stream: S,
    request: &Value,
    mut line: impl FnMut(String) -> bool,
) -> Result<bool> {
    let mut said = serde_json::to_vec(request)?;
    said.push(b'\n');
    stream.write_all(&said).await.map_err(lane_failed)?;
    stream.flush().await.map_err(lane_failed)?;
    let mut reader = BufReader::new(stream);
    loop {
        let Some(reply) = reply_line(&mut reader).await.map_err(lane_failed)? else {
            return Err(Error::new(
                Code::Transport,
                "The read's lane to the guest ended before its final reply.",
            ));
        };
        let reply = reply.trim_end().to_owned();
        let last = final_reply(&reply);
        if !line(reply) || last {
            return Ok(last);
        }
    }
}

/// The next reply line on `reader`, its newline taken off: none once the
/// lane ended between lines. Each line is bounded on its own by [`LINE`].
///
/// # Errors
///
/// Refuses a line the lane's end cut short, never passed on as if whole,
/// and one longer than [`LINE`]; fails when the lane does.
async fn reply_line<R: AsyncBufRead + Unpin>(reader: &mut R) -> std::io::Result<Option<String>> {
    let mut line = String::new();
    let read = tokio::io::AsyncReadExt::take(&mut *reader, LINE)
        .read_line(&mut line)
        .await?;
    if read == 0 {
        return Ok(None);
    }
    if line.pop() != Some('\n') {
        let why = if read as u64 >= LINE {
            "a reply line longer than the keeper carries"
        } else {
            "a reply line cut short by the lane's end"
        };
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, why));
    }
    Ok(Some(line))
}

/// Whether `line` is a request's final reply: an object that is not an
/// event. One that is no object ends the read too, as nothing after it can
/// be read as a reply.
fn final_reply(line: &str) -> bool {
    serde_json::from_str::<Value>(line).map_or(true, |reply| {
        reply
            .as_object()
            .is_none_or(|row| !row.get("event").is_some_and(Value::is_string))
    })
}

fn lane_failed(error: std::io::Error) -> Error {
    Error::new(
        Code::Transport,
        format!("The read's lane to the guest failed: {error}."),
    )
    .with_source(error)
}

/// The VM's state as the runner `runner` says it, its socket read and
/// written within `within`, off the runtime: the thread that asks ends with
/// the exchange.
async fn runner_state(
    runner: impl FnOnce() -> Result<super::runner::Runner> + Send + 'static,
    within: Duration,
) -> Result<super::runner::Reply> {
    blocking(move || runner()?.read_within(&super::runner::Operation::State, within)).await
}

/// `work`, which waits on the disk or the Keychain, off the runtime.
async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T> + Send + 'static,
) -> Result<T> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|_| Error::new(Code::State, "The keeper's work stopped."))?
}

#[cfg(test)]
#[path = "keeper_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "keeper/advice_tests.rs"]
mod advice_tests;

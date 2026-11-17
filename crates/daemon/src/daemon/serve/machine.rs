//! A machine desktop's daemon serving its station over their session: the
//! control connection, which gives each generation its number, and one
//! lane per request, admitted in the live generation and bound to what it
//! carries before anything is done.
use super::{Asker, Daemon, Gate, Reader, Writer, answer, write};
use crate::machine::{
    changes::{Asked, Changes, State},
    ended::OWNER,
    processes::Supervising,
    stop::{Kept as StopKept, Stop, Why},
    vault::Vault,
};
use atlas_core::{
    desktop::DesktopId,
    error::{Code, Error, Result},
    protocol::{self, ConnectionPhase, MAX_REQUEST, Request, failure},
    session::key::PeerKey,
};
use atlas_session::{
    authority::{Authority, Hook, Retired},
    clock::Clock,
    control::{self, Guest, Outbox, Outcome, Refusal},
    credit::Credit,
    generation::{Generation, Generations},
    keys::Keypair,
    lane::{Header, LaneId, LaneKind, Lanes, Opener, read_header},
    secure::{Purpose, connect_as_guest},
    station::Raw,
};
use serde_json::Value;
use std::{
    collections::BTreeSet,
    future::Future,
    io,
    pin::Pin,
    sync::{
        Arc, Mutex as Held, PoisonError,
        atomic::{AtomicU64, AtomicUsize, Ordering::SeqCst},
    },
    time::Duration,
};
use tokio::{
    io::AsyncReadExt,
    sync::{Mutex, Semaphore, watch},
    task::{AbortHandle, JoinSet},
};

/// How often the watchdog checks the generation's lease, whatever else
/// happens or does not.
const WATCHDOG: Duration = Duration::from_millis(250);

/// The longest a lane's handshake and header may take.
const OPENING: Duration = Duration::from_secs(10);

/// Lanes served at once, their handshakes included; more are refused.
const LANES: usize = 32;

/// Control connections being set up at once; more are refused.
const CONTROLS: usize = 4;

/// Connections not yet saying what they are for, at once; more are closed.
const FRAMING: usize = 16;

/// Pairing connections tried at once; more are closed.
const PAIRINGS: usize = 4;

/// A connection the station made, once it says what it is for: before any
/// authentication, so it is read on its own, bounded and timed.
pub type Framing = Pin<Box<dyn Future<Output = io::Result<(Purpose, Raw)>> + Send>>;

/// The next connection the station made, as soon as it is made.
pub type Accepting<'a> = Pin<Box<dyn Future<Output = io::Result<Framing>> + Send + 'a>>;

/// How the station reaches the guest: each connection it makes, then what
/// it is for. Only the listener's own failure ends serving.
pub trait Listener: Send + Sync {
    fn accept(&self) -> Accepting<'_>;
}

/// Who the guest is to the station it paired with.
pub struct Paired {
    pub keys: Keypair,
    pub desktop: DesktopId,
    pub station: PeerKey,
    /// The enrolment its pairing was committed for.
    pub enrolment: atlas_core::files::Token,
    /// Its pairing records: what a pairing connection finishes, never
    /// another station's.
    pub pairing: Arc<dyn atlas_session::pairing::GuestRecords>,
    pub generations: Arc<dyn Generations + Send + Sync>,
    /// What each change it was asked for came to, on its disk.
    pub changes: Arc<Changes<Arc<dyn Vault>>>,
    /// Whether agents are stopped here, on its disk.
    pub stop: Arc<Stop<Arc<dyn Vault>>>,
    /// The updates of its own Atlas, on its disk.
    pub updates: Arc<crate::machine::update::Updates<Arc<dyn Vault>>>,
}

impl std::fmt::Debug for Paired {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Paired")
            .field("desktop", &self.desktop)
            .field("station", &self.station)
            .finish_non_exhaustive()
    }
}

/// What the station said that the guest's work carries out.
#[derive(Debug, Default)]
pub struct Said {
    pub stops: AtomicUsize,
    pub releases: AtomicUsize,
    /// Every generation that retired, and why.
    pub retired: Held<Vec<(Generation, Retired)>>,
}

/// The guest's full Stop and Resume: what a Stop from the station, and a
/// generation lost, carry out. Every one goes to one worker, in the order
/// it came, so a Resume never undoes a Stop that came after it, and each
/// numbered one is answered in that order.
struct Halting {
    daemon: Arc<Daemon>,
    transitions: tokio::sync::mpsc::UnboundedSender<Transition>,
    /// Every Stop that came, saved or not yet: a Resume reopens nothing
    /// when one came after it.
    received: Arc<AtomicU64>,
    /// Held while a Stop is taken in or a Resume reopens: the two never
    /// cross. Never held across storage.
    turning: Arc<Held<()>>,
    /// Shut by a test to hold a Resume between lifting the Stop on disk and
    /// reopening.
    #[cfg(test)]
    reopening: watch::Sender<bool>,
}

/// What the worker carries out.
enum Transition {
    /// A Stop for `why`, answered when numbered.
    Stop(Why, Option<Answer>),
    /// A Resume, from the generation it came in, after so many Stops came:
    /// lifted only while that generation is live and no Stop came since;
    /// answered when numbered.
    Resume(Authority, u64, Option<Answer>),
}

/// How long a numbered Stop waits for the work it ended to have ended
/// before it is answered: past it, it is answered as kept, and the hello
/// shows the work still being ended.
const ENDING: Duration = Duration::from_secs(10);

/// Where a numbered Stop or Resume is answered: on the control of the
/// generation it came in, only while that generation is live, so one lost
/// first leaves it unanswered.
struct Answer {
    seq: u64,
    authority: Authority,
    outbox: Arc<Outbox>,
}

impl Answer {
    fn give(self, outcome: Outcome) {
        let Self {
            seq,
            authority,
            outbox,
        } = self;
        let _ = authority.if_live(|| outbox.say(control::Said::Applied { seq, outcome }));
    }
}

/// What a Resume came to.
enum Resumed {
    /// The Stop is lifted and agents are admitted.
    Lifted,
    /// Agents stay stopped as they were.
    Refused(Refusal),
    /// Lifted on disk, then refused at the last moment: a full Stop of its
    /// own puts it back.
    PutBack(Refusal),
}

/// What the worker reads and changes.
struct Worker {
    daemon: Arc<Daemon>,
    stop: Arc<Stop<Arc<dyn Vault>>>,
    gate: Arc<crate::machine::update::Gate>,
    /// Every Stop that came, as [`Halting::received`].
    since: Arc<AtomicU64>,
    /// As [`Halting::turning`].
    turn: Arc<Held<()>>,
    /// How closed the guest is: a numbered Resume is answered `resumed` only
    /// where agents' changes are admitted.
    posture: Arc<Seen>,
    #[cfg(test)]
    reopening: watch::Receiver<bool>,
}

impl Worker {
    /// Carry out a Stop for `why`: saved, then its work ended. Whether it
    /// was saved.
    async fn stop(&self, why: Why) -> bool {
        let saving = self.stop.clone();
        let saved = tokio::task::spawn_blocking(move || saving.stop(why)).await;
        // Saved, or as saved as it can be: now its work ends.
        // Its work is being ended now: none of it counts as settled until
        // judged, so no Resume passes over it.
        self.daemon.keepers.ending_all();
        self.daemon.halted.send_modify(|halt| {
            halt.stops += 1;
            halt.halted = true;
        });
        if let Some(jobs) = &self.daemon.jobs {
            let reason = atlas_core::protocol::StopReason::Person;
            let request = atlas_core::protocol::JobRequest::StopAll { reason };
            let _ = jobs.serve(request).await;
        }
        matches!(saved, Ok(Ok(())))
    }

    /// Wait, up to [`ENDING`], until no keeper holds work: what a Stop
    /// ended has ended, or been left to recovery.
    async fn ended(&self) {
        let keepers = &self.daemon.keepers;
        let _ = tokio::time::timeout(ENDING, async {
            while keepers.keeping() {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await;
    }

    /// A Resume from `authority`'s generation, after `at` Stops came, with
    /// a Stop not saved since this daemon started if `unsaved`. A
    /// `numbered` one, whose answer says agents are admitted, also stays
    /// stopped while the guest's posture refuses their changes.
    async fn resume(
        &self,
        authority: &Authority,
        at: u64,
        unsaved: bool,
        numbered: bool,
    ) -> Resumed {
        let (worker, since) = (&self.daemon, &self.since);
        let current = || authority.check().is_ok() && since.load(SeqCst) == at;
        if unsaved {
            return Resumed::Refused(Refusal::Unkept);
        }
        if !current() {
            return Resumed::Refused(Refusal::LaterStop);
        }
        if numbered && self.posture.now().await.held().is_some() {
            return Resumed::Refused(Refusal::Posture);
        }
        // Never while an update is unresolved or being admitted, and none
        // admitted while this commits.
        let Some(_resuming) = self.gate.resume() else {
            return Resumed::Refused(Refusal::Update);
        };
        // What earlier daemons left, looked at again under this saved Stop,
        // and never lifted over: work still being ended, or not known to
        // have ended.
        let (looking, saving) = (worker.clone(), self.stop.clone());
        let resolved = tokio::task::spawn_blocking(move || {
            // Forgotten only under a Stop read back saved.
            let saved = matches!(saving.kept(), Ok(StopKept::Stopped { .. }));
            looking.recover_now(saved)
        })
        .await
        .unwrap_or(false);
        if !resolved || worker.keepers.unsettled() {
            return Resumed::Refused(Refusal::Work);
        }
        if worker.input.as_ref().is_some_and(|input| input.unsettled()) {
            return Resumed::Refused(Refusal::Input);
        }
        if committing(worker) {
            return Resumed::Refused(Refusal::Work);
        }
        if !current() {
            return Resumed::Refused(Refusal::LaterStop);
        }
        let (lifting, checking) = (self.stop.clone(), authority.clone());
        let lifted = tokio::task::spawn_blocking(move || {
            // Checked again at the last moment before the disk.
            checking.check()?;
            lifting.resume()
        })
        .await;
        if !matches!(lifted, Ok(Ok(()))) {
            return Resumed::Refused(Refusal::Unkept);
        }
        #[cfg(test)]
        {
            let mut reopening = self.reopening.clone();
            let _ = reopening.wait_for(|open| *open).await;
        }
        let turned = self.turn.lock().unwrap_or_else(PoisonError::into_inner);
        // Reopened only while no work stands unsettled, and none can come to
        // meanwhile.
        // Input is looked at again here too, last: a service that ended
        // meanwhile keeps agents stopped.
        // The generation is looked at without retiring it: its hook halts,
        // which takes this turn.
        // A stage authorizes its commit under this turn too, so none starts
        // committing between here and reopening.
        let later = authority.if_live(|| Ok(())).is_err() || since.load(SeqCst) != at;
        let (mut input_held, mut posture_held) = (false, false);
        let mut reopening = || {
            !later
                && !committing(worker)
                && worker.keepers.settled_then(|| {
                    let reopen = || worker.halted.send_modify(|halt| halt.halted = false);
                    // Lifted under the same turn, so a Stop that comes next
                    // holds input again after it.
                    match worker.input.as_deref() {
                        Some(input) => {
                            let lifted = input.lift_if_settled(reopen);
                            input_held = !lifted;
                            lifted
                        }
                        None => {
                            reopen();
                            true
                        }
                    }
                })
        };
        // A numbered one's posture is looked at again with the decision,
        // and none is taken in meanwhile: storage may have taken a while.
        let reopened = if numbered {
            self.posture.admitted_then(reopening).unwrap_or_else(|| {
                posture_held = true;
                false
            })
        } else {
            reopening()
        };
        if reopened {
            return Resumed::Lifted;
        }
        // Lifted on disk by a Resume its generation no longer gives: a full
        // Stop of its own puts it back.
        since.fetch_add(1, SeqCst);
        worker.halted.send_modify(|halt| halt.halted = true);
        drop(turned);
        Resumed::PutBack(if later {
            Refusal::LaterStop
        } else if posture_held {
            Refusal::Posture
        } else if input_held {
            Refusal::Input
        } else {
            Refusal::Work
        })
    }
}

impl Halting {
    /// Halting for `daemon`, its Stop kept in `stop`.
    fn new(
        daemon: Arc<Daemon>,
        stop: Arc<Stop<Arc<dyn Vault>>>,
        gate: Arc<crate::machine::update::Gate>,
        posture: Arc<Seen>,
    ) -> Arc<Self> {
        let (transitions, mut coming) = tokio::sync::mpsc::unbounded_channel();
        let received = Arc::new(AtomicU64::new(0));
        let turning = Arc::new(Held::new(()));
        #[cfg(test)]
        let reopening = watch::Sender::new(true);
        let worker = Worker {
            daemon: daemon.clone(),
            stop,
            gate,
            since: received.clone(),
            turn: turning.clone(),
            posture,
            #[cfg(test)]
            reopening: reopening.subscribe(),
        };
        tokio::spawn(async move {
            // A Stop that could not be saved: what agents are is unknown,
            // and no Resume lifts it.
            let mut unsaved = false;
            while let Some(transition) = coming.recv().await {
                match transition {
                    Transition::Stop(why, answer) => {
                        let saved = worker.stop(why).await;
                        unsaved |= !saved;
                        if let Some(answer) = answer {
                            worker.ended().await;
                            answer.give(if saved {
                                Outcome::Stopped
                            } else {
                                Outcome::Refused {
                                    why: Refusal::Unkept,
                                }
                            });
                        }
                    }
                    Transition::Resume(authority, at, answer) => {
                        let numbered = answer.is_some();
                        let resumed = worker.resume(&authority, at, unsaved, numbered).await;
                        let why = match resumed {
                            Resumed::Lifted => None,
                            Resumed::Refused(why) => Some(why),
                            // Put back before the Resume is answered, and
                            // before anything that came after it is done.
                            Resumed::PutBack(why) => {
                                unsaved |= !worker.stop(Why::ControlLost).await;
                                Some(why)
                            }
                        };
                        if let Some(answer) = answer {
                            answer
                                .give(why.map_or(Outcome::Resumed, |why| Outcome::Refused { why }));
                        }
                    }
                }
            }
        });
        Arc::new(Self {
            daemon,
            transitions,
            received,
            turning,
            #[cfg(test)]
            reopening,
        })
    }

    /// Stop agents for `why`: no change is taken from now on, and the
    /// worker saves the Stop, then ends the work it holds, and gives
    /// `answer` when there is one.
    fn halt(&self, why: Why, answer: Option<Answer>) {
        let _taking = self.turning.lock().unwrap_or_else(PoisonError::into_inner);
        self.received.fetch_add(1, SeqCst);
        self.daemon.halted.send_modify(|halt| halt.halted = true);
        // Input is held on receipt, whatever saving the Stop comes to.
        if let Some(input) = &self.daemon.input {
            input.hold(true);
        }
        let _ = self.transitions.send(Transition::Stop(why, answer));
    }

    /// The person's Resume, in `authority`'s generation, giving `answer`
    /// when there is one.
    fn resume(&self, authority: Authority, answer: Option<Answer>) {
        let at = self.received.load(SeqCst);
        let _ = self
            .transitions
            .send(Transition::Resume(authority, at, answer));
    }
}

/// What a control connection's reader hands on: never the machine itself,
/// so a reader still going never keeps it alive.
struct Hand {
    said: Arc<Said>,
    changes: Arc<Changes<Arc<dyn Vault>>>,
    halting: Arc<Halting>,
    /// The generation its connection is in.
    authority: Authority,
    /// What the guest says on that connection.
    outbox: Arc<Outbox>,
    /// The generation's mirrors, if the guest serves them.
    mirror: Option<Arc<crate::machine::mirror::Owner>>,
}

impl Hand {
    /// Where the Stop or Resume numbered `seq` is answered.
    fn answer(&self, seq: u64) -> Answer {
        Answer {
            seq,
            authority: self.authority.clone(),
            outbox: self.outbox.clone(),
        }
    }
}

impl Guest for Hand {
    fn stop(&self) {
        self.said.stops.fetch_add(1, SeqCst);
        self.halting.halt(Why::Person, None);
    }

    fn release(&self) {
        self.said.releases.fetch_add(1, SeqCst);
    }

    fn resume(&self) {
        self.halting.resume(self.authority.clone(), None);
    }

    fn halt(&self, seq: u64) {
        self.said.stops.fetch_add(1, SeqCst);
        self.halting.halt(Why::Person, Some(self.answer(seq)));
    }

    fn lift(&self, seq: u64) {
        self.halting
            .resume(self.authority.clone(), Some(self.answer(seq)));
    }

    /// The station dials every lane itself: the guest is never asked to.
    fn dial(&self, _: LaneId, _: LaneKind) {}

    fn mirror(&self, list: u64, endpoints: Vec<atlas_session::mirror::Endpoint>) {
        if let Some(mirror) = &self.mirror {
            mirror.list(list, endpoints);
        }
    }

    /// Forgotten off the runtime, without waiting: one not forgotten now
    /// is named again at the next hello.
    fn settled(&self, changes: Vec<atlas_core::files::Token>) {
        let records = self.changes.clone();
        tokio::task::spawn_blocking(move || {
            let _ = records.settled(&changes);
        });
    }
}

/// Tasks ended when their owner goes.
#[derive(Default)]
struct Tasks(Vec<AbortHandle>);

impl Drop for Tasks {
    fn drop(&mut self) {
        for task in &self.0 {
            task.abort();
        }
    }
}

/// How a machine's daemon starts, off the runtime: work an earlier daemon
/// may have left running stops agents, saved first, and what it left is then
/// recovered, never forgotten without that saved Stop. Agents stay stopped
/// when the Stop says so or cannot be told.
async fn start(
    daemon: Arc<Daemon>,
    stop: Arc<Stop<Arc<dyn Vault>>>,
    changes: Arc<Changes<Arc<dyn Vault>>>,
) {
    // What an earlier daemon staged is put right before any lane is served;
    // stages are refused when it cannot be.
    if let Some(stages) = daemon.stages.clone() {
        let _ = tokio::task::spawn_blocking(move || stages.start(&changes)).await;
    }
    let reading = (daemon.clone(), stop);
    let (left, saved, clear) = tokio::task::spawn_blocking(move || {
        let (daemon, stop) = reading;
        let left = daemon.supervising.as_ref().is_some_and(
            |supervising| !matches!(supervising.kept.kept(), Ok(work) if work.is_empty()),
        );
        let saved = left && stop.stop(Why::Restarted).is_ok();
        (left, saved, matches!(stop.kept(), Ok(StopKept::Clear)))
    })
    .await
    .unwrap_or((true, false, false));
    daemon.halted.send_modify(|halt| halt.halted = !clear);
    // Stopped before it started, or unable to tell: input is held before
    // anything is served.
    if !clear && let Some(input) = &daemon.input {
        input.hold(true);
    }
    if !left {
        daemon.keepers.orphans(false);
        return;
    }
    // Recovered while serving, agents stopped meanwhile.
    tokio::task::spawn_blocking(move || {
        daemon.recover_published(crate::machine::processes::LEFT_ENDING, saved)
    });
}

/// What work may still run on the guest, each part unknown when it cannot
/// be told. Blocking.
///
/// The supervisors' record is read first, then the keepers and whether work
/// was handed to recovery, both at once: a supervisor that starts or is
/// handed over meanwhile is counted, or leaves the supervisors unknown,
/// never missed.
fn work(daemon: &Daemon, jobs: Option<&jobs::Jobs>) -> super::super::facts::Work {
    counted(daemon, jobs, scanned(daemon))
}

/// [`work`], with `between` run between its two reads.
#[cfg(test)]
fn work_scanned_then(
    daemon: &Daemon,
    jobs: Option<&jobs::Jobs>,
    between: impl FnOnce(),
) -> super::super::facts::Work {
    let unresolved = scanned(daemon);
    between();
    counted(daemon, jobs, unresolved)
}

/// The supervisors kept that no keeper holds and are not known to be done
/// with: none when that cannot be told. Blocking.
fn scanned(daemon: &Daemon) -> Option<usize> {
    let keepers = &daemon.keepers;
    daemon.supervising.as_ref().map_or(Some(0), |supervising| {
        crate::machine::processes::unresolved(
            &*supervising.kept,
            &supervising.proofs,
            |supervisor| keepers.owns(supervisor),
            |supervisor| !supervisor.ended(),
        )
    })
}

/// What work may still run, the supervisors' record read already as
/// `unresolved`: the keepers and whether work was handed to recovery read
/// now, together.
fn counted(
    daemon: &Daemon,
    jobs: Option<&jobs::Jobs>,
    unresolved: Option<usize>,
) -> super::super::facts::Work {
    let (held, orphaned) = daemon.keepers.snapshot();
    super::super::facts::Work {
        jobs: jobs.map_or(Some(0), jobs::Jobs::running),
        changes: Some(held),
        // Work left that recovery could not show done with is never none.
        supervisors: unresolved.filter(|left| *left > 0 || !orphaned),
    }
}

/// Whether a stage is committing: a Resume waits for what it comes to.
fn committing(daemon: &Daemon) -> bool {
    daemon
        .stages
        .as_ref()
        .is_some_and(|stages| stages.committing())
}

/// Keep as ended each change in `changes` whose outcome is unknown and
/// whose work is proven gone by evidence of its own kind: `only` that one
/// when it is named, and the changes to input among them only when
/// `released` names them. Anything that cannot be read or kept stays as it
/// is. Blocking.
fn end_proven(
    daemon: &Daemon,
    changes: &Changes<Arc<dyn Vault>>,
    only: Option<&atlas_core::files::Token>,
    released: &BTreeSet<atlas_core::files::Token>,
) {
    let Some(supervising) = daemon.supervising.as_ref() else {
        return;
    };
    let Ok(unresolved) = changes.unresolved() else {
        return;
    };
    let unresolved = unresolved
        .into_iter()
        .filter(|intended| only.is_none_or(|only| intended.change == *only))
        .collect();
    let keepers = &daemon.keepers;
    let evidence = crate::machine::ended::Evidence {
        kept: &*supervising.kept,
        proofs: &supervising.proofs,
        carried: &|change| keepers.carries(change),
        owned: &|supervisor| keepers.owns(supervisor),
        running: &|supervisor| !supervisor.ended(),
        released: &|change| released.contains(change),
    };
    for change in evidence.proven_gone(unresolved) {
        let _ = changes.ended(&change);
    }
}

/// The changes to input kept with their outcome unknown that this daemon
/// does not carry out now: `only` that one when it is named. Blocking.
fn input_unresolved(
    daemon: &Daemon,
    changes: &Changes<Arc<dyn Vault>>,
    only: Option<&atlas_core::files::Token>,
) -> BTreeSet<atlas_core::files::Token> {
    let Ok(unresolved) = changes.unresolved() else {
        return BTreeSet::new();
    };
    unresolved
        .into_iter()
        .filter(|intended| crate::machine::ended::INPUT.contains(&intended.operation.as_str()))
        .filter(|intended| only.is_none_or(|only| intended.change == *only))
        .filter(|intended| !daemon.keepers.carries(&intended.change))
        .map(|intended| intended.change)
        .collect()
}

/// How long the input service has to say whether it holds anything, when
/// changes to input wait on it.
const INPUT_QUIET: Duration = Duration::from_secs(1);

/// Of `unresolved`, changes to input already seen no longer carried out
/// here, those the input service let go of: all of them once its own hello,
/// read after they were seen so, says no owner holds input and nothing it
/// pressed is held or unsettled. None when it cannot say so in time, is not
/// running, or holds anything.
async fn input_released(
    daemon: &Daemon,
    unresolved: BTreeSet<atlas_core::files::Token>,
) -> BTreeSet<atlas_core::files::Token> {
    if unresolved.is_empty() {
        return unresolved;
    }
    let Some(input) = daemon.input.as_deref() else {
        return BTreeSet::new();
    };
    let hello = tokio::time::timeout(INPUT_QUIET, forward(input, 0, br#"{"op":"hello"}"#)).await;
    let quiet = hello.is_ok_and(|hello| {
        hello.is_ok_and(|hello| {
            hello["ok"] == true
                && hello["controlActive"] == false
                && hello["heldInput"] == false
                && hello["unsettled"] == false
        })
    });
    if quiet { unresolved } else { BTreeSet::new() }
}

/// What a machine guest was made with, which decides what it declares.
#[derive(Clone, Copy, Debug, Default)]
struct Serving {
    supervising: bool,
    stages: bool,
    input: bool,
    mirrors: bool,
    jobs: bool,
}

impl Serving {
    /// Whether it serves `capability`: one it does not know it never does.
    fn serves(self, capability: &str) -> bool {
        use atlas_session::compatibility::{
            ACKNOWLEDGED, CHANGES, COPIES, ENDED, INPUT_LANES, JOBS, LEFTOVERS, MACHINE_PROOF,
            MIRRORS, STAGE, STOP_REPORT, UPDATES,
        };
        match capability {
            CHANGES | STOP_REPORT | ACKNOWLEDGED | MACHINE_PROOF => true,
            // What a change whose outcome is unknown came to is proven by its
            // supervisors.
            LEFTOVERS | ENDED => self.supervising,
            // An update's bundle comes as a stage.
            STAGE | COPIES | UPDATES => self.stages,
            INPUT_LANES => self.input,
            MIRRORS => self.mirrors,
            JOBS => self.jobs,
            _ => false,
        }
    }

    /// The capabilities it declares, in the order a machine guest declares
    /// them.
    fn declared(self) -> Vec<String> {
        atlas_session::compatibility::MACHINE
            .iter()
            .filter(|capability| self.serves(capability))
            .map(|capability| (*capability).to_owned())
            .collect()
    }
}

/// The generation live now, and the tasks that keep it.
struct Live {
    authority: Authority,
    lanes: Arc<Lanes>,
    /// What the guest says on the generation's control connection.
    outbox: Arc<Outbox>,
    /// What the guest may send on its bulk lanes, given back over control.
    credit: Arc<Credit>,
    /// The generation's mirrors, if the guest serves them.
    mirror: Option<Arc<crate::machine::mirror::Owner>>,
    _tasks: Tasks,
}

/// A machine's daemon, serving the station it paired with.
pub struct Machine {
    daemon: Arc<Daemon>,
    paired: Paired,
    clock: Arc<dyn Clock>,
    live: Mutex<Option<Live>>,
    halting: Arc<Halting>,
    /// Finding how it started: what Stop says, and what an earlier daemon
    /// left. Serving waits for it.
    starting: std::sync::Mutex<Option<tokio::task::JoinHandle<()>>>,
    /// Its mirrors of the station's loopback endpoints, if it serves them.
    mirrors: Option<crate::machine::mirror::Mirrors>,
    /// Its background jobs, kept beside its supervisors; none when it has
    /// none, or their folder could not be made.
    jobs: Option<Arc<jobs::Jobs>>,
    /// Blocking work begun for connections not yet authenticated, or that
    /// may be cancelled, still running.
    blocking: Arc<Semaphore>,
    /// How closed the guest is: agents' lanes are held unless it is closed.
    posture: Arc<Seen>,
    /// Where an update's admission and a Resume pass, never crossing.
    gate: Arc<crate::machine::update::Gate>,
    /// What carries each update to its end.
    reconciler: Arc<crate::machine::update::Reconciler<Arc<dyn Vault>>>,
    /// Set once it serves its station: started, recovered, taking lanes.
    serving: Arc<std::sync::atomic::AtomicBool>,
    /// The digest of the executable it started from, read once at its start,
    /// so a file replaced while it runs never changes what it says.
    build: Option<String>,
    pub said: Arc<Said>,
}

/// How long a reading of how closed the guest is holds before an agent's
/// lane, or a numbered Resume, reads it again.
const POSTURE_FRESH: Duration = Duration::from_secs(10);

/// How closed the guest is, as last read, and when: held unless closed, and
/// before any reading. (Its lease clock, `CLOCK_MONOTONIC`, was seen to
/// count a minute's stall of its VM.)
struct Seen {
    last: std::sync::Mutex<(super::super::facts::Posture, Option<std::time::Instant>)>,
    /// Where it is read again, off the runtime.
    blocking: Arc<Semaphore>,
}

impl Seen {
    /// Keep `posture` as read now.
    fn keep(&self, posture: super::super::facts::Posture) {
        *self.last.lock().unwrap_or_else(PoisonError::into_inner) =
            (posture, Some(std::time::Instant::now()));
    }

    /// Run `then` only while the posture last kept admits agents' changes,
    /// with none kept meanwhile: none when it holds them.
    fn admitted_then<T>(&self, then: impl FnOnce() -> T) -> Option<T> {
        let last = self.last.lock().unwrap_or_else(PoisonError::into_inner);
        last.0.held().is_none().then(then)
    }

    /// How closed the guest is: as last read while that is fresh, else
    /// read again; unknown when it cannot be read now.
    async fn now(&self) -> super::super::facts::Posture {
        let (posture, at) = *self.last.lock().unwrap_or_else(PoisonError::into_inner);
        if at.is_some_and(|at| at.elapsed() < POSTURE_FRESH) {
            return posture;
        }
        let read = bounded::off_runtime(&self.blocking, READ_POSTURE)
            .await
            .unwrap_or_default();
        self.keep(read);
        read
    }
}

/// How closed the guest is now; in tests, closed, so none reads the Mac it
/// runs on.
#[cfg(not(test))]
const READ_POSTURE: fn() -> super::super::facts::Posture = super::super::facts::Posture::now;
#[cfg(test)]
const READ_POSTURE: fn() -> super::super::facts::Posture = || super::super::facts::Posture::CLOSED;

impl Machine {
    pub fn new(
        mut daemon: Daemon,
        paired: Paired,
        clock: Arc<dyn Clock>,
        supervising: Supervising,
        input: Option<crate::machine::input::Input>,
        stages: Option<crate::machine::stage::Stages>,
        mirrors: Option<crate::machine::mirror::Mirrors>,
    ) -> Arc<Self> {
        daemon.kept = Some(paired.changes.clone());
        daemon.supervising = Some(supervising);
        daemon.input = input.map(Arc::new);
        daemon.stages = stages.map(Arc::new);
        let jobs = daemon.supervising.as_ref().and_then(|supervising| {
            let folder = supervising.proofs.with_file_name("jobs");
            jobs::Jobs::open(&folder, nix::unistd::getuid().as_raw())
                .ok()
                .map(Arc::new)
        });
        // Nothing is taken until it is known whether agents were stopped,
        // and what an earlier daemon left.
        daemon.halted.send_modify(|halt| halt.halted = true);
        daemon.keepers.orphans(true);
        let daemon = Arc::new(daemon);
        // Taken as unresolved until the record is read: no Resume passes an
        // update not yet looked at.
        let gate = Arc::new(crate::machine::update::Gate::new(true));
        let serving = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let reconciler = crate::machine::update::Reconciler::new(
            paired.updates.clone(),
            paired.changes.clone(),
            gate.clone(),
            Arc::new(crate::machine::update::Guest {
                input: daemon.input.clone(),
                serving: serving.clone(),
            }),
        );
        tokio::spawn(reconciler.clone().run());
        reconciler.wake();
        if let Some(input) = &daemon.input {
            let (mut standing, waking) = (input.standing(), reconciler.clone());
            tokio::spawn(async move {
                while standing.changed().await.is_ok() {
                    waking.wake();
                }
            });
        }
        let blocking = Arc::new(Semaphore::new(bounded::BLOCKING));
        let posture = Arc::new(Seen {
            last: std::sync::Mutex::default(),
            blocking: blocking.clone(),
        });
        let halting = Halting::new(
            daemon.clone(),
            paired.stop.clone(),
            gate.clone(),
            posture.clone(),
        );
        let starting = tokio::spawn(start(
            daemon.clone(),
            paired.stop.clone(),
            paired.changes.clone(),
        ));
        Arc::new(Self {
            daemon,
            paired,
            clock,
            live: Mutex::new(None),
            halting,
            starting: std::sync::Mutex::new(Some(starting)),
            mirrors,
            jobs,
            posture,
            gate,
            reconciler,
            serving,
            blocking,
            build: atlas_native::native::service::own_build(),
            said: Arc::default(),
        })
    }

    /// Take the guest to be as `posture` says, as a hello would.
    #[cfg(test)]
    pub(in crate::daemon) fn seen_as(&self, posture: super::super::facts::Posture) {
        self.posture.keep(posture);
    }

    /// What this guest declares it serves: what it was made with, so it
    /// never declares a service it does not have, each by its name in
    /// [`atlas_session::compatibility::MACHINE`].
    fn declared(&self) -> atlas_session::compatibility::Declared {
        let serving = Serving {
            supervising: self.daemon.supervising.is_some(),
            stages: self.daemon.stages.is_some(),
            input: self.daemon.input.is_some(),
            mirrors: self.mirrors.is_some(),
            jobs: self.jobs.is_some(),
        };
        atlas_session::compatibility::Declared::ours(serving.declared())
    }

    /// Serve every connection `listener` gives, until it fails.
    ///
    /// # Errors
    ///
    /// Returns the listener's failure.
    pub async fn serve(self: Arc<Self>, listener: &dyn Listener) -> Result<()> {
        // Connections wait until the daemon knows how it started.
        let starting = self
            .starting
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        if let Some(starting) = starting {
            let _ = starting.await;
        }
        // What an earlier daemon's jobs left in their folder goes, but what
        // their records still name: before any lane is taken, so no job of
        // this daemon's can have started yet.
        if let Some((jobs, supervising)) = self.jobs.clone().zip(self.daemon.supervising.as_ref()) {
            let (kept, sweeping) = (supervising.kept.clone(), jobs.clone());
            let swept = tokio::task::spawn_blocking({
                let kept = kept.clone();
                move || sweeping.sweep(&*kept)
            })
            .await;
            // Their work may still write, unproved ended: their logs stay bounded.
            jobs.bound_kept(swept.unwrap_or_default(), &kept);
        }
        let (framing, lanes, controls, pairings) = (
            Arc::new(Semaphore::new(FRAMING)),
            Arc::new(Semaphore::new(LANES)),
            Arc::new(Semaphore::new(CONTROLS)),
            Arc::new(Semaphore::new(PAIRINGS)),
        );
        let mut connections = JoinSet::new();
        // Serving from here: an update this build was placed for may now be
        // released.
        self.serving.store(true, SeqCst);
        self.reconciler.wake();
        loop {
            while connections.try_join_next().is_some() {}
            let framed = listener.accept().await.map_err(|error| {
                Error::new(Code::Transport, "The guest stopped taking connections.")
                    .with_source(error)
            })?;
            // One saying nothing yet is held only within the bound and the
            // opening time; past either it is closed, alone.
            let Ok(saying) = framing.clone().try_acquire_owned() else {
                continue;
            };
            let (machine, lanes, controls, pairings) = (
                self.clone(),
                lanes.clone(),
                controls.clone(),
                pairings.clone(),
            );
            connections.spawn(async move {
                let framed = tokio::time::timeout(OPENING, framed).await;
                drop(saying);
                let Ok(Ok((purpose, raw))) = framed else {
                    return;
                };
                // Room is taken before anything else, and a connection over
                // the bound is closed at once, before its handshake.
                match purpose {
                    Purpose::Control => {
                        if let Ok(_room) = controls.try_acquire_owned() {
                            let _ = machine.control(raw).await;
                        }
                    }
                    Purpose::Lane => {
                        if let Ok(_room) = lanes.try_acquire_owned() {
                            let _ = machine.lane(raw).await;
                        }
                    }
                    Purpose::Pairing => {
                        if let Ok(_room) = pairings.try_acquire_owned() {
                            let _ = machine.finish_pairing(raw).await;
                        }
                    }
                }
            });
        }
    }

    /// Finish, on the pairing connection `raw`, the pairing this guest
    /// committed, for a station whose acknowledgment was lost: never a new
    /// one, and never another station's.
    async fn finish_pairing(&self, raw: Raw) -> Result<()> {
        // Timed from the first: its records' read included.
        tokio::time::timeout(OPENING, self.finishing_pairing(raw))
            .await
            .map_err(|_| Error::new(Code::Transport, "The pairing did not finish in time."))?
    }

    async fn finishing_pairing(&self, raw: Raw) -> Result<()> {
        let paired = &self.paired;
        // Its records' reads go on past any timeout: bounded while they run.
        let records: Arc<dyn atlas_session::pairing::GuestRecords> = Arc::new(bounded::Bounded {
            records: paired.pairing.clone(),
            permits: self.blocking.clone(),
        });
        let reading = records.clone();
        let committed = tokio::task::spawn_blocking(move || reading.committed())
            .await
            .map_err(|_| Error::new(Code::State, "The pairing could not be read."))??;
        if committed != Some((paired.station, paired.enrolment.clone())) {
            return Err(Error::new(
                Code::Identity,
                "This desktop is paired for another enrolment.",
            ));
        }
        let facts = atlas_session::pairing::GuestFacts {
            uid: nix::unistd::getuid().as_raw(),
            boot: atlas_core::process::boot()?,
        };
        let finishing = atlas_session::pairing::guest_pairs(
            raw,
            &paired.keys,
            &paired.desktop,
            &paired.enrolment,
            &facts,
            &records,
        );
        finishing.await?;
        Ok(())
    }

    /// Go live in a new generation on the control connection `raw`; the
    /// generation live before retires, replaced.
    async fn control(self: &Arc<Self>, raw: Raw) -> Result<()> {
        let paired = &self.paired;
        let handshake = connect_as_guest(
            raw,
            &paired.keys,
            paired.station,
            &paired.desktop,
            Purpose::Control,
        );
        let secure = tokio::time::timeout(OPENING, handshake)
            .await
            .map_err(|_| Error::new(Code::Transport, "Control did not open in time."))??;
        let (said, halting) = (self.said.clone(), self.halting.clone());
        // A generation lost, not replaced, is a full Stop: agents stay
        // stopped until the person resumes them.
        let hook: Hook = Box::new(move |generation, cause| {
            said.retired
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push((generation, cause));
            let why = match cause {
                Retired::Lapsed => Why::Lapsed,
                Retired::ControlLost => Why::ControlLost,
                Retired::Revoked => Why::Revoked,
                Retired::Replaced => return,
            };
            halting.halt(why, None);
        });
        // One replacement at a time, its number given under the same lock,
        // so an older generation never replaces a newer one.
        let mut live = self.live.lock().await;
        let generations = paired.generations.clone();
        let generation = tokio::task::spawn_blocking(move || generations.next())
            .await
            .map_err(|_| Error::new(Code::State, "The generation could not be given."))??;
        // Changes done and unsettled are named again at every hello, so a
        // settlement lost on the way is caught up; those whose work is
        // proven gone are named as ended first. Read before the lease
        // starts: slow storage never spends it.
        let (records, daemon) = (paired.changes.clone(), self.daemon.clone());
        let unresolved =
            tokio::task::spawn_blocking(move || input_unresolved(&daemon, &records, None))
                .await
                .unwrap_or_default();
        let released = input_released(&self.daemon, unresolved).await;
        let (records, daemon) = (paired.changes.clone(), self.daemon.clone());
        let unsettled = tokio::task::spawn_blocking(move || {
            end_proven(&daemon, &records, None, &released);
            records.unsettled()
        })
        .await
        .ok()
        .and_then(Result::ok)
        .unwrap_or_default();
        let authority = Authority::new(generation, self.clock.clone(), hook);
        let lanes = Arc::new(Lanes::new(authority.clone()));
        let outbox = Arc::new(Outbox::hello(generation, unsettled, self.declared()));
        let credit = Arc::new(Credit::default());
        let saying = outbox.clone();
        let (reading, writing) = tokio::io::split(secure);
        let mut tasks = Tasks::default();
        tasks
            .0
            .push(tokio::spawn(control::send(writing, outbox)).abort_handle());
        let mirror = self
            .mirrors
            .as_ref()
            .map(|mirrors| mirrors.generation(authority.clone(), saying.clone()));
        let sending = credit.clone();
        let guest = Hand {
            mirror: mirror.clone(),
            said: self.said.clone(),
            changes: paired.changes.clone(),
            halting: self.halting.clone(),
            authority: authority.clone(),
            outbox: saying.clone(),
        };
        let reader = (authority.clone(), lanes.clone());
        tasks.0.push(
            tokio::spawn(async move {
                let (authority, lanes) = reader;
                control::guest_reads(reading, &authority, &lanes, &credit, &guest).await;
            })
            .abort_handle(),
        );
        // A retired generation stops talking: a partition that heals never
        // brings it back to life for the station.
        let talking = tasks.0.clone();
        let ended = authority.clone();
        tasks.0.push(
            tokio::spawn(async move {
                ended.ended().await;
                for task in talking {
                    task.abort();
                }
            })
            .abort_handle(),
        );
        tasks
            .0
            .push(tokio::spawn(watchdog(authority.clone())).abort_handle());
        if let Some(input) = &self.daemon.input {
            input.live(authority.generation().0, authority.clone());
        }
        let replaced = live.replace(Live {
            authority,
            lanes,
            outbox: saying,
            credit: sending,
            mirror,
            _tasks: tasks,
        });
        if let Some(replaced) = replaced {
            replaced.authority.retire(Retired::Replaced);
        }
        Ok(())
    }

    /// Serve the one request on the lane `raw`, once it is admitted in the
    /// live generation and what it carries is what its header says. Its
    /// work ends when the lane is cancelled or the generation retires,
    /// whether or not its connection stays open.
    async fn lane(self: &Arc<Self>, raw: Raw) -> Result<()> {
        let paired = &self.paired;
        let opening = async {
            let mut secure = connect_as_guest(
                raw,
                &paired.keys,
                paired.station,
                &paired.desktop,
                Purpose::Lane,
            )
            .await?;
            let header = read_header(&mut secure).await?;
            Ok::<_, Error>((secure, header))
        };
        let (secure, header) = tokio::time::timeout(OPENING, opening)
            .await
            .map_err(|_| Error::new(Code::Transport, "A lane did not open in time."))??;
        let (authority, lanes, outbox, credit, mirror) = {
            let live = self.live.lock().await;
            let live = live
                .as_ref()
                .ok_or_else(|| Error::new(Code::Denied, "No generation is live."))?;
            (
                live.authority.clone(),
                live.lanes.clone(),
                live.outbox.clone(),
                live.credit.clone(),
                live.mirror.clone(),
            )
        };
        // A lane of another generation, or one cancelled, is refused here;
        // a refused lane is closed, which is all the station hears.
        // The Stops so far, taken before the lane is admitted: one after this
        // refuses its change and ends its command, a Resume between or not.
        let stops = self.daemon.halted.borrow().stops;
        lanes.admit(&header, Opener::Station)?;
        // Carried out here from before its intent is kept until its lane is
        // done: nothing proves its work gone meanwhile.
        let _carrying = match &header.kind {
            LaneKind::Change { change }
            | LaneKind::Stage { change }
            | LaneKind::Input {
                change: Some(change),
            } => Some(self.daemon.keepers.carry(change)),
            _ => None,
        };
        // Only the station's reads while the guest is open to more than
        // Atlas, or not yet seen closed.
        if !matches!(header.kind, LaneKind::Read)
            && let Some(why) = self.posture.now().await.held()
        {
            return Err(Error::new(Code::Platform, why));
        }
        let (mut reader, writer) = tokio::io::split(secure);
        // A mirror carries bytes both ways from the first: no request line.
        if let LaneKind::Mirror {
            connection,
            endpoint,
        } = header.kind
        {
            let Some(joined) = mirror.and_then(|mirror| mirror.join(connection, endpoint)) else {
                return Ok(());
            };
            let mut closing = joined.closing.clone();
            // Ended with its lane or its generation, or its endpoint's
            // incarnation; never by a Stop.
            tokio::select! {
                () = atlas_session::mirror::carry(
                    joined.stream,
                    reader,
                    writer,
                    header.lane,
                    &credit,
                    &outbox,
                ) => {}
                () = lanes.ended(header.lane) => {}
                _ = closing.wait_for(|closed| *closed) => {}
            }
            return Ok(());
        }
        let mut writer: Writer = Box::new(writer);
        let line = atlas_native::native::endpoint::read_line(&mut reader, MAX_REQUEST).await;
        let Ok(mut line) = line else {
            return Ok(());
        };
        // Its own hello: never the hidden login's facts or capabilities.
        if matches!(header.kind, LaneKind::Read)
            && matches!(
                protocol::parse(&line),
                Ok(protocol::Asked {
                    request: Request::Hello,
                    ..
                })
            )
        {
            // Its account's facts may wait on the directory service: off the
            // workers the generation's watchdog runs on.
            let (manager, declared) = (self.daemon.manager.clone(), self.declared());
            let service = self.daemon.input.clone();
            let build = self.build.clone();
            let (daemon, jobs, kept) = (
                self.daemon.clone(),
                self.jobs.clone(),
                self.paired.stop.clone(),
            );
            let saying = bounded::off_runtime(&self.blocking, move || {
                let posture = READ_POSTURE();
                let work = work(&daemon, jobs.as_deref());
                let stop = super::super::facts::Stop {
                    kept: kept.kept().ok(),
                    halted: daemon.halted.borrow().halted,
                };
                // Read last, after what may have taken a while, and whole.
                let (standing, failure) = service
                    .as_ref()
                    .map(|service| service.snapshot())
                    .map_or((None, None), |(standing, failure)| {
                        (Some(standing), failure)
                    });
                let input = super::super::facts::Input {
                    standing: standing.map(|standing| match standing {
                        crate::machine::input::Standing::Starting => "starting",
                        crate::machine::input::Standing::Ready => "ready",
                        crate::machine::input::Standing::Gone => "gone",
                        crate::machine::input::Standing::Stopping => "stopping",
                        crate::machine::input::Standing::CleanupUnknown => "cleanupUnknown",
                    }),
                    failure,
                };
                let itself = super::super::facts::Itself::now(build);
                let hello = super::super::facts::machine_hello(
                    &manager, &declared, &itself, posture, input, &stop, work,
                );
                (hello, posture)
            });
            // Ended with its lane, as every request is, however long it waits.
            let hello = tokio::select! {
                said = saying => match said {
                    Some((hello, posture)) => {
                        self.posture.keep(posture);
                        hello
                    }
                    None => failure(Code::Busy, "The guest could not say hello now"),
                },
                () = ended(&lanes, header.lane) => return Ok(()),
            };
            return write(&mut writer, &hello).await;
        }
        // What a change came to is asked once the station lost its answer:
        // one whose outcome is unknown is answered as ended when its work is
        // proven gone by now.
        if matches!(header.kind, LaneKind::Read)
            && let Ok(protocol::Asked {
                request: Request::Outcome { change },
                ..
            }) = protocol::parse(&line)
        {
            let (daemon, changes, blocking) = (
                self.daemon.clone(),
                paired.changes.clone(),
                self.blocking.clone(),
            );
            let judging = async move {
                let (looked, records, asked) = (daemon.clone(), changes.clone(), change.clone());
                let unresolved = bounded::off_runtime(&blocking, move || {
                    input_unresolved(&looked, &records, Some(&asked))
                })
                .await
                .unwrap_or_default();
                let released = input_released(&daemon, unresolved).await;
                bounded::off_runtime(&blocking, move || {
                    end_proven(&daemon, &changes, Some(&change), &released);
                })
                .await
            };
            tokio::select! {
                _ = judging => {}
                () = ended(&lanes, header.lane) => return Ok(()),
            }
        }
        // Passed to the input service as it is, once judged here.
        if let LaneKind::Input { change } = &header.kind {
            let gate = Gate {
                authority,
                lanes: lanes.clone(),
                lane: header.lane,
                halted: self.daemon.halted.subscribe(),
                stops,
                held: true,
            };
            let mut hanging_up = [0; 1];
            let reply = tokio::select! {
                reply = self.input_lane(change.as_ref(), &line, &gate) => reply,
                () = ended(&lanes, header.lane) => return Ok(()),
                // The station sends nothing after its request: anything more,
                // or its end, is its hanging up, which ends the request too.
                _ = reader.read(&mut hanging_up) => return Ok(()),
            };
            return write(&mut writer, &reply).await;
        }
        // Bytes follow its request: they are the stage's own to read.
        if let LaneKind::Stage { change } = &header.kind {
            let gate = Gate {
                authority,
                lanes: lanes.clone(),
                lane: header.lane,
                halted: self.daemon.halted.subscribe(),
                stops,
                held: true,
            };
            let staged = stage::Lane {
                change: change.clone(),
                id: header.lane,
                gate,
                outbox,
            };
            return self.stage_lane(staged, &line, reader, writer).await;
        }
        if let Err(refusal) = bound(&header, &line) {
            return write(&mut writer, &refusal).await;
        }
        // An update is kept by its own lane, which answers one asked before
        // from its record: it outlives the daemon that asked the installer.
        if let LaneKind::Change { change } = &header.kind
            && let Ok(protocol::Asked {
                request: Request::Update { staged },
                ..
            }) = protocol::parse(&line)
            && let Ok(request) = serde_json::from_slice::<Value>(&line)
        {
            let holding = update::Holding {
                authority: authority.clone(),
                lanes: lanes.clone(),
                lane: header.lane,
            };
            let reply = self.update(change.clone(), staged, request, holding).await;
            return write(&mut writer, &reply).await;
        }
        // A change is kept as intended before it runs; one asked before is
        // answered from what it came to, never run again.
        if let LaneKind::Change { change } = &header.kind
            && let Some(answered) = self.intended(change, &line).await
        {
            return write(&mut writer, &answered).await;
        }
        // A copy is removed by the machine's own code, which keeps what it
        // came to and owns its work to the end.
        if let LaneKind::Change { change } = &header.kind
            && let Ok(protocol::Asked {
                request: Request::RemoveCopy { copy, node },
                ..
            }) = protocol::parse(&line)
        {
            let gate = Gate {
                authority,
                lanes: lanes.clone(),
                lane: header.lane,
                halted: self.daemon.halted.subscribe(),
                stops,
                held: true,
            };
            return self
                .remove_copy_lane(change.clone(), copy, node, gate, reader, writer)
                .await;
        }
        // Background jobs are the machine's own too: started and stopped as
        // changes it keeps, and read from what it keeps of them.
        if let Ok(protocol::Asked {
            request: Request::Job(job),
            ..
        }) = protocol::parse(&line)
        {
            let gate = Gate {
                authority,
                lanes: lanes.clone(),
                lane: header.lane,
                halted: self.daemon.halted.subscribe(),
                stops,
                held: true,
            };
            return self.job_lane(&header.kind, job, gate, writer).await;
        }
        line.push(b'\n');
        let reader: Reader = Box::new(std::io::Cursor::new(line).chain(reader));
        let asker = Asker {
            controller: true,
            host: None,
            gate: Some(Gate {
                authority,
                lanes: lanes.clone(),
                lane: header.lane,
                halted: self.daemon.halted.subscribe(),
                stops,
                held: true,
            }),
        };
        let phase = watch::channel(ConnectionPhase::Refusal).0;
        // Dropping the answer cancels whatever it runs, as a client that
        // hung up does.
        tokio::select! {
            served = answer(reader, writer, self.daemon.clone(), asker, phase) => served,
            () = ended(&lanes, header.lane) => Ok(()),
        }
    }
}

impl Machine {
    /// Keep `change`, asked for with the request `line`, as intended: none
    /// when it is new and may run now, else the reply to give instead.
    async fn intended(&self, change: &atlas_core::files::Token, line: &[u8]) -> Option<Value> {
        // One that is not a request at all is refused as anywhere, with no
        // intent left behind that nothing could finish.
        protocol::parse(line).ok()?;
        let request: Value = serde_json::from_slice(line).ok()?;
        self.intend(change, request).await
    }

    /// Keep `change`, asked for with `request`, as intended: none when it is
    /// new and may run now, else the reply to give instead.
    async fn intend(&self, change: &atlas_core::files::Token, request: Value) -> Option<Value> {
        let operation = request["op"].as_str().unwrap_or_default().to_owned();
        let (changes, change) = (self.paired.changes.clone(), change.clone());
        let asked = tokio::task::spawn_blocking(move || changes.ask(&change, &operation, &request))
            .await
            .map_err(|_| Error::new(Code::State, "The change records could not be read."))
            .and_then(|asked| asked);
        match asked {
            Ok(Asked::Begun) => None,
            Ok(Asked::Again(State::Done { reply })) => Some(reply),
            Ok(Asked::Again(State::Intended)) => Some(atlas_core::protocol::interrupted(
                Code::Uncertain,
                "This change was asked for before, and what it came to is unknown",
            )),
            // Refused for certain only when nothing could have run: the same
            // ID with another request, or a new one past the bound. Records
            // that cannot be read leave what the change came to unknown.
            Err(error) if matches!(error.code, Code::BadRequest | Code::Busy) => {
                Some(atlas_core::protocol::refused(error))
            }
            Err(error) => Some(serde_json::json!(error.uncertain())),
        }
    }
}

impl Machine {
    /// Pass the request `line` on an input lane to the input service, once
    /// it is what the lane was opened for: a read names no change, a change
    /// names the lane's, is admitted by `gate` right before, and is kept as
    /// intended before it goes and with what it came to before its reply.
    async fn input_lane(
        &self,
        lane: Option<&atlas_core::files::Token>,
        line: &[u8],
        gate: &Gate,
    ) -> Value {
        use atlas_native::native::service::Kind;
        let Some(input) = self.daemon.input.as_deref() else {
            return failure(Code::Platform, "This desktop has no input service");
        };
        let change = match (atlas_native::native::service::classify(line), lane) {
            (Some(Kind::Read), None) => None,
            (Some(Kind::Mutation(Some(asked))), Some(lane)) if asked == *lane => Some(asked),
            (Some(Kind::Mutation(None)), _) => {
                return failure(Code::BadRequest, "A change to input names its change");
            }
            _ => {
                return failure(
                    Code::BadRequest,
                    "The request is not what its lane was opened for",
                );
            }
        };
        // The owner a change to input acts for is carried out here from
        // before its record is kept until its lane is done: nothing proves
        // it let go of meanwhile.
        let (mut owner, mut operation, mut _owning) = (None, None, None);
        if let Some(change) = &change {
            let Ok(request) = serde_json::from_slice::<Value>(line) else {
                return failure(Code::BadRequest, "The request could not be read");
            };
            owner = request["owner"]
                .as_str()
                .and_then(atlas_core::files::Token::parse);
            operation = request["op"].as_str().map(str::to_owned);
            _owning = owner.as_ref().map(|owner| self.daemon.keepers.carry(owner));
            // One asked before is answered from what it came to, Stop or
            // not: only new work is admitted.
            if let Some(answered) = self.intend(change, request).await {
                return answered;
            }
            // Admitted once its intent is kept: a Stop before or meanwhile
            // ends it here, before it goes, and it is kept as refused.
            if let Some(refusal) = gate.refused() {
                return self.came_to(change.clone(), refusal).await;
            }
            // Input is taken only by an owner kept first, whose record says
            // when it let go of it.
            if operation.as_deref() == Some("acquire")
                && let Some(refusal) = self.owner_kept(owner.as_ref()).await
            {
                return self.came_to(change.clone(), refusal).await;
            }
        }
        // A Stop, or the generation ending, while it runs ends it too.
        let reply = tokio::select! {
            reply = forward(input, gate.authority.generation().0, line) => reply,
            () = gate.over() => Err(Error::new(
                Code::Stopped,
                "Agents were stopped while the change ran",
            )
            .uncertain()),
        };
        let Some(change) = change else {
            return reply.unwrap_or_else(|error| failure(error.code, error.error));
        };
        // Sent: what it came to is unknown unless the service said.
        let reply = reply.unwrap_or_else(|error| serde_json::json!(error.uncertain()));
        // The service acknowledged its owner's cleanup, or refused it input
        // for certain, before any was given: its record says so, and the
        // station's settlement then removes it.
        let refused = reply["ok"] == false && reply["uncertain"] != true;
        let ended = match operation.as_deref() {
            Some("release") if reply["ok"] == true && reply["released"] == true => {
                Some(reply.clone())
            }
            Some("acquire") if refused => Some(serde_json::json!(
                {"ok": true, "op": OWNER, "taken": false}
            )),
            _ => None,
        };
        if let (Some(ended), Some(owner)) = (ended, owner) {
            self.let_go(owner, &ended).await;
        }
        self.came_to(change, reply).await
    }

    /// Keep `owner` before it takes input, as a change of its own whose
    /// outcome stays unknown until its input service acknowledges it let go:
    /// none when it is kept, else the refusal of its acquire. One that let
    /// go already never takes input again.
    async fn owner_kept(&self, owner: Option<&atlas_core::files::Token>) -> Option<Value> {
        let Some(owner) = owner.cloned() else {
            return Some(failure(
                Code::BadRequest,
                "Input is taken by an owner named by a token",
            ));
        };
        let changes = self.paired.changes.clone();
        let request = serde_json::json!({"op": OWNER, "owner": owner});
        let kept = tokio::task::spawn_blocking(move || changes.ask(&owner, OWNER, &request))
            .await
            .map_err(|_| Error::new(Code::State, "The change records could not be read."))
            .and_then(|asked| asked);
        match kept {
            Ok(Asked::Begun | Asked::Again(State::Intended)) => None,
            Ok(Asked::Again(State::Done { .. })) => Some(failure(
                Code::BadRequest,
                "This owner let go of input already; input is taken by a new one",
            )),
            Err(error) => Some(serde_json::json!(error.certain())),
        }
    }

    /// Keep `owner` as let go of, with the `reply` its input service gave
    /// its release, while its record still waits for that. Best effort: one
    /// left waiting is ended once the service is proven to hold nothing.
    async fn let_go(&self, owner: atlas_core::files::Token, reply: &Value) {
        let (changes, reply) = (self.paired.changes.clone(), reply.clone());
        let _ = tokio::task::spawn_blocking(move || {
            if matches!(changes.of(&owner), Ok(Some(State::Intended))) {
                let _ = changes.done(&owner, &reply);
            }
        })
        .await;
    }

    /// Keep `reply` as what `change` came to, before it is given: one that
    /// cannot be kept goes as uncertain.
    async fn came_to(&self, change: atlas_core::files::Token, reply: Value) -> Value {
        let (changes, kept) = (self.paired.changes.clone(), reply.clone());
        let done = tokio::task::spawn_blocking(move || changes.done(&change, &kept))
            .await
            .is_ok_and(|done| done.is_ok());
        if done {
            reply
        } else {
            serde_json::json!(
                Error::new(
                    Code::Uncertain,
                    "What this change came to could not be kept"
                )
                .uncertain()
            )
        }
    }
}

/// Send `line` to the input service for `generation`, and read its one
/// reply.
///
/// # Errors
///
/// Fails when the service is not running, or ends without replying.
async fn forward(
    input: &crate::machine::input::Input,
    generation: u64,
    line: &[u8],
) -> Result<Value> {
    let mut opened = input.open(generation)?;
    let mut sent = line.to_vec();
    sent.push(b'\n');
    opened.send(&sent).await?;
    let unanswered =
        || Error::new(Code::Native, "The input service ended without answering").uncertain();
    // Exactly one line, whole and within bounds, that is a reply.
    let mut said = Vec::new();
    while !said.contains(&b'\n') {
        let bytes = opened.said.recv().await.ok_or_else(unanswered)?;
        said.extend(bytes);
        if said.len() > MAX_REQUEST {
            return Err(unanswered());
        }
    }
    let line = said.strip_suffix(b"\n").ok_or_else(unanswered)?;
    if line.contains(&b'\n') {
        return Err(unanswered());
    }
    let reply: Value = serde_json::from_slice(line).map_err(|_| unanswered())?;
    if !reply.get("ok").is_some_and(Value::is_boolean) {
        return Err(unanswered());
    }
    Ok(reply)
}

/// Completes once a lane's work must end at once: the lane cancelled, or
/// its generation replaced by a newer one. A generation lost instead is a
/// full Stop, which ends the work itself once the Stop is saved.
async fn ended(lanes: &Lanes, lane: LaneId) {
    lanes.ended(lane).await;
    if lanes.cancelled(lane) || lanes.retired() == Some(Retired::Replaced) {
        return;
    }
    // Its own cancel still ends it while the Stop is saved.
    lanes.until_cancelled(lane).await;
}

/// Retire `authority`'s generation the moment its lease lapses, checked on
/// its own, with no request, heartbeat or other check needed.
async fn watchdog(authority: Authority) {
    let mut ticks = tokio::time::interval(WATCHDOG);
    loop {
        ticks.tick().await;
        if authority.check().is_err() {
            return;
        }
    }
}

/// Whether the request `line` is what the lane `header` names: a read on a
/// lane that reads, and a change on the lane of that very change. Pictures,
/// screenshots and files are refused until their lanes are paced by the
/// session's bulk credit, which keeps them from crowding out control. A
/// request that does not parse is left for the daemon to refuse as it would
/// anywhere.
fn bound(header: &Header, line: &[u8]) -> std::result::Result<(), Value> {
    let Ok(asked) = protocol::parse(line) else {
        return Ok(());
    };
    // What sends a lot: pictures, screenshots and files.
    let bulk = matches!(
        asked.request,
        Request::Stream { .. }
            | Request::Frame { .. }
            | Request::Screenshot { .. }
            | Request::Read(_)
    );
    let fits = match (&header.kind, asked.request.policy().phase) {
        (LaneKind::Read, ConnectionPhase::Read(_)) => !bulk,
        (LaneKind::Change { change }, ConnectionPhase::Mutation(_)) => {
            asked.change.as_ref() == Some(change)
        }
        _ => false,
    };
    if fits {
        Ok(())
    } else {
        Err(failure(
            Code::BadRequest,
            "The request is not what its lane was opened for",
        ))
    }
}

mod bounded;
pub mod entry;
mod jobs;
mod listen;
mod stage;
mod update;

#[cfg(test)]
#[path = "machine_tests.rs"]
mod tests;

//! A desktop that shares nothing with the station, for tests: its own keys,
//! clock, generations and records, reached only across a [`link::Link`]
//! whose conditions a test sets. It is the start of the conformance kit
//! every kind of desktop passes.
pub mod link;
pub mod serve;

use link::Tasks;

use crate::{
    authority::{Authority, Hook, Retired},
    clock::Clock,
    control::{self, Guest, Outbox, Said, Station},
    credit::Credit,
    generation::{Counted, Generation, Generations},
    keys::Keypair,
    lane::{LaneId, LaneKind, Lanes},
    secure::{Purpose, connect_as_guest, connect_as_station},
};
use atlas_core::{desktop::DesktopId, error::Result, session::key::PeerKey};
use std::{
    sync::{
        Arc, Mutex, PoisonError,
        atomic::{AtomicUsize, Ordering::SeqCst},
    },
    time::Duration,
};
use tokio::io::{AsyncRead, AsyncWrite, BufReader};

/// A clock that runs with the runtime's, `skew` ahead of the station's.
#[derive(Debug)]
pub struct Skewed {
    start: tokio::time::Instant,
    skew: Duration,
}

impl Skewed {
    pub fn ahead_by(skew: Duration) -> Self {
        Self {
            start: tokio::time::Instant::now(),
            skew,
        }
    }
}

impl Clock for Skewed {
    fn now(&self) -> Option<Duration> {
        Some(self.skew + self.start.elapsed())
    }
}

/// The stand-in desktop.
#[derive(Debug)]
pub struct StandIn {
    pub keys: Keypair,
    pub desktop: DesktopId,
    clock: Arc<Skewed>,
    generations: Counted,
    /// What the station said, as the desktop's work heard it.
    pub stops: AtomicUsize,
    pub releases: AtomicUsize,
    pub resumes: AtomicUsize,
    pub dials: Mutex<Vec<LaneId>>,
    /// The changes the station settled.
    pub settled: Mutex<Vec<atlas_core::files::Token>>,
    /// Every generation that retired, and why.
    pub retired: Arc<Mutex<Vec<(Generation, Retired)>>>,
    /// The generation live now: a newer one retires it.
    current: Mutex<Option<Authority>>,
    /// Dial work runs on a task of its own, never in the control reader,
    /// and waits for this gate: a test holds it shut to keep a dial stuck.
    pub dial_gate: Arc<tokio::sync::Semaphore>,
    /// How many dials began their work.
    pub dials_begun: Arc<AtomicUsize>,
    /// The dials' work, ended when the stand-in goes.
    dial_work: Tasks,
}

/// A generation the stand-in is live in. Dropping it ends its control
/// connection's tasks, as the desktop going would.
#[derive(Debug)]
pub struct Live {
    pub authority: Authority,
    pub lanes: Arc<Lanes>,
    pub outbox: Arc<Outbox>,
    pub credit: Arc<Credit>,
    tasks: Tasks,
}

impl StandIn {
    /// A stand-in for `desktop` whose clock is `skew` ahead of the
    /// station's.
    ///
    /// # Panics
    ///
    /// When no key can be made.
    pub fn new(desktop: DesktopId, skew: Duration) -> Arc<Self> {
        Arc::new(Self {
            keys: Keypair::generate().expect("a key"),
            desktop,
            clock: Arc::new(Skewed::ahead_by(skew)),
            generations: Counted::default(),
            stops: AtomicUsize::new(0),
            releases: AtomicUsize::new(0),
            resumes: AtomicUsize::new(0),
            dials: Mutex::new(Vec::new()),
            settled: Mutex::new(Vec::new()),
            retired: Arc::new(Mutex::new(Vec::new())),
            current: Mutex::new(None),
            dial_gate: Arc::new(tokio::sync::Semaphore::new(
                tokio::sync::Semaphore::MAX_PERMITS,
            )),
            dials_begun: Arc::new(AtomicUsize::new(0)),
            dial_work: Tasks::default(),
        })
    }

    /// Dial the station whose key is `station` for control on `stream`, and
    /// go live in a new generation, its hello first; the generation live
    /// before retires, replaced.
    ///
    /// # Errors
    ///
    /// Fails when the connection does.
    pub async fn control<S>(self: &Arc<Self>, stream: S, station: PeerKey) -> Result<Live>
    where
        S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    {
        let secure =
            connect_as_guest(stream, &self.keys, station, &self.desktop, Purpose::Control).await?;
        let retired = self.retired.clone();
        let hook: Hook = Box::new(move |generation, cause| {
            retired
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push((generation, cause));
        });
        // One replacement at a time: the number given, the generation
        // before retired and this one made live, all under one lock, so a
        // newer generation is never replaced by an older one.
        let authority = {
            let mut current = self.current.lock().unwrap_or_else(PoisonError::into_inner);
            let generation = self.generations.next()?;
            let authority = Authority::new(generation, self.clock.clone(), hook);
            if let Some(replaced) = current.replace(authority.clone()) {
                replaced.retire(Retired::Replaced);
            }
            authority
        };
        let generation = authority.generation();
        let live = Live {
            lanes: Arc::new(Lanes::new(authority.clone())),
            authority,
            outbox: Arc::new(Outbox::hello(
                generation,
                Vec::new(),
                crate::compatibility::Declared::ours(Vec::new()),
            )),
            credit: Arc::new(Credit::default()),
            tasks: Tasks::default(),
        };
        let (reading, writing) = tokio::io::split(secure);
        live.tasks
            .hold(tokio::spawn(control::send(writing, live.outbox.clone())).abort_handle());
        let guest = self.clone();
        let (authority, lanes, credit) = (
            live.authority.clone(),
            live.lanes.clone(),
            live.credit.clone(),
        );
        live.tasks.hold(
            tokio::spawn(async move {
                control::guest_reads(reading, &authority, &lanes, &credit, &*guest).await;
            })
            .abort_handle(),
        );
        Ok(live)
    }
}

impl Guest for StandIn {
    fn stop(&self) {
        self.stops.fetch_add(1, SeqCst);
    }

    fn release(&self) {
        self.releases.fetch_add(1, SeqCst);
    }

    fn resume(&self) {
        self.resumes.fetch_add(1, SeqCst);
    }

    fn settled(&self, changes: Vec<atlas_core::files::Token>) {
        self.settled
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .extend(changes);
    }

    fn dial(&self, lane: LaneId, _: LaneKind) {
        self.dials
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(lane);
        let (gate, begun) = (self.dial_gate.clone(), self.dials_begun.clone());
        let work = tokio::spawn(async move {
            begun.fetch_add(1, SeqCst);
            let _ = gate.acquire().await;
        });
        self.dial_work.hold(work.abort_handle());
    }
}

/// What the station hears of the stand-in's control connection.
#[derive(Debug, Default)]
pub struct Heard {
    pub silent: AtomicUsize,
    pub again: AtomicUsize,
}

impl Station for Heard {
    fn silent(&self) {
        self.silent.fetch_add(1, SeqCst);
    }

    fn heard_again(&self) {
        self.again.fetch_add(1, SeqCst);
    }
}

/// The station's end of a control connection. Dropping it ends the
/// connection's tasks, as the station going would.
#[derive(Debug)]
pub struct Controlling {
    pub generation: Generation,
    /// What the guest's hello said it keeps done and unsettled.
    pub unsettled: Vec<atlas_core::files::Token>,
    /// What it declared it speaks and serves, judged per operation.
    pub declared: crate::compatibility::Declared,
    pub outbox: Arc<Outbox>,
    pub credit: Arc<Credit>,
    pub heard: Arc<Heard>,
    pub reading: tokio::task::JoinHandle<Result<()>>,
    /// Held only to end the connection's tasks when it goes.
    _tasks: Tasks,
}

/// Answer, as the station with key pair `own`, a control connection from
/// the desktop whose key is `desktop_key` on `stream`.
///
/// # Errors
///
/// Fails when the connection does, or its first message is not a hello.
pub async fn station_control<S>(
    stream: S,
    own: &Keypair,
    desktop_key: PeerKey,
    desktop: &DesktopId,
) -> Result<Controlling>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let secure = connect_as_station(stream, own, desktop_key, desktop, Purpose::Control).await?;
    let (reading, writing) = tokio::io::split(secure);
    let mut reading = BufReader::new(reading);
    // A guest of another protocol is refused here, before anything is said
    // to it: no generation of it is ever live for the station.
    let control::Hello {
        generation,
        unsettled,
        declared,
    } = control::hello(&mut reading).await?;
    let outbox = Arc::new(Outbox::default());
    let tasks = Tasks::default();
    tasks.hold(tokio::spawn(control::send(writing, outbox.clone())).abort_handle());
    let (credit, heard) = (Arc::new(Credit::default()), Arc::new(Heard::default()));
    let reading = {
        let (credit, heard) = (credit.clone(), heard.clone());
        tokio::spawn(async move { control::station_reads(reading, &credit, &*heard).await })
    };
    tasks.hold(reading.abort_handle());
    Ok(Controlling {
        generation,
        unsettled,
        declared,
        outbox,
        credit,
        heard,
        reading,
        _tasks: tasks,
    })
}

/// Say `said` from the station.
///
/// # Panics
///
/// When the outbox is closed.
pub fn say(controlling: &Controlling, said: Said) {
    controlling.outbox.say(said).expect("the outbox is open");
}

#[cfg(test)]
#[path = "standin_tests.rs"]
mod tests;

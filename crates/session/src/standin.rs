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
    control::{self, Guest, Outbox, Outcome, Said},
    credit::Credit,
    generation::{Counted, Generation, Generations},
    keys::Keypair,
    lane::{LaneId, LaneKind, Lanes},
    secure::{Purpose, connect_as_guest},
    station::control::{Control, Event},
};
use atlas_core::{desktop::DesktopId, error::Result, session::key::PeerKey};
use std::{
    sync::{
        Arc, Mutex, PoisonError,
        atomic::{AtomicUsize, Ordering::SeqCst},
    },
    time::Duration,
};
use tokio::io::{AsyncRead, AsyncWrite};

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

/// What a test runs as the stand-in carries out a numbered Resume.
#[derive(Default)]
pub struct OnLift(Mutex<Option<Box<dyn Fn() + Send + Sync>>>);

impl OnLift {
    /// Run `hook` as each numbered Resume to come is carried out.
    pub fn set(&self, hook: impl Fn() + Send + Sync + 'static) {
        *self.0.lock().unwrap_or_else(PoisonError::into_inner) = Some(Box::new(hook));
    }

    fn run(&self) {
        if let Some(hook) = &*self.0.lock().unwrap_or_else(PoisonError::into_inner) {
            hook();
        }
    }
}

impl std::fmt::Debug for OnLift {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("OnLift(..)")
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
    /// How many numbered Stops to come it answers as not kept.
    pub unkept: AtomicUsize,
    /// Run as each numbered Resume is carried out, before it is answered.
    pub on_lift: OnLift,
    /// Whether it is an older guest, declaring no numbered Stops and Resumes.
    pub plain: std::sync::atomic::AtomicBool,
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
    /// Its sending, which a test ends to make it fall silent one way.
    sending: Mutex<Option<tokio::task::AbortHandle>>,
    tasks: Tasks,
}

impl Live {
    /// Say nothing more to the station, its heartbeats included, while still
    /// hearing it: a link silent one way.
    pub fn fall_silent(&self) {
        if let Some(sending) = self
            .sending
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
        {
            sending.abort();
        }
    }
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
            unkept: AtomicUsize::new(0),
            on_lift: OnLift::default(),
            plain: std::sync::atomic::AtomicBool::new(false),
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
                // Its Stop report is a test's own lanes' to give.
                crate::compatibility::Declared::ours(if self.plain.load(SeqCst) {
                    vec![crate::compatibility::STOP_REPORT.to_owned()]
                } else {
                    vec![
                        crate::compatibility::ACKNOWLEDGED.to_owned(),
                        crate::compatibility::STOP_REPORT.to_owned(),
                    ]
                }),
            )),
            credit: Arc::new(Credit::default()),
            sending: Mutex::new(None),
            tasks: Tasks::default(),
        };
        let (reading, writing) = tokio::io::split(secure);
        let sending = tokio::spawn(control::send(writing, live.outbox.clone())).abort_handle();
        *live.sending.lock().unwrap_or_else(PoisonError::into_inner) = Some(sending.clone());
        live.tasks.hold(sending);
        let guest = Handler {
            standin: self.clone(),
            authority: live.authority.clone(),
            outbox: live.outbox.clone(),
        };
        let (authority, lanes, credit) = (
            live.authority.clone(),
            live.lanes.clone(),
            live.credit.clone(),
        );
        live.tasks.hold(
            tokio::spawn(async move {
                control::guest_reads(reading, &authority, &lanes, &credit, &guest).await;
            })
            .abort_handle(),
        );
        Ok(live)
    }
}

/// What one control connection's reader hands on: to the stand-in, and
/// each numbered Stop or Resume answered on this connection alone, while
/// its generation is live.
struct Handler {
    standin: Arc<StandIn>,
    authority: Authority,
    outbox: Arc<Outbox>,
}

impl Handler {
    /// Answer the Stop or Resume numbered `seq`: the stand-in carries each
    /// out at once.
    fn answer(&self, seq: u64, outcome: Outcome) {
        let _ = self
            .authority
            .if_live(|| self.outbox.say(Said::Applied { seq, outcome }));
    }
}

impl Guest for Handler {
    fn stop(&self) {
        self.standin.stops.fetch_add(1, SeqCst);
    }

    fn release(&self) {
        self.standin.releases.fetch_add(1, SeqCst);
    }

    fn resume(&self) {
        self.standin.resumes.fetch_add(1, SeqCst);
    }

    fn halt(&self, seq: u64) {
        self.standin.stops.fetch_add(1, SeqCst);
        let unkept = self
            .standin
            .unkept
            .fetch_update(SeqCst, SeqCst, |left| left.checked_sub(1))
            .is_ok();
        self.answer(
            seq,
            if unkept {
                Outcome::Refused {
                    why: crate::control::Refusal::Unkept,
                }
            } else {
                Outcome::Stopped
            },
        );
    }

    fn lift(&self, seq: u64) {
        self.standin.resumes.fetch_add(1, SeqCst);
        self.standin.on_lift.run();
        self.answer(seq, Outcome::Resumed);
    }

    fn settled(&self, changes: Vec<atlas_core::files::Token>) {
        self.standin
            .settled
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .extend(changes);
    }

    fn dial(&self, lane: LaneId, _: LaneKind) {
        let standin = &self.standin;
        standin
            .dials
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(lane);
        let (gate, begun) = (standin.dial_gate.clone(), standin.dials_begun.clone());
        let work = tokio::spawn(async move {
            begun.fetch_add(1, SeqCst);
            let _ = gate.acquire().await;
        });
        standin.dial_work.hold(work.abort_handle());
    }
}

/// What the station hears of the stand-in's control connection.
#[derive(Debug, Default)]
pub struct Heard {
    pub silent: AtomicUsize,
    pub again: AtomicUsize,
    /// Every answer to a mirror list, in order.
    pub taken: std::sync::Mutex<Vec<(u64, Vec<crate::mirror::Refused>)>>,
    /// Mirrored connections asking for their lanes, not yet taken.
    pub connects: std::sync::Mutex<std::collections::VecDeque<Connect>>,
    /// Told whenever a mirror message comes.
    pub mirrored: tokio::sync::Notify,
    /// Every answer to a numbered Stop or Resume, in the order it came.
    pub applied: std::sync::Mutex<Vec<(u64, Outcome)>>,
    /// Told whenever an answer comes.
    pub answered: tokio::sync::Notify,
}

/// A guest's request for a mirrored connection's lane.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Connect {
    pub connection: u64,
    pub endpoint: crate::mirror::Endpoint,
    pub list: u64,
}

impl Heard {
    /// The next connection asking for its lane, once one does.
    pub async fn connect(&self) -> Connect {
        loop {
            let mirrored = self.mirrored.notified();
            if let Some(connect) = self
                .connects
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .pop_front()
            {
                return connect;
            }
            mirrored.await;
        }
    }

    /// The answer to the Stop or Resume numbered `seq`, once it comes.
    pub async fn applied(&self, seq: u64) -> Outcome {
        loop {
            let answered = self.answered.notified();
            let found = self
                .applied
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .iter()
                .find(|(answer, _)| *answer == seq)
                .map(|(_, outcome)| *outcome);
            if let Some(outcome) = found {
                return outcome;
            }
            answered.await;
        }
    }

    /// The answer to list `list`, once it comes.
    pub async fn taken(&self, list: u64) -> Vec<crate::mirror::Refused> {
        loop {
            let mirrored = self.mirrored.notified();
            let found = self
                .taken
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .iter()
                .rev()
                .find(|(answered, _)| *answered == list)
                .map(|(_, refused)| refused.clone());
            if let Some(refused) = found {
                return refused;
            }
            mirrored.await;
        }
    }
}

impl Heard {
    /// Keep what the station heard as `event`; a connection lost is not
    /// kept here, but ends the reading.
    fn hear(&self, event: Event) {
        match event {
            Event::Silent => {
                self.silent.fetch_add(1, SeqCst);
            }
            Event::HeardAgain => {
                self.again.fetch_add(1, SeqCst);
            }
            Event::Taken { list, refused } => {
                self.taken
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .push((list, refused));
                self.mirrored.notify_waiters();
            }
            Event::Connect {
                connection,
                endpoint,
                list,
            } => {
                self.connects
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .push_back(Connect {
                        connection,
                        endpoint,
                        list,
                    });
                self.mirrored.notify_waiters();
            }
            Event::Applied { seq, outcome } => {
                self.applied
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .push((seq, outcome));
                self.answered.notify_waiters();
            }
            Event::Lost(_) => {}
        }
    }
}

/// The station's end of a control connection, as the station's own code
/// holds it, with what it heard kept for tests to read. Dropping it ends
/// the connection's tasks, as the station going would.
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
    /// Ends with why the connection was lost.
    pub reading: tokio::task::JoinHandle<Result<()>>,
    /// The station's own end, for its numbered Stops and Resumes.
    pub control: Control,
    /// Held only to end the connection's tasks when it goes.
    _tasks: Tasks,
}

/// Answer, as the station with key pair `own`, a control connection from
/// the desktop whose key is `desktop_key` on `stream`, through the
/// station's own code. Nothing the hello names is settled: a test settles
/// what it means to.
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
    let (control, mut events) =
        crate::station::control::open(stream, own, desktop_key, desktop, |_| true).await?;
    let heard = Arc::new(Heard::default());
    let reading = {
        let heard = heard.clone();
        tokio::spawn(async move {
            while let Some(event) = events.recv().await {
                if let Event::Lost(why) = event {
                    return Err(why);
                }
                heard.hear(event);
            }
            Ok(())
        })
    };
    let tasks = Tasks::default();
    tasks.hold(reading.abort_handle());
    Ok(Controlling {
        generation: control.generation(),
        unsettled: control.unsettled().to_vec(),
        declared: control.declared().clone(),
        outbox: control.outbox().clone(),
        credit: control.credit().clone(),
        heard,
        reading,
        control,
        _tasks: tasks,
    })
}

/// Say `said` from the station.
///
/// # Panics
///
/// When the outbox is closed.
pub fn say(controlling: &Controlling, said: Said) {
    controlling.control.say(said).expect("the outbox is open");
}

#[cfg(test)]
#[path = "standin_tests.rs"]
mod tests;

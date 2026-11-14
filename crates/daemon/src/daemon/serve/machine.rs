//! A machine desktop's daemon serving its station over their session: the
//! control connection, which gives each generation its number, and one
//! lane per request, admitted in the live generation and bound to what it
//! carries before anything is done.
use super::{Asker, Daemon, Gate, Reader, Writer, answer, write};
use crate::machine::{
    changes::{Asked, Changes, State},
    stop::{Kept as StopKept, Stop, Why},
    supervise::invocation::Supervisor,
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
    control::{self, Guest, Outbox},
    credit::Credit,
    generation::{Generation, Generations},
    keys::Keypair,
    lane::{Header, LaneId, LaneKind, Lanes, Opener, read_header},
    secure::{Purpose, connect_as_guest},
    station::Raw,
};
use serde_json::Value;
use std::{
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

/// What a connection the station made is for.
pub type Accepting<'a> = Pin<Box<dyn Future<Output = io::Result<(Purpose, Raw)>> + Send + 'a>>;

/// How the station reaches the guest: each connection it makes, and what
/// it is for.
pub trait Listener: Send + Sync {
    fn accept(&self) -> Accepting<'_>;
}

/// Who the guest is to the station it paired with.
pub struct Paired {
    pub keys: Keypair,
    pub desktop: DesktopId,
    pub station: PeerKey,
    pub generations: Arc<dyn Generations + Send + Sync>,
    /// What each change it was asked for came to, on its disk.
    pub changes: Arc<Changes<Arc<dyn Vault>>>,
    /// Whether agents are stopped here, on its disk.
    pub stop: Arc<Stop<Arc<dyn Vault>>>,
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
/// it came, so a Resume never undoes a Stop that came after it.
struct Halting {
    daemon: Arc<Daemon>,
    transitions: tokio::sync::mpsc::UnboundedSender<Transition>,
    /// Every Stop that came, saved or not yet: a Resume reopens nothing
    /// when one came after it.
    received: Arc<AtomicU64>,
    /// Held while a Stop is taken in or a Resume reopens: the two never
    /// cross. Never held across storage.
    turning: Arc<Held<()>>,
}

/// What the worker carries out.
enum Transition {
    Stop(Why),
    /// A Resume, from the generation it came in, after so many Stops came:
    /// lifted only while that generation is live and no Stop came since.
    Resume(Authority, u64),
}

impl Halting {
    /// Halting for `daemon`, its Stop kept in `stop`.
    fn new(daemon: Arc<Daemon>, stop: Arc<Stop<Arc<dyn Vault>>>) -> Arc<Self> {
        let (transitions, mut coming) = tokio::sync::mpsc::unbounded_channel();
        let worker = daemon.clone();
        let received = Arc::new(AtomicU64::new(0));
        let since = received.clone();
        let turning = Arc::new(Held::new(()));
        let turn = turning.clone();
        // The worker queues a Stop of its own without keeping its queue open.
        let again = transitions.downgrade();
        tokio::spawn(async move {
            // A Stop that could not be saved: what agents are is unknown,
            // and no Resume lifts it.
            let mut unsaved = false;
            while let Some(transition) = coming.recv().await {
                match transition {
                    Transition::Stop(why) => {
                        let saving = stop.clone();
                        let saved = tokio::task::spawn_blocking(move || saving.stop(why)).await;
                        unsaved |= !matches!(saved, Ok(Ok(())));
                        // Saved, or as saved as it can be: now its work ends.
                        worker.halted.send_modify(|halt| {
                            halt.stops += 1;
                            halt.halted = true;
                        });
                        if let Some(jobs) = &worker.jobs {
                            let reason = atlas_core::protocol::StopReason::Person;
                            let request = atlas_core::protocol::JobRequest::StopAll { reason };
                            let _ = jobs.serve(request).await;
                        }
                    }
                    Transition::Resume(authority, at) => {
                        let current = || authority.check().is_ok() && since.load(SeqCst) == at;
                        if unsaved || !current() {
                            continue;
                        }
                        let (lifting, checking) = (stop.clone(), authority.clone());
                        let lifted = tokio::task::spawn_blocking(move || {
                            // Checked again at the last moment before the disk.
                            checking.check()?;
                            lifting.resume()
                        })
                        .await;
                        if !matches!(lifted, Ok(Ok(()))) {
                            continue;
                        }
                        let turned = turn.lock().unwrap_or_else(PoisonError::into_inner);
                        if current() {
                            worker.halted.send_modify(|halt| halt.halted = false);
                            continue;
                        }
                        // Lifted on disk by a Resume its generation no longer
                        // gives: a full Stop of its own puts it back.
                        since.fetch_add(1, SeqCst);
                        worker.halted.send_modify(|halt| halt.halted = true);
                        drop(turned);
                        if let Some(again) = again.upgrade() {
                            let _ = again.send(Transition::Stop(Why::ControlLost));
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
        })
    }

    /// Stop agents for `why`: no change is taken from now on, and the
    /// worker saves the Stop, then ends the work it holds.
    fn halt(&self, why: Why) {
        let _taking = self.turning.lock().unwrap_or_else(PoisonError::into_inner);
        self.received.fetch_add(1, SeqCst);
        self.daemon.halted.send_modify(|halt| halt.halted = true);
        let _ = self.transitions.send(Transition::Stop(why));
    }

    /// The person's Resume, in `authority`'s generation.
    fn resume(&self, authority: Authority) {
        let at = self.received.load(SeqCst);
        let _ = self.transitions.send(Transition::Resume(authority, at));
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
}

impl Guest for Hand {
    fn stop(&self) {
        self.said.stops.fetch_add(1, SeqCst);
        self.halting.halt(Why::Person);
    }

    fn release(&self) {
        self.said.releases.fetch_add(1, SeqCst);
    }

    fn resume(&self) {
        self.halting.resume(self.authority.clone());
    }

    /// The station dials every lane itself: the guest is never asked to.
    fn dial(&self, _: LaneId, _: LaneKind) {}

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

/// The generation live now, and the tasks that keep it.
struct Live {
    authority: Authority,
    lanes: Arc<Lanes>,
    _tasks: Tasks,
}

/// A machine's daemon, serving the station it paired with.
pub struct Machine {
    daemon: Arc<Daemon>,
    paired: Paired,
    clock: Arc<dyn Clock>,
    live: Mutex<Option<Live>>,
    halting: Arc<Halting>,
    pub said: Arc<Said>,
}

impl Machine {
    pub fn new(
        mut daemon: Daemon,
        paired: Paired,
        clock: Arc<dyn Clock>,
        supervisor: Supervisor,
    ) -> Arc<Self> {
        daemon.kept = Some(paired.changes.clone());
        daemon.supervisor = Some(supervisor);
        // Stopped before it started, or unable to tell: still stopped.
        let clear = matches!(paired.stop.kept(), Ok(StopKept::Clear));
        daemon.halted.send_modify(|halt| halt.halted = !clear);
        let daemon = Arc::new(daemon);
        let halting = Halting::new(daemon.clone(), paired.stop.clone());
        Arc::new(Self {
            daemon,
            paired,
            clock,
            live: Mutex::new(None),
            halting,
            said: Arc::default(),
        })
    }

    /// Serve every connection `listener` gives, until it fails.
    ///
    /// # Errors
    ///
    /// Returns the listener's failure.
    pub async fn serve(self: Arc<Self>, listener: &dyn Listener) -> Result<()> {
        let (lanes, controls) = (
            Arc::new(Semaphore::new(LANES)),
            Arc::new(Semaphore::new(CONTROLS)),
        );
        let mut connections = JoinSet::new();
        loop {
            while connections.try_join_next().is_some() {}
            let (purpose, raw) = listener.accept().await.map_err(|error| {
                Error::new(Code::Transport, "The guest stopped taking connections.")
                    .with_source(error)
            })?;
            let machine = self.clone();
            // Room is taken before anything is spawned, and a connection
            // over the bound is closed at once, before its handshake.
            let room = match purpose {
                Purpose::Control => controls.clone().try_acquire_owned(),
                Purpose::Lane => lanes.clone().try_acquire_owned(),
                // Pairing has its own way in, before there is a station.
                Purpose::Pairing => continue,
            };
            let Ok(room) = room else {
                continue;
            };
            match purpose {
                Purpose::Control => {
                    connections.spawn(async move {
                        let _room = room;
                        let _ = machine.control(raw).await;
                    });
                }
                Purpose::Lane => {
                    connections.spawn(async move {
                        let _room = room;
                        let _ = machine.lane(raw).await;
                    });
                }
                Purpose::Pairing => {}
            }
        }
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
            halting.halt(why);
        });
        // One replacement at a time, its number given under the same lock,
        // so an older generation never replaces a newer one.
        let mut live = self.live.lock().await;
        let generations = paired.generations.clone();
        let generation = tokio::task::spawn_blocking(move || generations.next())
            .await
            .map_err(|_| Error::new(Code::State, "The generation could not be given."))??;
        // Changes done and unsettled are named again at every hello, so a
        // settlement lost on the way is caught up. Read before the lease
        // starts: slow storage never spends it.
        let records = paired.changes.clone();
        let unsettled = tokio::task::spawn_blocking(move || records.unsettled())
            .await
            .ok()
            .and_then(Result::ok)
            .unwrap_or_default();
        let authority = Authority::new(generation, self.clock.clone(), hook);
        let lanes = Arc::new(Lanes::new(authority.clone()));
        let outbox = Arc::new(Outbox::hello(generation, unsettled));
        let credit = Arc::new(Credit::default());
        let (reading, writing) = tokio::io::split(secure);
        let mut tasks = Tasks::default();
        tasks
            .0
            .push(tokio::spawn(control::send(writing, outbox)).abort_handle());
        let guest = Hand {
            said: self.said.clone(),
            changes: paired.changes.clone(),
            halting: self.halting.clone(),
            authority: authority.clone(),
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
        let replaced = live.replace(Live {
            authority,
            lanes,
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
        let (authority, lanes) = {
            let live = self.live.lock().await;
            let live = live
                .as_ref()
                .ok_or_else(|| Error::new(Code::Denied, "No generation is live."))?;
            (live.authority.clone(), live.lanes.clone())
        };
        // A lane of another generation, or one cancelled, is refused here;
        // a refused lane is closed, which is all the station hears.
        // The Stops so far, taken before the lane is admitted: one after this
        // refuses its change and ends its command, a Resume between or not.
        let stops = self.daemon.halted.borrow().stops;
        lanes.admit(&header, Opener::Station)?;
        let (mut reader, writer) = tokio::io::split(secure);
        let mut writer: Writer = Box::new(writer);
        let line = atlas_native::native::endpoint::read_line(&mut reader, MAX_REQUEST).await;
        let Ok(mut line) = line else {
            return Ok(());
        };
        if let Err(refusal) = bound(&header, &line) {
            return write(&mut writer, &refusal).await;
        }
        // A change is kept as intended before it runs; one asked before is
        // answered from what it came to, never run again.
        if let LaneKind::Change { change } = &header.kind
            && let Some(answered) = self.intended(change, &line).await
        {
            return write(&mut writer, &answered).await;
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

#[cfg(test)]
#[path = "machine_tests.rs"]
mod tests;

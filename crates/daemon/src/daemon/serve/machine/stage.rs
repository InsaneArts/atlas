//! A stage lane: the station's request, a manifest, then every file's
//! contents, written by a transaction that owns its work to the end. The
//! lane may go, be cancelled or time out; the transaction still cleans up
//! and keeps what it came to, and a retry is answered from that.
use super::{Gate, Machine, Writer, write};
use crate::machine::stage::{self, Placed, Stages, copies, write::Contents};
use atlas_core::{
    error::{Code, Error},
    files::Token,
    protocol::{failure, interrupted},
    stage::{MOST_CONTENTS, MOST_ENTRIES, MOST_MANIFEST, Snapshot},
};
use atlas_session::{
    control::{Outbox, Said},
    lane::LaneId,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    sync::{Arc, Mutex, PoisonError},
    time::Duration,
};
use tokio::{
    io::{AsyncRead, AsyncReadExt},
    sync::mpsc,
    time::Instant,
};

/// The least rate a stage's bytes must keep, after its grace.
const RATE: u64 = 256 * 1024;

/// How long a stage may take before its bytes count against [`RATE`].
const GRACE: Duration = Duration::from_secs(30);

/// How long a lane waits for its transaction's storage work, and a retry
/// for one still running, before it answers uncertain.
const SETTLING: Duration = Duration::from_secs(120);

/// The most read from the lane at once.
const CHUNK: usize = 64 * 1024;

/// Chunks on their way to the writer at once.
const QUEUED: usize = 8;

/// A stage lane: its change, its number, what admits it, and where its
/// credit goes back.
pub(super) struct Lane {
    pub(super) change: Token,
    pub(super) id: LaneId,
    pub(super) gate: Gate,
    pub(super) outbox: Arc<Outbox>,
}

/// The station's request for a stage.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Asked {
    op: String,
    change: Token,
    name: String,
    digest: String,
    entries: u64,
    size: u64,
    attributes: u64,
    manifest: u64,
    /// What it is staged for, when that is not an agent's own use.
    #[serde(default)]
    purpose: Option<Purpose>,
}

/// What a stage may be for besides an agent's own use.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
enum Purpose {
    /// The bundle of an update of the guest's Atlas: staged even while
    /// agents are stopped, which an update asks.
    Update,
}

/// Where a transaction stands: cancelling only stops one not yet
/// authorized to commit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Running,
    Committing,
    Ended,
}

#[derive(Debug)]
struct Going(Mutex<Phase>);

impl Going {
    fn phase(&self) -> std::sync::MutexGuard<'_, Phase> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn running(&self) -> bool {
        *self.phase() == Phase::Running
    }

    /// End it, if it is still running.
    fn end(&self) {
        let mut phase = self.phase();
        if *phase == Phase::Running {
            *phase = Phase::Ended;
        }
    }

    /// Move it to committing: whether it was still running.
    fn commit(&self) -> bool {
        let mut phase = self.phase();
        let running = *phase == Phase::Running;
        if running {
            *phase = Phase::Committing;
        }
        running
    }
}

/// What a transaction came to: kept before it is said, or, when its
/// cleanup failed, said as uncertain and left intended for the next start.
enum Came {
    To(Value),
    Pending(Value),
}

/// The bytes a transaction's writer takes, as the lane hands them over.
struct Receiving(mpsc::Receiver<Vec<u8>>);

impl Contents for Receiving {
    fn next_chunk(&mut self) -> Option<Vec<u8>> {
        self.0.blocking_recv()
    }
}

impl Machine {
    /// Serve the stage lane `lane`, whose request is `line`: bytes follow
    /// on `reader` once it is told to go ahead on `writer`.
    pub(super) async fn stage_lane<R>(
        self: &Arc<Self>,
        mut lane: Lane,
        line: &[u8],
        reader: R,
        mut writer: Writer,
    ) -> atlas_core::error::Result<()>
    where
        R: AsyncRead + Unpin + Send + 'static,
    {
        let Some(stages) = self.daemon.stages.clone() else {
            let refusal = failure(Code::Platform, "This desktop takes no stages");
            return write(&mut writer, &refusal).await;
        };
        if stages.root().is_none() {
            let refusal = failure(Code::State, "The desktop's folder for copies is not ready");
            return write(&mut writer, &refusal).await;
        }
        let (Ok(asked), Ok(request)) = (
            serde_json::from_slice::<Asked>(line),
            serde_json::from_slice::<Value>(line),
        ) else {
            let refusal = failure(Code::BadRequest, "The stage could not be read");
            return write(&mut writer, &refusal).await;
        };
        if asked.op != stage::OPERATION || asked.change != lane.change {
            let refusal = failure(Code::BadRequest, "The stage is not its lane's change");
            return write(&mut writer, &refusal).await;
        }
        // Only writes into the daemon's folder, which no agent reaches
        // while they are stopped: a Stop never holds an update's bundle.
        if asked.purpose == Some(Purpose::Update) {
            lane.gate.held = false;
        }
        let admitting = stages.admitting.lock().await;
        if let Some(mut done) = stages.running(&lane.change) {
            // Asked again while it runs: answered once it is done, all of it
            // within one bound, the records' own lookup included.
            drop(admitting);
            let answered = tokio::time::timeout(SETTLING, async {
                let _ = done.wait_for(|done| *done).await;
                self.intend(&lane.change, request).await
            })
            .await;
            let answer = answered.ok().flatten().unwrap_or_else(|| {
                interrupted(Code::Uncertain, "What this stage came to is unknown")
            });
            return write(&mut writer, &answer).await;
        }
        if let Some(answered) = self.intend(&lane.change, request.clone()).await {
            return write(&mut writer, &answered).await;
        }
        // Kept as intended from here: whatever it comes to is kept. A token
        // being removed is never staged into.
        if stages.removing(&lane.change) {
            drop(admitting);
            let busy = failure(Code::Busy, "The copy is being removed");
            let reply = self.came_to(lane.change.clone(), busy).await;
            return write(&mut writer, &reply).await;
        }
        let running = stages.register(&lane.change);
        drop(admitting);
        let (permit, reserved) = match begin(&stages, &lane, &asked) {
            Ok(held) => held,
            Err(refusal) => {
                let reply = self.came_to(lane.change.clone(), refusal).await;
                drop(running);
                return write(&mut writer, &reply).await;
            }
        };
        let rate = asked.size.saturating_add(asked.manifest) / RATE;
        let deadline = Instant::now() + GRACE + Duration::from_secs(rate);
        if write(&mut writer, &json!({"ready": true})).await.is_err() {
            let reply = stage::nothing_placed("the station hung up");
            let _ = self.came_to(lane.change.clone(), reply).await;
            return Ok(());
        }
        let (machine, lanes, id) = (self.clone(), lane.gate.lanes.clone(), lane.id);
        let mut transaction = tokio::spawn(async move {
            let _held = (running, permit, reserved);
            let change = lane.change.clone();
            let mut reader = reader;
            let (came, committing) = machine
                .transact(&stages, &lane, &asked, &request, &mut reader, deadline)
                .await;
            let reply = match came {
                Came::To(reply) => machine.came_to(change, reply).await,
                Came::Pending(reply) => reply,
            };
            // Counted as committing until what it came to is kept.
            drop(committing);
            reply
        });
        // The lane's end lets it go, and so does storage that takes too
        // long: the transaction still ends its own work and keeps what it
        // came to, for a retry to hear.
        tokio::select! {
            joined = &mut transaction => match joined {
                Ok(reply) => write(&mut writer, &reply).await,
                Err(_) => Ok(()),
            },
            () = lanes.ended(id) => Ok(()),
            () = tokio::time::sleep_until(deadline + SETTLING) => {
                let unknown = interrupted(
                    Code::Uncertain,
                    "The stage is still being finished; ask again for what it came to",
                );
                write(&mut writer, &unknown).await
            }
        }
    }

    /// Carry out the stage `asked` for, its bytes on `reader`, by
    /// `deadline`: what it came to, and, once authorized, its count as
    /// committing, held until that is kept.
    async fn transact<R>(
        &self,
        stages: &Arc<Stages>,
        lane: &Lane,
        asked: &Asked,
        request: &Value,
        reader: &mut R,
        deadline: Instant,
    ) -> (Came, Option<stage::Committing>)
    where
        R: AsyncRead + Unpin + Send,
    {
        let going = Arc::new(Going(Mutex::new(Phase::Running)));
        let stopping = stopping(lane, deadline);
        tokio::pin!(stopping);
        let manifest = tokio::select! {
            read = manifest(reader, asked, lane) => read,
            why = &mut stopping => Err(why),
        };
        let snapshot = match manifest {
            Ok(snapshot) => snapshot,
            Err(why) => return (Came::To(stage::nothing_placed(&why)), None),
        };
        let (sending, receiving) = mpsc::channel(QUEUED);
        let writer = {
            let (stages, change, request) = (stages.clone(), lane.change.clone(), request.clone());
            let going = going.clone();
            tokio::task::spawn_blocking(move || {
                let root = stages
                    .root()
                    .ok_or_else(|| Error::new(Code::State, "No root"))?;
                stage::write::write(
                    root,
                    &change,
                    &snapshot,
                    &request,
                    &mut Receiving(receiving),
                    &|| going.running(),
                )
            })
        };
        #[cfg(test)]
        let held = stages.clone();
        let writing = async move {
            let written = writer
                .await
                .map_err(|_| Error::new(Code::Io, "The stage's writer ended"))
                .and_then(|written| written);
            #[cfg(test)]
            held.held_before_commit().await;
            written
        };
        tokio::pin!(writing);
        let pumped = tokio::select! {
            pumped = pump(reader, asked.size, lane, &sending) => pumped,
            why = &mut stopping => Err(why),
        };
        drop(sending);
        // Its bytes are all in: from here until it is authorized, the
        // station hanging up or sending more ends it, as a cancel does.
        let mut more = [0_u8; 1];
        let ended = match pumped {
            Err(why) => Err(why),
            // Whatever ends it goes first: a writer done at the same moment
            // never wins over it.
            Ok(()) => tokio::select! {
                biased;
                why = &mut stopping => Err(why),
                read = reader.read(&mut more) => Err(match read {
                    Ok(0) | Err(_) => "the station hung up".to_owned(),
                    Ok(_) => "more bytes came than its files hold".to_owned(),
                }),
                written = &mut writing => Ok(written),
            },
        };
        let (ended, written) = match ended {
            Ok(written) => (None, written),
            Err(why) => {
                going.end();
                (Some(why), (&mut writing).await)
            }
        };
        let refusal = match (ended, written) {
            (Some(why), _) => stage::nothing_placed(&why),
            (None, Err(error)) => stage::nothing_placed(&error.error),
            (None, Ok(())) => {
                // Authorized under the turn a Stop is taken in, and counted
                // as committing under it, so a Resume waits for it.
                let committing = {
                    let _turn = self
                        .halting
                        .turning
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner);
                    match lane
                        .gate
                        .authorize(|| Instant::now() < deadline && going.commit())
                    {
                        None => Ok(stages.commit()),
                        Some(refusal) => Err(refusal),
                    }
                };
                match committing {
                    Ok(committing) => {
                        let came = self.commit(stages, lane, asked).await;
                        return (came, Some(committing));
                    }
                    Err(refusal) => refusal,
                }
            }
        };
        (discarded(stages, &lane.change, refusal).await, None)
    }

    /// Put the written stage in place, now that it is authorized.
    async fn commit(&self, stages: &Arc<Stages>, lane: &Lane, asked: &Asked) -> Came {
        let (placing, change) = (stages.clone(), lane.change.clone());
        let placed = tokio::task::spawn_blocking(move || {
            placing.root().map(|root| stage::place(root, &change))
        })
        .await;
        let Some(root) = stages.root() else {
            return Came::Pending(interrupted(Code::Uncertain, "The stage's folder went"));
        };
        match placed {
            Ok(Some(Placed::Done)) => {
                Came::To(stage::reply(root, &lane.change, &asked.name, &asked.digest))
            }
            Ok(Some(Placed::Unsynced)) => {
                let mut reply = interrupted(
                    Code::Uncertain,
                    "The copy was placed, but whether that lasts a crash is unknown",
                );
                reply["path"] = json!(root.placed(&lane.change, &asked.name));
                Came::To(reply)
            }
            Ok(Some(Placed::NotMoved(error))) => {
                discarded(stages, &lane.change, stage::nothing_placed(&error.error)).await
            }
            Ok(None) | Err(_) => Came::Pending(interrupted(
                Code::Uncertain,
                "What this stage came to is unknown until the desktop starts again",
            )),
        }
    }
}

/// Take what a new stage holds while it runs: a place among those running
/// at once, and room on the volume. The refusal when it may not start.
fn begin(
    stages: &Stages,
    lane: &Lane,
    asked: &Asked,
) -> Result<(tokio::sync::OwnedSemaphorePermit, stage::Reserved), Value> {
    if let Some(refusal) = lane.gate.refused() {
        return Err(refusal);
    }
    // An update's stage while agents are stopped: bounded, as nothing
    // staged then can be removed until they resume.
    let halt = *lane.gate.halted.borrow();
    if !lane.gate.held && halt.halted && !stages.take_stopped_update(halt.stops) {
        return Err(failure(
            Code::Busy,
            "Too many update bundles were staged while agents are stopped",
        ));
    }
    let bounded = asked.manifest <= MOST_MANIFEST as u64
        && asked.entries <= MOST_ENTRIES as u64
        && asked.size <= MOST_CONTENTS;
    if !bounded {
        return Err(failure(Code::BadRequest, "The stage is too large"));
    }
    let permit = stages
        .place_among()
        .ok_or_else(|| failure(Code::Busy, "Too many stages are running; try again"))?;
    let bytes = asked
        .size
        .saturating_add(asked.attributes)
        .saturating_add(asked.manifest);
    let reserved = stages
        .reserve(asked.entries, bytes)
        .ok_or_else(|| failure(Code::Busy, "The desktop has too little room for this"))?;
    Ok((permit, reserved))
}

/// Remove the stage's staging folder, then keep `refusal`: nothing was
/// placed and nothing is left. One that cannot be removed leaves the
/// change intended, put right at the next start.
async fn discarded(stages: &Arc<Stages>, change: &Token, refusal: Value) -> Came {
    let (stages, change) = (stages.clone(), change.clone());
    let cleared = tokio::task::spawn_blocking(move || {
        let root = stages.root()?;
        stage::write::discard(&root.folder, &stage::staging(&change)).ok()?;
        root.folder.sync_all().ok()
    })
    .await;
    if matches!(cleared, Ok(Some(()))) {
        Came::To(refusal)
    } else {
        Came::Pending(interrupted(
            Code::Uncertain,
            "What was staged could not be cleared away yet",
        ))
    }
}

/// Completes once the stage must end: its lane cancelled, agents stopped
/// or its generation over, or its deadline passed. Why.
async fn stopping(lane: &Lane, deadline: Instant) -> String {
    tokio::select! {
        () = lane.gate.lanes.until_cancelled(lane.id) => "it was cancelled".into(),
        () = lane.gate.over() => "agents were stopped".into(),
        () = tokio::time::sleep_until(deadline) => "it took too long".into(),
    }
}

/// Read the stage's manifest: its length, as the request said, then that
/// many bytes, every one credited back.
async fn manifest<R>(reader: &mut R, asked: &Asked, lane: &Lane) -> Result<Snapshot, String>
where
    R: AsyncRead + Unpin + Send,
{
    let hung_up = |_| "the station hung up".to_owned();
    let length = reader.read_u64().await.map_err(hung_up)?;
    credit(lane, 8);
    if length != asked.manifest {
        return Err("its manifest is not the length it said".into());
    }
    let length = usize::try_from(length).map_err(|_| "its manifest is too large".to_owned())?;
    let mut read = vec![0_u8; length];
    let mut at = 0;
    while at < length {
        let end = length.min(at + CHUNK);
        let got = reader.read(&mut read[at..end]).await.map_err(hung_up)?;
        if got == 0 {
            return Err("the station hung up".into());
        }
        credit(lane, got);
        at += got;
    }
    let snapshot: Snapshot =
        serde_json::from_slice(&read).map_err(|_| "its manifest could not be read".to_owned())?;
    // Checked before anything in it is added up.
    snapshot.check().map_err(|error| error.error)?;
    let matches = snapshot.name == asked.name
        && snapshot.digest == asked.digest
        && snapshot.entries.len() as u64 == asked.entries
        && snapshot.size() == asked.size
        && snapshot.attribute_bytes() == asked.attributes;
    if !matches {
        return Err("its manifest is not the one it asked for".into());
    }
    Ok(snapshot)
}

/// Hand `size` bytes from `reader` to the writer, every read credited.
async fn pump<R>(
    reader: &mut R,
    size: u64,
    lane: &Lane,
    sending: &mpsc::Sender<Vec<u8>>,
) -> Result<(), String>
where
    R: AsyncRead + Unpin + Send,
{
    let mut left = size;
    let mut buffer = vec![0_u8; CHUNK];
    while left > 0 {
        let room = usize::try_from(left).unwrap_or(CHUNK).min(CHUNK);
        let got = reader
            .read(&mut buffer[..room])
            .await
            .map_err(|_| "the station hung up".to_owned())?;
        if got == 0 {
            return Err("the station hung up".into());
        }
        credit(lane, got);
        left -= got as u64;
        if sending.send(buffer[..got].to_vec()).await.is_err() {
            // The writer stopped: what it came to says why.
            return Ok(());
        }
    }
    Ok(())
}

/// Give `bytes` of the lane's credit back to the station.
fn credit(lane: &Lane, bytes: usize) {
    let _ = lane.outbox.say(Said::Credit {
        lane: lane.id,
        bytes: bytes as u64,
    });
}

impl Machine {
    /// Remove the copy `copy`, still the folder `node` says, as the change
    /// `change`, already kept as intended, admitted by `gate`. The removal
    /// owns its work to the end, whatever happens to its lane.
    pub(super) async fn remove_copy_lane<R>(
        self: &Arc<Self>,
        change: Token,
        copy: Token,
        node: String,
        gate: Gate,
        mut reader: R,
        mut writer: Writer,
    ) -> atlas_core::error::Result<()>
    where
        R: AsyncRead + Unpin + Send + 'static,
    {
        let (lanes, id) = (gate.lanes.clone(), gate.lane);
        let Some(stages) = self.daemon.stages.clone() else {
            let refusal = failure(Code::Platform, "This desktop keeps no copies");
            let reply = self.came_to(change, refusal).await;
            return write(&mut writer, &reply).await;
        };
        if let Some(refusal) = gate.refused() {
            let reply = self.came_to(change, refusal).await;
            return write(&mut writer, &reply).await;
        }
        let claimed = {
            let _admitting = stages.admitting.lock().await;
            stages.place_among().zip(stages.claim(&copy))
        };
        let Some((permit, claimed)) = claimed else {
            let busy = failure(
                Code::Busy,
                "The copy is busy, or too many changes are running",
            );
            let reply = self.came_to(change, busy).await;
            return write(&mut writer, &reply).await;
        };
        // Counted as committing from here, under the turn a Stop is taken
        // in, until what it came to is kept: putting the copy right may
        // already finish an earlier removal, and a Resume waits for it all.
        let committing = {
            let _turn = self
                .halting
                .turning
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            // Admitted again under the turn: a Stop and Resume while it
            // waited for its place refuse it here.
            match gate.authorize(|| true) {
                None => Ok(stages.commit()),
                Some(refusal) => Err(refusal),
            }
        };
        let committing = match committing {
            Ok(committing) => committing,
            Err(refusal) => {
                let reply = self.came_to(change, refusal).await;
                return write(&mut writer, &reply).await;
            }
        };
        // The station hanging up, or sending anything more, ends it before
        // it is authorized.
        let hung_up = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let watching = tokio::spawn({
            let hung_up = hung_up.clone();
            async move {
                let mut more = [0_u8; 1];
                let _ = reader.read(&mut more).await;
                hung_up.store(true, std::sync::atomic::Ordering::SeqCst);
            }
        });
        let latched = hung_up.clone();
        let (machine, turning) = (self.clone(), self.halting.turning.clone());
        let mut removal = tokio::spawn(async move {
            let _held = (claimed, permit, committing);
            let changes = machine.paired.changes.clone();
            let (removing, asked, of) = (stages.clone(), change.clone(), copy.clone());
            let removed = tokio::task::spawn_blocking(move || {
                let root = removing.root()?;
                // What an earlier removal or stage of it left is put right
                // first: until it is, nothing new is done to it, and its
                // record is never this removal's to forget.
                if !copies::put_right(root, &changes, &of) {
                    let busy = failure(
                        Code::Busy,
                        "An earlier change to this copy is not yet put right",
                    );
                    return Some((copies::Removal::Came(busy), false));
                }
                // Authorized under the turn a Stop is taken in.
                let came = copies::remove(root, &asked, &of, &node, || {
                    let _turn = turning.lock().unwrap_or_else(PoisonError::into_inner);
                    gate.authorize(|| !hung_up.load(std::sync::atomic::Ordering::SeqCst))
                });
                Some((came, true))
            })
            .await;
            match removed {
                Ok(Some((copies::Removal::Came(reply), owned))) => {
                    let kept = machine.came_to(change, reply.clone()).await;
                    // Its own record goes only once what it came to is kept.
                    if owned && kept == reply {
                        let _ = tokio::task::spawn_blocking(move || {
                            stages.root().map(|root| copies::forget(root, &copy))
                        })
                        .await;
                    }
                    kept
                }
                Ok(Some((copies::Removal::Pending(reply), _))) => reply,
                Ok(None) | Err(_) => interrupted(
                    Code::Uncertain,
                    "What this removal came to is unknown until the desktop starts again",
                ),
            }
        });
        tokio::select! {
            joined = &mut removal => {
                // Done: the lane closes once its reply is written.
                watching.abort();
                match joined {
                    Ok(reply) => write(&mut writer, &reply).await,
                    Err(_) => Ok(()),
                }
            }
            () = lanes.ended(id) => {
                // Its lane gone, it may no longer be authorized.
                latched.store(true, std::sync::atomic::Ordering::SeqCst);
                watching.abort();
                Ok(())
            }
        }
    }
}

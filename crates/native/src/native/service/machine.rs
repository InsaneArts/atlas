//! The input service of a machine desktop: no socket of its own, only the
//! channel its daemon started it with. Every connection arrives over it,
//! each given a socket pair here so it is served as on the helper login;
//! the daemon's renewals, revocations and holds arrive over it too, ahead
//! of any data, and are answered over it. Demultiplexing never waits on a
//! connection: one whose queue is full is closed. Anything the channel says
//! that is not a frame, or its end, is loss, and loss ends the service with
//! agents held.
use super::{
    Service,
    channel::{Connection, Frame, Generation, Lost, PAYLOAD},
    drain_and_release,
};
use std::{
    collections::{BTreeSet, HashMap},
    sync::{
        Arc, Mutex as Shared, PoisonError,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{
        UnixStream,
        unix::{OwnedReadHalf, OwnedWriteHalf},
    },
    sync::{Notify, mpsc, oneshot},
};

/// Frames waiting to be written that say something to the daemon.
const CONTROL: usize = 64;
/// Frames of data waiting to be written, across every connection.
const DATA: usize = 256;
/// Data frames waiting for one connection before it is closed.
const PER_CONNECTION: usize = 64;
/// Connections handed to the service at once, waiting to be served.
const HANDED: usize = 16;
/// The longest one frame may take to write before the channel is lost.
const WRITING: Duration = Duration::from_secs(1);

/// A connection the daemon opened, for the service to serve.
pub(super) struct Handed {
    pub(super) stream: UnixStream,
    pub(super) generation: Generation,
    /// Completes when the daemon closes the connection: its handler ends.
    pub(super) closed: oneshot::Receiver<()>,
}

/// The channel, as the service's loop sees it: connections handed over,
/// and the end of it.
pub(super) struct Channel {
    pub(super) handed: mpsc::Receiver<Handed>,
    pub(super) lost: Arc<Notify>,
    settler: Settler,
}

/// What tells the daemon a revoked generation's work is over.
#[derive(Clone)]
pub(super) struct Settler {
    control: mpsc::Sender<Frame>,
    unsettled: Arc<Shared<BTreeSet<Generation>>>,
    lost: Arc<Notify>,
    /// Set while the service rebinds to a display that moved.
    rebinding: Arc<std::sync::atomic::AtomicBool>,
    /// Ticks since the display was last looked at.
    ticks: Arc<AtomicU64>,
}

impl Channel {
    /// Read and write `stream` for `service` on tasks of their own.
    pub(super) fn open(stream: UnixStream, service: Arc<Service>) -> Self {
        let (reading, writing) = stream.into_split();
        let (control, controls) = mpsc::channel(CONTROL);
        let (data, datas) = mpsc::channel(DATA);
        let (hand, handed) = mpsc::channel(HANDED);
        let lost = Arc::new(Notify::new());
        let unsettled = Arc::new(Shared::new(BTreeSet::new()));
        tokio::spawn(write(writing, controls, datas, lost.clone()));
        tokio::spawn(
            Reader {
                service,
                control: control.clone(),
                data,
                hand,
                lost: lost.clone(),
                open: Arc::default(),
                newest: None,
                unsettled: unsettled.clone(),
                held: Arc::new(AtomicU64::new(0)),
            }
            .read(reading),
        );
        Self {
            handed,
            lost: lost.clone(),
            settler: Settler {
                control,
                unsettled,
                lost,
                rebinding: Arc::default(),
                ticks: Arc::default(),
            },
        }
    }

    /// What settles revoked generations, for the service's loop.
    pub(super) fn settler(&self) -> Settler {
        self.settler.clone()
    }
}

impl Settler {
    /// About once a second, when nobody has control and the session's display
    /// moved, bind to the display it shows now, releasing what was held on
    /// the one before: on the helper login the controller asks for this; a
    /// machine desktop's service does it itself, agents held meanwhile by the
    /// gate, which gives no control while it runs.
    pub(super) fn follow_display(&self, service: &Arc<Service>) {
        if !self.ticks.fetch_add(1, Ordering::SeqCst).is_multiple_of(20)
            || service.gate.active().unwrap_or(true)
        {
            return;
        }
        let Ok(current) = crate::native::session_info().and_then(|info| {
            atlas_core::session::InputEpoch::from_proof(&info, std::process::id())
        }) else {
            return;
        };
        // Again while the last recovery failed, though the display is the
        // one bound now.
        let moved = service.epoch().is_ok_and(|bound| bound != current);
        if !(moved || service.operations.failed())
            || !crate::native::accessibility_trusted()
            || self.rebinding.swap(true, Ordering::SeqCst)
        {
            return;
        }
        let (service, rebinding) = (service.clone(), self.rebinding.clone());
        tokio::spawn(async move {
            let _ = service.rebind_to(current).await;
            rebinding.store(false, Ordering::SeqCst);
        });
    }

    /// Tell the daemon each revoked generation whose work is over: nothing
    /// of it is owned any more, and nothing it started is still being
    /// carried out or released.
    pub(super) fn settle(&self, service: &Service) {
        let mut unsettled = self
            .unsettled
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let Some(&newest) = unsettled.last() else {
            return;
        };
        if service.gate.owns_through(newest).unwrap_or(true) {
            return;
        }
        // The engine is held through every event and every release: free and
        // with nothing held, everything earlier is over.
        let Ok(engine) = service.engine.clone().try_lock_owned() else {
            return;
        };
        if engine.unsettled() {
            return;
        }
        for generation in std::mem::take(&mut *unsettled) {
            if self.control.try_send(Frame::Settled(generation)).is_err() {
                self.lost.notify_one();
                return;
            }
        }
    }
}

/// Write frames from `controls` and `datas` to `writing`, control first,
/// each within [`WRITING`]; the channel is lost when one cannot be.
async fn write(
    mut writing: OwnedWriteHalf,
    mut controls: mpsc::Receiver<Frame>,
    mut datas: mpsc::Receiver<Frame>,
    lost: Arc<Notify>,
) {
    loop {
        let frame = tokio::select! {
            biased;
            Some(frame) = controls.recv() => frame,
            Some(frame) = datas.recv() => frame,
            else => return,
        };
        let written = tokio::time::timeout(WRITING, frame.write(&mut writing)).await;
        if !matches!(written, Ok(Ok(()))) {
            lost.notify_one();
            return;
        }
    }
}

/// One connection the daemon opened, as the reader keeps it.
struct Open {
    /// Data for it, written to its socket in order.
    inbound: mpsc::Sender<Vec<u8>>,
    /// Ends its handler when sent, or dropped.
    closing: oneshot::Sender<()>,
}

/// The reader of the channel, and what it keeps of the connections.
struct Reader {
    service: Arc<Service>,
    control: mpsc::Sender<Frame>,
    data: mpsc::Sender<Frame>,
    hand: mpsc::Sender<Handed>,
    lost: Arc<Notify>,
    /// Shared with each connection's forwarding, which takes its own out
    /// once it ends.
    open: Arc<Shared<HashMap<Connection, Open>>>,
    /// The newest connection the daemon opened.
    newest: Option<Connection>,
    unsettled: Arc<Shared<BTreeSet<Generation>>>,
    /// The newest word on the hold the service applied.
    held: Arc<AtomicU64>,
}

impl Reader {
    async fn read(mut self, mut reading: OwnedReadHalf) {
        while let Ok(frame) = Frame::read(&mut reading).await {
            if self.take(frame).is_err() {
                break;
            }
        }
        self.lost.notify_one();
    }

    /// Act on one frame from the daemon, never waiting on a connection.
    fn take(&mut self, frame: Frame) -> Result<(), Lost> {
        match frame {
            Frame::Open(connection, generation) => self.open(connection, generation),
            Frame::Data(connection, bytes) => {
                if !self.opened(connection) {
                    return Err(Lost);
                }
                // Closed already, its last frames still coming: let go.
                let full = {
                    let open = self.open.lock().unwrap_or_else(PoisonError::into_inner);
                    let Some(open) = open.get(&connection) else {
                        return Ok(());
                    };
                    open.inbound.try_send(bytes).is_err()
                };
                if full {
                    // Too far behind: that connection ends.
                    self.close(connection);
                }
                Ok(())
            }
            Frame::Close(connection) => {
                if !self.opened(connection) {
                    return Err(Lost);
                }
                self.close(connection);
                Ok(())
            }
            Frame::Renew(generation, deadline) => self
                .service
                .gate
                .renew_authority(generation, deadline)
                .map_err(|_| Lost),
            Frame::Revoke(generation) => {
                self.service
                    .gate
                    .revoke_authority(generation)
                    .map_err(|_| Lost)?;
                self.unsettled
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .insert(generation);
                self.say(Frame::Revoked(generation))
            }
            Frame::Hold { seq, on } => self.hold(seq, on),
            Frame::Revoked(_) | Frame::Settled(_) | Frame::Held { .. } => Err(Lost),
        }
    }

    fn open(&mut self, connection: Connection, generation: Generation) -> Result<(), Lost> {
        // The daemon numbers connections from one up, each the next, so
        // every number up to the newest was opened.
        if connection != self.newest.map_or(1, |newest| newest.saturating_add(1)) {
            return Err(Lost);
        }
        self.newest = Some(connection);
        let Ok((ours, theirs)) = UnixStream::pair() else {
            return self
                .data
                .try_send(Frame::Close(connection))
                .map_err(|_| Lost);
        };
        let (closing, closed) = oneshot::channel();
        let handed = Handed {
            stream: theirs,
            generation,
            closed,
        };
        if self.hand.try_send(handed).is_err() {
            // Too many waiting to be served: refused as it came.
            return self
                .data
                .try_send(Frame::Close(connection))
                .map_err(|_| Lost);
        }
        let (inbound, inbounds) = mpsc::channel(PER_CONNECTION);
        let (outward, inward) = ours.into_split();
        tokio::spawn(deliver(inward, inbounds));
        self.open
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(connection, Open { inbound, closing });
        tokio::spawn(forward(
            outward,
            connection,
            self.data.clone(),
            self.open.clone(),
        ));
        Ok(())
    }

    /// Whether the daemon ever opened `connection`: numbered from one up,
    /// every number up to the newest was.
    fn opened(&self, connection: Connection) -> bool {
        connection >= 1 && self.newest.is_some_and(|newest| connection <= newest)
    }

    fn close(&mut self, connection: Connection) {
        let open = self
            .open
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&connection);
        if let Some(open) = open {
            let _ = open.closing.send(());
        }
    }

    /// Apply the daemon's `seq`-th word on the hold, unless a newer one was
    /// applied already: the gate first, then what it held is released, then
    /// it is acknowledged.
    fn hold(&mut self, seq: u64, on: bool) -> Result<(), Lost> {
        if self.held.fetch_max(seq, Ordering::SeqCst) >= seq {
            return Ok(());
        }
        self.service.gate.hold(on).map_err(|_| Lost)?;
        if !on {
            return self.say(Frame::Held { seq, on });
        }
        let (service, control, held, lost) = (
            self.service.clone(),
            self.control.clone(),
            self.held.clone(),
            self.lost.clone(),
        );
        tokio::spawn(async move {
            // Tried again until it is released, or a newer word came.
            while held.load(Ordering::SeqCst) == seq {
                let released =
                    drain_and_release(&service.operations, &service.engine, &service.gate).await;
                if released.is_ok() {
                    if held.load(Ordering::SeqCst) == seq
                        && control.send(Frame::Held { seq, on }).await.is_err()
                    {
                        lost.notify_one();
                    }
                    return;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        });
        Ok(())
    }

    fn say(&self, frame: Frame) -> Result<(), Lost> {
        self.control.try_send(frame).map_err(|_| Lost)
    }
}

/// Write what the daemon sends for one connection into its socket, then
/// end its writing.
async fn deliver(mut inward: OwnedWriteHalf, mut inbounds: mpsc::Receiver<Vec<u8>>) {
    while let Some(bytes) = inbounds.recv().await {
        if inward.write_all(&bytes).await.is_err() {
            return;
        }
    }
    let _ = inward.shutdown().await;
}

/// Send what one connection writes to the daemon, then its end, and let
/// the connection go.
async fn forward(
    mut outward: OwnedReadHalf,
    connection: Connection,
    data: mpsc::Sender<Frame>,
    open: Arc<Shared<HashMap<Connection, Open>>>,
) {
    let mut buffer = vec![0; PAYLOAD];
    loop {
        match outward.read(&mut buffer).await {
            Ok(0) | Err(_) => break,
            Ok(count) => {
                let frame = Frame::Data(connection, buffer[..count].to_vec());
                if data.send(frame).await.is_err() {
                    return;
                }
            }
        }
    }
    open.lock()
        .unwrap_or_else(PoisonError::into_inner)
        .remove(&connection);
    let _ = data.send(Frame::Close(connection)).await;
}

#[cfg(test)]
#[path = "machine_tests.rs"]
mod tests;

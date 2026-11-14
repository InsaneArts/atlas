//! A link between a station and a desktop that tests set the conditions
//! of: every connection between the two shares it, as connections to one
//! machine share its one network, so bytes on one queue behind bytes on
//! another, in both directions. It can add delay, limit bandwidth, fall
//! silent without closing anything (a partition), and cut connections.
//!
//! The link holds at most its buffer's worth of bytes waiting to cross
//! ([`BUFFERED`] unless a test says otherwise), and each connection at most
//! [`UNREAD`] bytes that crossed and were not read yet: a writer whose bytes
//! cannot cross, or whose reader reads nothing, waits, as one would on a
//! full network, without keeping any other connection from crossing.
//! Dropping the last handle to the link ends every task it runs and every
//! connection across it.
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex, PoisonError},
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt, DuplexStream, ReadHalf, WriteHalf, duplex},
    sync::{OwnedSemaphorePermit, Semaphore, mpsc, watch},
    task::AbortHandle,
    time::Instant,
};

/// The most read from a connection at once.
const PIECE: usize = 16 * 1024;
/// The most bytes waiting to cross the link at once, unless a test says.
pub const BUFFERED: usize = 256 * 1024;
/// The most bytes one connection holds on their way or unread, unless a
/// test says: less than the link's buffer, so a connection nobody reads
/// never fills it.
pub const UNREAD: usize = 64 * 1024;

/// What the link does to what crosses it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Conditions {
    /// How long every piece takes to arrive once it left.
    pub delay: Duration,
    /// How many bytes leave each second; none for no limit.
    pub bytes_per_second: Option<u64>,
    /// Nothing leaves and nothing arrives, and nothing closes either.
    pub partitioned: bool,
}

/// Where a piece goes once it crossed, with when it is due.
type Arrivals = mpsc::UnboundedSender<(Instant, Piece)>;

/// A piece on its way: its share of the link's buffer until it crosses,
/// and of its connection's until it is delivered.
struct Piece {
    bytes: Vec<u8>,
    to: Arrivals,
    crossing: Option<OwnedSemaphorePermit>,
    _unread: OwnedSemaphorePermit,
}

/// The shared link.
#[derive(Clone, Debug)]
pub struct Link {
    inner: Arc<Inner>,
    tasks: Arc<Tasks>,
}

struct Inner {
    conditions: watch::Sender<Conditions>,
    /// Pieces waiting to leave, in the order they came.
    waiting: Mutex<VecDeque<Piece>>,
    arrived: tokio::sync::Notify,
    room: Arc<Semaphore>,
    size: usize,
    unread: usize,
}

impl std::fmt::Debug for Inner {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Inner")
            .field("conditions", &*self.conditions.borrow())
            .finish_non_exhaustive()
    }
}

/// Tasks ended when their owner goes.
#[derive(Debug, Default)]
pub struct Tasks(Mutex<Vec<AbortHandle>>);

impl Tasks {
    /// End `task` when these tasks end.
    pub fn hold(&self, task: AbortHandle) {
        lock(&self.0).push(task);
    }
}

impl Drop for Tasks {
    fn drop(&mut self) {
        for task in lock(&self.0).drain(..) {
            task.abort();
        }
    }
}

/// One connection across the link.
#[derive(Debug)]
pub struct Connection(Vec<AbortHandle>);

impl Connection {
    /// Close this connection alone, both ends.
    pub fn cut(&self) {
        for end in &self.0 {
            end.abort();
        }
    }
}

impl Link {
    /// A link under `conditions`, crossing on a task of its own.
    pub fn new(conditions: Conditions) -> Self {
        Self::buffering(conditions, BUFFERED, UNREAD)
    }

    /// A link under `conditions` holding at most `size` bytes waiting to
    /// cross, and each connection at most `unread` on their way or unread;
    /// each is at least a byte. A piece is never larger than either.
    pub fn buffering(conditions: Conditions, size: usize, unread: usize) -> Self {
        let (size, unread) = (size.max(1), unread.max(1));
        let link = Self {
            inner: Arc::new(Inner {
                conditions: watch::Sender::new(conditions),
                waiting: Mutex::new(VecDeque::new()),
                arrived: tokio::sync::Notify::new(),
                room: Arc::new(Semaphore::new(size)),
                size,
                unread,
            }),
            tasks: Arc::new(Tasks::default()),
        };
        let crossing = link.inner.clone();
        link.tasks
            .hold(tokio::spawn(async move { crossing.cross().await }).abort_handle());
        link
    }

    /// Change what the link does from now on, to what is on its way too.
    pub fn set(&self, conditions: Conditions) {
        self.inner.conditions.send_replace(conditions);
        self.inner.arrived.notify_one();
    }

    /// How many bytes wait to cross now.
    pub fn buffered(&self) -> usize {
        self.inner.size - self.inner.room.available_permits()
    }

    /// A new connection across the link: the station's end, the
    /// desktop's, and a way to cut it alone.
    pub fn connect(&self) -> (DuplexStream, DuplexStream, Connection) {
        let (station, station_far) = duplex(PIECE);
        let (desktop_far, desktop) = duplex(PIECE);
        let (station_read, station_write) = tokio::io::split(station_far);
        let (desktop_read, desktop_write) = tokio::io::split(desktop_far);
        let (to_desktop, delivering_desktop) = self.deliver(desktop_write);
        let (to_station, delivering_station) = self.deliver(station_write);
        let connection = Connection(vec![
            delivering_desktop,
            delivering_station,
            self.pump(station_read, to_desktop),
            self.pump(desktop_read, to_station),
        ]);
        for task in &connection.0 {
            self.tasks.hold(task.clone());
        }
        (station, desktop, connection)
    }

    /// Close every connection across the link, both ends.
    pub fn cut(&self) {
        // The crossing task is the first held; every later one is an end.
        let mut tasks = lock(&self.tasks.0);
        for end in tasks.drain(1..) {
            end.abort();
        }
    }

    /// Read what one end sends and queue it to cross, waiting for room.
    fn pump(&self, mut from: ReadHalf<DuplexStream>, to: Arrivals) -> AbortHandle {
        let inner = self.inner.clone();
        // This connection's own room at the far end.
        let unread = Arc::new(Semaphore::new(self.inner.unread));
        let largest = PIECE.min(self.inner.size).min(self.inner.unread);
        tokio::spawn(async move {
            let mut read = vec![0; largest];
            loop {
                let Ok(count) = from.read(&mut read).await else {
                    return;
                };
                if count == 0 {
                    return;
                }
                let bytes = u32::try_from(count).unwrap_or(u32::MAX);
                let Ok(unread) = unread.clone().acquire_many_owned(bytes).await else {
                    return;
                };
                let Ok(crossing) = inner.room.clone().acquire_many_owned(bytes).await else {
                    return;
                };
                lock(&inner.waiting).push_back(Piece {
                    bytes: read[..count].to_vec(),
                    to: to.clone(),
                    crossing: Some(crossing),
                    _unread: unread,
                });
                inner.arrived.notify_one();
            }
        })
        .abort_handle()
    }

    /// Hand what crossed to the other end once it is due and the link is
    /// not partitioned, in order.
    fn deliver(&self, mut to: WriteHalf<DuplexStream>) -> (Arrivals, AbortHandle) {
        let (arriving, mut arrivals) = mpsc::unbounded_channel::<(Instant, Piece)>();
        let mut conditions = self.inner.conditions.subscribe();
        let delivering = tokio::spawn(async move {
            while let Some((due, piece)) = arrivals.recv().await {
                tokio::time::sleep_until(due).await;
                let mut at = 0;
                while at < piece.bytes.len() {
                    if conditions
                        .wait_for(|conditions| !conditions.partitioned)
                        .await
                        .is_err()
                    {
                        return;
                    }
                    // A write waiting on a full reader gives way to a
                    // partition that comes meanwhile; what it wrote stays.
                    let written = tokio::select! {
                        biased;
                        _ = conditions.wait_for(|conditions| conditions.partitioned) => continue,
                        written = to.write(&piece.bytes[at..]) => written,
                    };
                    match written {
                        Ok(0) | Err(_) => return,
                        Ok(count) => at += count,
                    }
                }
            }
            let _ = to.shutdown().await;
        });
        (arriving, delivering.abort_handle())
    }
}

impl Inner {
    /// Let pieces leave one at a time, as fast as the bandwidth allows and
    /// never while partitioned, a partition that comes while one is leaving
    /// holding it back too.
    async fn cross(self: Arc<Self>) {
        let mut conditions = self.conditions.subscribe();
        loop {
            if conditions
                .wait_for(|conditions| !conditions.partitioned)
                .await
                .is_err()
            {
                return;
            }
            let arrived = self.arrived.notified();
            let Some(piece) = lock(&self.waiting).pop_front() else {
                arrived.await;
                continue;
            };
            let rate = conditions.borrow().bytes_per_second;
            if let Some(rate) = rate.filter(|rate| *rate > 0) {
                let takes = Duration::from_secs_f64(piece.bytes.len() as f64 / rate as f64);
                tokio::time::sleep(takes).await;
            }
            // Partitioned while it was leaving: it waits for the link.
            if conditions
                .wait_for(|conditions| !conditions.partitioned)
                .await
                .is_err()
            {
                return;
            }
            let delay = conditions.borrow().delay;
            let to = piece.to.clone();
            // Crossed: its place in the link's buffer is free.
            let mut piece = piece;
            drop(piece.crossing.take());
            let _ = to.send((Instant::now() + delay, piece));
        }
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

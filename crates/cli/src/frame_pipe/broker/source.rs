//! The broker's one source of pictures. Only it reads the helper's live
//! stream, one acquisition at a time, on demand, keeping just the latest
//! picture; observers that stop waiting never cancel an acquisition half
//! way, and a source left without demand gives the helper's stream back.
//! A screen that stays still is read less often, until it changes or input
//! is sent; every picture is still read anew and timed when it was read.
use atlas_client::client::{Client, live::LiveStream};
use atlas_control::Visible;
use atlas_core::{
    frame_pipe::{
        budget::Budget,
        diagnostics::{Reason, Ring},
        model::{Measure, Positive, Source},
        pixels::FramePixels,
    },
    protocol::CaptureBackend,
    session::Epoch,
};
use std::{
    future::Future,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::{Notify, watch};

/// How long a source may go without demand before it gives the stream back.
pub(crate) const IDLE: Duration = Duration::from_secs(2);
/// The shortest time between two acquisitions: 30 a second at most.
const CADENCE: Duration = Duration::from_millis(33);
/// The time between acquisitions while the screen stays still. Every picture
/// is copied and sent whole, so reading a still screen less often saves the
/// helper and the broker CPU.
const STILL_CADENCE: Duration = Duration::from_millis(250);
/// How many pictures in a row must match the one before for the screen to
/// count as still.
const STILL_AFTER: u32 = 10;

/// One verified picture of the helper, shared by every observer it serves.
pub(crate) struct Picture {
    pub(crate) pixels: FramePixels,
    pub(crate) sequence: Positive,
    /// When its acquisition passed the readiness checkpoint.
    pub(crate) at: Instant,
    pub(crate) display: [Measure; 3],
    pub(crate) backend: Source,
    /// The helper session both its proofs named.
    pub(crate) epoch: Epoch,
    /// The screen's privacy generation through the whole acquisition, or
    /// None when it was hidden or a sign-in began or ended meanwhile: such
    /// a picture is shown to the app alone.
    pub(crate) visible: Option<Visible>,
}

impl Picture {
    /// Whether `other` shows exactly what this one does, in the same session.
    fn same_as(&self, other: &Self) -> bool {
        self.epoch == other.epoch
            && self.display == other.display
            && self.pixels.width() == other.pixels.width()
            && self.pixels.height() == other.pixels.height()
            && self.pixels.as_slice() == other.pixels.as_slice()
    }
}

/// How long the screen has looked the same, by the source's own pictures.
#[derive(Default)]
struct Stillness {
    /// The newest picture, also held by the latest answer. It is let go
    /// once that answer is a failure or the stream was given back.
    last: Option<Arc<Picture>>,
    /// How many pictures in a row matched the one before.
    repeats: u32,
}

impl Stillness {
    /// Note `picture`, the newest.
    fn saw(&mut self, picture: &Arc<Picture>) {
        let same = self.last.as_ref().is_some_and(|last| last.same_as(picture));
        self.repeats = if same {
            self.repeats.saturating_add(1)
        } else {
            0
        };
        self.last = Some(picture.clone());
    }

    /// Input was sent: the screen may change at any moment.
    fn stirred(&mut self) {
        self.repeats = 0;
    }

    /// The time from one acquisition's start to the next one's.
    fn cadence(&self) -> Duration {
        if self.repeats >= STILL_AFTER {
            STILL_CADENCE
        } else {
            CADENCE
        }
    }
}

/// What reads pictures for the source.
pub(crate) trait Acquire: Send + 'static {
    /// The next verified picture, its pixels reserved from `budget`.
    fn acquire(&mut self, budget: &Budget) -> impl Future<Output = Result<Picture, Reason>> + Send;

    /// Give the helper's stream back, once it confirms it drained.
    fn stop(&mut self) -> impl Future<Output = Result<(), Reason>> + Send;
}

/// What the source last made of a demand.
#[derive(Clone)]
pub(crate) enum Latest {
    Nothing,
    Picture(Arc<Picture>),
    Failed(Reason),
    /// The stream was given back for want of demand.
    Idle,
}

/// The latest outcome, with the demand it answers: every demand up to it
/// was made before the acquisition that made it began.
#[derive(Clone)]
pub(crate) struct Answer {
    pub(crate) demand: u64,
    pub(crate) latest: Latest,
}

/// The broker's handle on its source.
pub(crate) struct Handle {
    demand: watch::Sender<u64>,
    pub(crate) answers: watch::Receiver<Answer>,
    task: tokio::task::JoinHandle<()>,
}

impl Handle {
    /// Ask for a picture whose acquisition begins from now on; the answer
    /// to it carries this demand or a later one.
    pub(crate) fn want(&self) -> u64 {
        let mut wanted = 0;
        self.demand.send_modify(|demand| {
            *demand += 1;
            wanted = *demand;
        });
        wanted
    }

    /// End the source, giving the stream back first.
    pub(crate) async fn close(self) {
        drop(self.demand);
        let _ = self.task.await;
    }
}

/// The screen's privacy generation now, or None while it is hidden.
pub(crate) type Visibility = Arc<dyn Fn() -> Option<Visible> + Send + Sync>;

/// Start the source over `acquire`, charging its pictures to `budget` and
/// marking each with the privacy generation `visible` held through its
/// acquisition. A notice on `input` says input was just sent to the helper,
/// which ends a still screen's slower cadence at once.
pub(crate) fn spawn(
    mut acquire: impl Acquire,
    budget: Budget,
    input: Arc<Notify>,
    visible: Visibility,
) -> Handle {
    let (demand, mut wanted) = watch::channel(0_u64);
    let (answer, answers) = watch::channel(Answer {
        demand: 0,
        latest: Latest::Nothing,
    });
    let task = tokio::spawn(async move {
        let mut answered = 0;
        let mut active = false;
        let mut still = Stillness::default();
        loop {
            let asked = *wanted.borrow_and_update();
            if asked > answered {
                let began = tokio::time::Instant::now();
                let before = visible();
                let latest = match read(&mut acquire, &budget).await {
                    Ok(mut picture) => {
                        picture.visible = before.filter(|before| visible() == Some(*before));
                        let picture = Arc::new(picture);
                        still.saw(&picture);
                        Latest::Picture(picture)
                    }
                    Err(reason) => {
                        still = Stillness::default();
                        Latest::Failed(reason)
                    }
                };
                active = true;
                answered = asked;
                answer.send_replace(Answer {
                    demand: asked,
                    latest,
                });
                // Waiting only puts off the next acquisition, which still
                // begins after the demand it answers: no picture is kept
                // longer or offered again.
                tokio::select! {
                    () = tokio::time::sleep_until(began + still.cadence()) => {}
                    () = input.notified() => {
                        still.stirred();
                        tokio::time::sleep_until(began + CADENCE).await;
                    }
                }
                continue;
            }
            tokio::select! {
                changed = wanted.changed() => if changed.is_err() { break },
                () = tokio::time::sleep(IDLE), if active => {
                    active = false;
                    still = Stillness::default();
                    let latest = match acquire.stop().await {
                        Ok(()) => Latest::Idle,
                        Err(reason) => Latest::Failed(reason),
                    };
                    answer.send_replace(Answer { demand: answered, latest });
                }
            }
        }
        let _ = acquire.stop().await;
    });
    Handle {
        demand,
        answers,
        task,
    }
}

/// The next picture from `acquire`. One that came too late is read again,
/// once and at once: the helper was slow for a moment, as when WindowServer
/// stalls, and its stream is still at a frame boundary. Reported as a
/// failure, it would end every observer's credits and, with no demand left,
/// the helper's stream; the new reading still begins after the demand it
/// answers.
async fn read(acquire: &mut impl Acquire, budget: &Budget) -> Result<Picture, Reason> {
    match acquire.acquire(budget).await {
        Err(Reason::SourceStale) => acquire.acquire(budget).await,
        read => read,
    }
}

/// The helper's live stream as a source.
pub(crate) struct Live {
    client: Arc<Client>,
    longest: u32,
    backend: CaptureBackend,
    stream: Option<LiveStream>,
    ring: Ring,
    sequence: u64,
}

impl Live {
    pub(crate) fn new(client: Arc<Client>, longest: u32, backend: CaptureBackend) -> Self {
        Self {
            client,
            longest,
            backend,
            stream: None,
            ring: Ring::default(),
            sequence: 0,
        }
    }
}

impl Acquire for Live {
    async fn acquire(&mut self, budget: &Budget) -> Result<Picture, Reason> {
        let stream = match &mut self.stream {
            Some(stream) => stream,
            None => {
                let opened = LiveStream::connect(&self.client, self.longest, self.backend)
                    .await
                    .map_err(|error| Reason::of(error.code))?
                    .ok_or(Reason::SourceStartFailed)?;
                self.stream.insert(opened)
            }
        };
        match stream
            .next_reserved(&self.client, &mut self.ring, budget)
            .await
        {
            Ok(frame) => {
                self.sequence += 1;
                let display = &frame.epoch.display;
                let measure = |value: f64| Measure::new(value).ok_or(Reason::WrongGeometry);
                Ok(Picture {
                    pixels: frame.rgb,
                    sequence: Positive::new(self.sequence).ok_or(Reason::Oversized)?,
                    at: frame.at,
                    display: [
                        measure(display.width)?,
                        measure(display.height)?,
                        measure(display.scale)?,
                    ],
                    backend: match self.backend {
                        CaptureBackend::CoreGraphics => Source::Cg,
                        CaptureBackend::ScreenCaptureKit => Source::Sck,
                    },
                    epoch: frame.epoch,
                    // The source marks it once read.
                    visible: None,
                })
            }
            Err(error) => {
                let reason = self
                    .ring
                    .last()
                    .map_or(Reason::of(error.code), |record| record.reason);
                // Only a stream left at a frame boundary is read again.
                if !stream.can_retry(&error) {
                    self.stop().await?;
                }
                Err(reason)
            }
        }
    }

    async fn stop(&mut self) -> Result<(), Reason> {
        match self.stream.take() {
            Some(mut stream) => stream
                .stop(&self.client)
                .await
                .map_err(|_| Reason::DrainUnconfirmed),
            None => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests;

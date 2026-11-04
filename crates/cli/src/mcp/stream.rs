//! The view's pictures from the menu bar's frame service: one observer for
//! every view this server draws, paired while a view reads and let go soon
//! after they stop, each picture made into the frame the view shows.
use super::Server;
use crate::frame_pipe::client::{LONGEST, Picture, PipeClient};
use atlas_client::helper_info::frame_rgb;
use serde_json::Value;
use std::time::{Duration, Instant};

/// How long an observer the view stopped reading from is kept.
const IDLE: Duration = Duration::from_secs(10);
/// How long a service that failed is left before it is tried again; the
/// viewer's frames stand in meanwhile.
const RETRY: Duration = Duration::from_secs(5);

/// The longest a view's read waits on the service, its turn included,
/// before the viewer's frame is read instead.
const DEADLINE: Duration = Duration::from_secs(3);

/// The observer, when the view last read through it, and when the service
/// last failed.
#[derive(Default)]
pub(super) struct Stream {
    observer: Option<PipeClient>,
    read: Option<Instant>,
    failed: Option<Instant>,
}

/// A frame from the service, the key input names it by, and the display's
/// size in points.
pub(super) struct Streamed {
    pub(super) frame: Value,
    pub(super) key: String,
    pub(super) display: (f64, f64),
}

/// `picture` made into the frame the view shows.
async fn framed(picture: Picture) -> Option<Streamed> {
    let metadata = picture.metadata;
    let pixels = (
        metadata.width.get() as usize,
        metadata.height.get() as usize,
        metadata.row_stride as usize,
    );
    let rgb = picture.rgb;
    let frame = tokio::task::spawn_blocking(move || frame_rgb(&rgb, pixels))
        .await
        .ok()?
        .ok()?;
    Some(Streamed {
        frame,
        key: metadata.frame_key.as_str().to_owned(),
        display: (metadata.display_width.get(), metadata.display_height.get()),
    })
}

impl Server {
    /// The service's next picture as a frame: None when the menu bar runs no
    /// service for this helper, it failed lately, or it does not answer in
    /// time. The stream stays locked through encoding, so a failure to
    /// encode counts as the service's.
    pub(super) async fn streamed(&self) -> Option<Streamed> {
        let until = tokio::time::Instant::now() + DEADLINE;
        let mut stream = tokio::time::timeout_at(until, self.stream.lock())
            .await
            .ok()?;
        if stream.failed.is_some_and(|at| at.elapsed() < RETRY) {
            return None;
        }
        let picture = tokio::time::timeout_at(until, self.picture(&mut stream))
            .await
            .ok()
            .flatten();
        let streamed = match picture {
            Some(picture) => framed(picture).await,
            None => None,
        };
        stream.failed = streamed.is_none().then(Instant::now);
        stream.read = Some(Instant::now());
        streamed
    }

    /// The next picture committed, pairing first when needed. The observer
    /// is held out of the stream while it reads, so one that fails or is
    /// given up on part way is let go.
    async fn picture(&self, stream: &mut Stream) -> Option<Picture> {
        let mut observer = match stream.observer.take() {
            Some(observer) => observer,
            None => PipeClient::open(&self.frame_service().await?, LONGEST)
                .await
                .ok()?,
        };
        let picture = observer.committed().await.ok()?;
        stream.observer = Some(observer);
        Some(picture)
    }

    /// The description of the frame service the menu bar's viewer runs for
    /// this helper.
    async fn frame_service(&self) -> Option<Value> {
        let output = self.client.output_dir().ok()?.to_path_buf();
        let (port, helper) = (self.port, self.client.transport.peer_uid);
        tokio::task::spawn_blocking(move || {
            crate::viewer::frame_service(&crate::viewer::local(&output, port)?, helper)
        })
        .await
        .ok()
        .flatten()
    }

    /// Let go of an observer the view stopped reading from, unless it is
    /// being read now.
    pub(super) fn rest_stream(&self) {
        if let Ok(mut stream) = self.stream.try_lock()
            && stream.read.is_some_and(|at| at.elapsed() >= IDLE)
        {
            stream.observer = None;
            stream.read = None;
        }
    }
}

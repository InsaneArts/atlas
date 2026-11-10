//! A terminal-owned viewer: the same verified capture and leased input rules,
//! with an uncached capture profile and no menu bar, app launch or record changes.
use super::{
    SharedFrame, Viewer,
    http::{self, CONNECTIONS},
};
use atlas_client::client::{
    Client,
    live::{LiveFrame, LiveStream},
};
use atlas_core::{
    error::{Code, Error, Result},
    frame_pipe::diagnostics::Ring,
    protocol::CaptureBackend,
};
use std::{path::Path, sync::Arc, time::Duration};
use tokio::{net::TcpListener, sync::oneshot, task::JoinHandle};

/// The widest picture the PNG path asks for, as the menu bar's viewer does.
const PNG_WIDTH: u32 = 1600;
/// How often the terminal's viewer follows its settings file and ends
/// expired control.
const UPKEEP: Duration = Duration::from_millis(100);
/// How long a reconfigured stream waits for the old one's slot to free.
const RECONNECT: Duration = Duration::from_millis(500);
/// The pause between tries while it waits.
const RECONNECT_TRY: Duration = Duration::from_millis(20);

/// The viewer a terminal runs for itself, and how it serves.
pub(crate) struct TerminalService {
    pub(crate) url: String,
    viewer: Arc<Viewer>,
    dimension: u32,
    backend: CaptureBackend,
    shutdown: oneshot::Sender<()>,
    server: JoinHandle<Result<()>>,
}

impl TerminalService {
    /// Serve a private viewer for `client`, following `settings`, whose
    /// captures are at most `width` pixels across.
    pub(crate) async fn start(
        client: Client,
        settings: &Path,
        width: u32,
        backend: CaptureBackend,
    ) -> Result<Self> {
        if !(320..=4096).contains(&width) {
            return Err(Error::new(
                Code::Usage,
                "Terminal capture size must be between 320 and 4096 pixels",
            ));
        }
        let (listener, mut viewer) = http::bind(client, 0, Some(settings)).await?;
        let configured = Arc::get_mut(&mut viewer).ok_or_else(|| {
            Error::new(
                Code::State,
                "The terminal viewer was shared before configuration",
            )
        })?;
        configured.frame_width = width.min(PNG_WIDTH);
        configured.frame_reuse = Duration::ZERO;
        let url = viewer.url.clone();
        let (shutdown, stopped) = oneshot::channel();
        let server = tokio::spawn(serve(listener, viewer.clone(), stopped));
        Ok(Self {
            url,
            viewer,
            dimension: width,
            backend,
            shutdown,
            server,
        })
    }

    pub(crate) fn frames(&self) -> TerminalFrames {
        TerminalFrames {
            viewer: self.viewer.clone(),
            stream: None,
            retired: None,
            connected: false,
            client: None,
            dimension: self.dimension,
            backend: self.backend,
            reconfiguring: false,
            ring: Ring::default(),
        }
    }

    /// Closing waits for native cleanup and every in-flight mutation. A failed
    /// cleanup remains pending in the existing global controller state.
    pub(crate) async fn shutdown(self) -> Result<()> {
        let _ = self.shutdown.send(());
        self.server.await.map_err(|_| {
            Error::new(Code::Uncertain, "Terminal viewer shutdown did not finish").uncertain()
        })?
    }
}

/// The renderer reads directly from its private service, without a second
/// local HTTP copy. Compatible helpers use the persistent raw connection.
pub(crate) struct TerminalFrames {
    viewer: Arc<Viewer>,
    stream: Option<LiveStream>,
    /// A stream left for a new size, stopped before the next one opens.
    retired: Option<LiveStream>,
    connected: bool,
    client: Option<Arc<Client>>,
    dimension: u32,
    backend: CaptureBackend,
    reconfiguring: bool,
    /// Every acquisition's outcome, failed ones included.
    ring: Ring,
}

impl TerminalFrames {
    pub(crate) fn set_dimension(&mut self, dimension: u32) {
        if self.dimension != dimension {
            self.reconfiguring = self.connected;
            self.dimension = dimension;
            self.retire();
            self.connected = false;
        }
    }

    /// Leave the open stream, if there is one, to be stopped before the next
    /// opens; a stream still waiting for its stop to be confirmed stays.
    fn retire(&mut self) {
        if let Some(stream) = self.stream.take() {
            self.retired = Some(stream);
        }
    }

    pub(crate) fn live(&self) -> bool {
        self.stream.is_some()
    }

    /// The shortest gap between reads: a raw stream's capture cadence, or the
    /// PNG path's slower one.
    pub(crate) fn interval(&self) -> Duration {
        match (self.live(), self.backend) {
            (true, CaptureBackend::ScreenCaptureKit) => Duration::from_millis(16),
            (true, CaptureBackend::CoreGraphics) => Duration::from_millis(33),
            (false, _) => Duration::from_millis(100),
        }
    }

    pub(crate) async fn next(&mut self) -> Result<SharedFrame> {
        let client = self.viewer.client();
        if self
            .client
            .as_ref()
            .is_none_or(|previous| !Arc::ptr_eq(previous, &client))
        {
            // A stream of another helper's daemon closes; that daemon gives
            // its source back when the connection ends.
            self.stream = None;
            self.retired = None;
            self.connected = false;
            self.client = Some(client.clone());
        }
        let result = self.read(&client).await;
        if result.as_ref().is_err_and(|error| {
            !self
                .stream
                .as_ref()
                .is_some_and(|stream| stream.can_retry(error))
        }) {
            self.retire();
            self.connected = false;
        }
        result
    }

    async fn read(&mut self, client: &Client) -> Result<SharedFrame> {
        if !self.connected {
            // The next stream opens only once the last one's source is
            // confirmed stopped; until then each read asks again and reports
            // the unconfirmed stop.
            if let Some(retired) = self.retired.as_mut() {
                retired.stop(client).await?;
                self.retired = None;
            }
            let until = tokio::time::Instant::now() + RECONNECT;
            self.stream = loop {
                match LiveStream::connect(client, self.dimension, self.backend).await {
                    Err(error)
                        if self.reconfiguring
                            && error.code == Code::Busy
                            && tokio::time::Instant::now() < until =>
                    {
                        tokio::time::sleep(RECONNECT_TRY).await
                    }
                    result => break result?,
                }
            };
            self.reconfiguring = false;
            self.connected = true;
        }
        if let Some(stream) = &mut self.stream {
            let frame = stream.next(client, &mut self.ring).await?;
            let key = self.viewer.remember_live(&frame).await;
            Ok(live_frame(frame, key))
        } else {
            let (key, frame) = self.viewer.frame().await?;
            Ok(SharedFrame {
                png: frame.png.to_vec(),
                rgb: None,
                pixels: (frame.width, frame.height),
                metrics: None,
                key,
                display: (frame.epoch.display.width, frame.epoch.display.height),
            })
        }
    }
}

/// A live frame as the renderer reads it, named by `key`.
fn live_frame(frame: LiveFrame, key: String) -> SharedFrame {
    SharedFrame {
        png: Vec::new(),
        rgb: Some(frame.rgb),
        pixels: (frame.width, frame.height),
        metrics: Some(frame.metrics),
        key,
        display: (frame.epoch.display.width, frame.epoch.display.height),
    }
}

async fn serve(
    listener: TcpListener,
    viewer: Arc<Viewer>,
    mut stopped: oneshot::Receiver<()>,
) -> Result<()> {
    let shutdown_view = viewer.clone();
    let (cleanup, completed) = oneshot::channel();
    http::accept(listener, viewer, CONNECTIONS, async move {
        let mut upkeep = tokio::time::interval(UPKEEP);
        loop {
            tokio::select! {
                _ = &mut stopped => break,
                _ = upkeep.tick() => {
                    shutdown_view.follow_settings().await;
                    shutdown_view.end_expired().await;
                }
            }
        }
        // Revoke ownership independently of an input request still executing.
        // Shutdown retains exclusion while both native cleanup and the request
        // finish, rather than waiting for input before starting cancellation.
        let ending = shutdown_view.clone();
        let release = tokio::spawn(async move { ending.release(None).await });
        shutdown_view.close().await;
        let result = release.await.unwrap_or_else(|_| {
            Err(Error::new(
                Code::Uncertain,
                "Terminal control cleanup stopped before acknowledgment",
            )
            .uncertain())
        });
        let _ = cleanup.send(result);
    })
    .await;
    completed.await.map_err(|_| {
        Error::new(
            Code::Uncertain,
            "Terminal viewer cleanup was not acknowledged",
        )
        .uncertain()
    })?
}

#[cfg(test)]
#[path = "terminal_tests.rs"]
mod tests;

//! Pictures of the helper as the viewer shares them: one capture at a time
//! for every request asking, and the recent frames kept for control
//! requests to name.
use super::{Viewer, unpoisoned};
use crate::{
    client::Frame,
    error::{Code, Error, Result},
    files,
    overview::{READ_DEADLINE, late},
};
use std::{collections::VecDeque, sync::Arc, time::Duration};
use tokio::sync::watch;

/// How long a frame is kept and may be named by control requests.
const FRAME_LIFETIME: Duration = Duration::from_secs(8);
/// The widest picture of the helper the viewer asks for, in pixels.
const FRAME_WIDTH: u32 = 1600;
/// How many recent frames are kept for control requests to name.
const KEPT_FRAMES: usize = 24;

/// The recent frames, each under the key control requests name it by.
#[derive(Default)]
pub(super) struct Frames {
    pub(super) items: VecDeque<(String, Arc<Frame>)>,
    /// Counts input sent, so a capture can tell whether it shows the latest.
    pub(super) input_revision: u64,
    /// The input revision the newest frame was captured at.
    capture_revision: Option<u64>,
}

/// A frame capture in progress: the input it shows, and its outcome once
/// known, which every request waiting on it shares.
pub(super) struct Capture {
    pub(super) revision: u64,
    pub(super) outcome: watch::Receiver<Option<Captured>>,
}

/// A capture's outcome as each request waiting on it reads it.
pub(super) type Captured = std::result::Result<(String, Arc<Frame>), (Code, String)>;

/// Empties the capture slot when the capture that filled it ends, however
/// it ends.
struct Capturing<'a>(&'a std::sync::Mutex<Option<Capture>>);

impl Drop for Capturing<'_> {
    fn drop(&mut self) {
        *unpoisoned(self.0) = None;
    }
}

impl Viewer {
    /// A current frame of the helper. One capture runs at a time and every
    /// request asking meanwhile shares it; one started before the latest
    /// input is waited out, then a fresh one taken. All of it, waiting
    /// included, ends within one deadline, so the app hears why before it
    /// gives up.
    pub(super) async fn frame(&self) -> Result<(String, Arc<Frame>)> {
        let until = tokio::time::Instant::now() + READ_DEADLINE;

        loop {
            let revision = self.frames.lock().await.input_revision;

            // Either this request captures, or it waits on the one running.
            let ours = {
                let mut slot = unpoisoned(&self.capture);
                match slot.as_ref() {
                    Some(running) => Err((running.outcome.clone(), running.revision >= revision)),
                    None => {
                        let (sender, outcome) = watch::channel(None);
                        *slot = Some(Capture { revision, outcome });
                        Ok(sender)
                    }
                }
            };

            let sender = match ours {
                Ok(sender) => sender,
                Err((mut outcome, current)) => {
                    let Ok(waited) =
                        tokio::time::timeout_at(until, outcome.wait_for(Option::is_some)).await
                    else {
                        return Err(late());
                    };

                    let shared = waited.ok().and_then(|outcome| outcome.clone());
                    match shared {
                        Some(captured) if current => {
                            return captured.map_err(|(code, message)| Error::new(code, message));
                        }
                        _ => continue,
                    }
                }
            };

            let _clear = Capturing(&self.capture);
            let outcome = tokio::time::timeout_at(until, self.take_frame())
                .await
                .unwrap_or_else(|_| Err(late()));
            let shared = match &outcome {
                Ok(frame) => Ok(frame.clone()),
                Err(error) => Err((error.code, error.error.clone())),
            };
            let _ = sender.send(Some(shared));

            return outcome;
        }
    }

    /// Take a frame, or reuse the last one when it is recent and no input
    /// came since.
    async fn take_frame(&self) -> Result<(String, Arc<Frame>)> {
        let client = self.client();
        let current = client.identity().await?.epoch;
        let revision;

        {
            let mut frames = self.frames.lock().await;
            frames
                .items
                .retain(|(_, frame)| frame.at.elapsed() < FRAME_LIFETIME && frame.epoch == current);
            revision = frames.input_revision;

            if frames.capture_revision == Some(revision)
                && let Some((key, frame)) = frames.items.back()
                && frame.at.elapsed() < Duration::from_millis(400)
            {
                return Ok((key.clone(), frame.clone()));
            }
        }

        let frame = Arc::new(client.frame(Some(FRAME_WIDTH)).await?);
        if frame.at.elapsed() >= FRAME_LIFETIME {
            return Err(Error::new(
                Code::StaleFrame,
                "Capture became stale while checking its session",
            ));
        }

        let key = files::token();
        let mut frames = self.frames.lock().await;
        frames.items.push_back((key.clone(), frame.clone()));
        frames.capture_revision = Some(revision);

        while frames.items.len() > KEPT_FRAMES {
            frames.items.pop_front();
        }

        Ok((key, frame))
    }

    /// The recent frame kept under `key`.
    pub(super) async fn observed(&self, key: &str) -> Result<Arc<Frame>> {
        self.frames
            .lock()
            .await
            .items
            .iter()
            .find(|(id, _)| id == key)
            .map(|(_, frame)| frame.clone())
            .filter(|frame| frame.at.elapsed() < FRAME_LIFETIME)
            .ok_or_else(|| {
                Error::new(
                    Code::StaleFrame,
                    "Wait for a fresh frame before taking control",
                )
            })
    }
}

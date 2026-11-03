//! The MCP view's lease at the local viewer: acquire, renew, input and cleanup.
use super::{
    Server,
    presentation::{add_points, tool_result},
    resources::FRAME_META,
};
use crate::viewer::{LeaseRefusal, SharedFrame};
use atlas_client::helper_info::FRAME_LIMIT;
use atlas_core::error::{Code, Error, Result};
use serde::Deserialize;
use serde_json::{Value, json};
use std::time::{Duration, Instant};

/// How long a frame's key names it in control requests: the viewer keeps
/// its last 24 frames for eight seconds, and a key it no longer keeps ends
/// control, so only a recent one is named even while Atlas's own viewer
/// window takes frames quickly.
const KEY_REUSE: Duration = Duration::from_secs(1);
/// How long the viewer's control lasts unless it is kept.
pub(super) const LEASE: Duration = Duration::from_secs(10);
/// How long a frame read from the menu bar's viewer may take.
const FRAME_DEADLINE: Duration = Duration::from_secs(6);
/// The most actions one input request carries, as the helper takes them.
pub(super) const ACTIONS: usize = 32;

/// A frame of the menu bar's viewer: the key control requests name it by,
/// and when it was seen.
pub(super) struct Seen {
    pub(super) key: String,
    pub(super) at: Instant,
}

/// The person's control taken from the view: the viewer's address, the
/// lease it gave, and when it was last kept.
#[derive(Clone)]
pub(super) struct Held {
    pub(super) url: String,
    pub(super) lease: String,
    pub(super) kept: Instant,
}

impl Held {
    /// Whether it was kept within a lease's length.
    fn live(&self) -> bool {
        self.kept.elapsed() < LEASE
    }
}

/// The request a lease was used for, which decides what success keeps.
pub(super) enum LeaseRequest {
    Renew,
    Input,
}

/// Run `work`, which waits on the viewer's socket, off the runtime.
async fn off_runtime<T: Send + 'static>(
    work: impl FnOnce() -> Result<T> + Send + 'static,
) -> Result<T> {
    tokio::task::spawn_blocking(work).await.map_err(|error| {
        Error::new(Code::Menubar, "Atlas's menu bar didn't answer").with_source(error)
    })?
}

/// POST `body` to `route` of the menu bar's viewer at `url`. An answer that
/// never came (`menubar`) may still have changed control, so it is
/// uncertain; the viewer's own refusals keep what they say.
async fn post(url: &str, route: &'static str, body: Value) -> Result<Value> {
    let url = url.to_owned();
    off_runtime(move || crate::viewer::post_local(&url, route, &body))
        .await
        .map_err(|error| match error.code {
            Code::Menubar => error.uncertain(),
            _ => error,
        })
}

/// Ask the viewer to end `held`. A release names its own lease, so it never
/// ends anyone else's.
async fn ask_release(held: &Held) -> Result<Value> {
    post(&held.url, "control/release", json!({"lease": held.lease})).await
}

/// End `held` at the viewer, as far as it still holds it. A failed
/// best-effort release leaves the viewer's native recovery records and lease
/// expiry intact.
pub(super) async fn release(held: &Held) {
    let _ = ask_release(held).await;
}

impl Server {
    /// Find the viewer, starting the menu bar when it is not running.
    async fn viewer(&self) -> Result<String> {
        let output = self.client.output_dir()?.to_path_buf();
        let port = self.port;
        let found = {
            let output = output.clone();
            tokio::task::spawn_blocking(move || crate::viewer::local(&output, port))
                .await
                .ok()
                .flatten()
        };

        match found {
            Some(url) => Ok(url),
            None => crate::viewer::launched_url(&self.config, port, &output).await,
        }
    }

    /// Remember `key` as the viewer's newest frame, seen now.
    pub(super) async fn saw_frame(&self, key: String) {
        *self.shared.lock().await = Some(Seen {
            key,
            at: Instant::now(),
        });
    }

    /// Reuse a recent frame key, otherwise fetch one the viewer still keeps.
    async fn key(&self, url: &str) -> Result<String> {
        if let Some(seen) = self.shared.lock().await.as_ref()
            && seen.at.elapsed() < KEY_REUSE
        {
            return Ok(seen.key.clone());
        }

        Ok(self.shared_frame(url).await?.key)
    }

    /// Fetch and remember the viewer's frame for subsequent renewals.
    async fn shared_frame(&self, url: &str) -> Result<SharedFrame> {
        let address = url.to_owned();
        let frame =
            off_runtime(move || crate::viewer::fetch_frame(&address, FRAME_DEADLINE, FRAME_LIMIT))
                .await?;
        self.saw_frame(frame.key.clone()).await;
        Ok(frame)
    }

    /// Take control of the helper for the person, through the menu bar's
    /// viewer (started when it is not running). Agents wait meanwhile.
    /// Control already taken from here is kept, or taken afresh when it
    /// lapsed.
    pub(super) async fn take_control(&self) -> Result<Value> {
        // The slow steps run outside the lock, so a status or Give Back
        // never waits on them.
        let current = self.held.lock().await.clone();
        let url = match &current {
            Some(current) if current.live() => current.url.clone(),
            _ => self.viewer().await?,
        };
        let frame = self.shared_frame(&url).await?;
        let kept = match &current {
            Some(current) if current.live() => self.renew(current).await.is_ok(),
            _ => false,
        };

        if !kept {
            if let Some(old) = current {
                self.forget(&old).await;
                release(&old).await;
            }
            let reply = post(&url, "control/acquire", json!({"frame": frame.key})).await?;
            let lease = reply["lease"].as_str().ok_or_else(|| {
                Error::new(Code::Menubar, "The menu bar gave no control").uncertain()
            })?;
            let taken = Held {
                url,
                lease: lease.to_owned(),
                kept: Instant::now(),
            };

            let mut held = self.held.lock().await;
            if held.is_none() {
                *held = Some(taken);
            } else {
                // Another take landed meanwhile: that one stays.
                drop(held);
                release(&taken).await;
            }
        }

        let mut result = self.status_result().await?;
        add_points(&mut result, frame.display);
        let mut picture = self.frame_of(frame.png).await?;
        picture["key"] = json!(frame.key);
        result["_meta"] = json!({ FRAME_META: picture });
        Ok(result)
    }

    /// Keep the person's control a while longer: the view says so every
    /// few seconds, so control ends soon after it closes.
    pub(super) async fn keep_control(&self) -> Result<Value> {
        let held = self.holding().await?;
        self.renew(&held).await?;
        Ok(tool_result("You still have control.", json!({})))
    }

    /// Renew `held` at the viewer, naming a frame it keeps.
    async fn renew(&self, held: &Held) -> Result<Value> {
        let renewed = async {
            let key = self.key(&held.url).await?;
            post(
                &held.url,
                "control/renew",
                json!({"lease": held.lease, "frame": key}),
            )
            .await
        }
        .await;
        self.settle(held, &renewed, LeaseRequest::Renew).await;
        renewed
    }

    /// End the person's control; agents may go on.
    pub(super) async fn give_back(&self) -> Result<Value> {
        let held = self.held.lock().await.clone();
        if let Some(held) = held {
            // Forgotten once the viewer let it go, or never had it; kept
            // (and still kept alive by the view) when the release failed.
            match ask_release(&held).await {
                Err(error) if error.code != Code::LeaseLost => return Err(error),
                _ => self.forget(&held).await,
            }
        }

        self.status_result().await
    }

    /// Send the person's input to the helper while they have control.
    pub(super) async fn send_input(&self, frame: Option<&str>, actions: &Value) -> Result<Value> {
        let actions = checked(actions)?;
        let held = self.holding().await?;
        let key = frame
            .filter(|key| !key.is_empty())
            .ok_or_else(Error::stale_frame)?;
        let body = json!({"lease": held.lease, "frame": key, "actions": actions});
        let sent = post(&held.url, "input", body).await;
        self.settle(&held, &sent, LeaseRequest::Input).await;
        sent?;

        let count = actions.len();
        Ok(tool_result(
            &format!("Sent {count} actions."),
            json!({"sent": count}),
        ))
    }

    /// The person's control from here, or why there is none: control nobody
    /// kept for as long as it lasts has ended, and is let go of.
    pub(super) async fn holding(&self) -> Result<Held> {
        // Keep local lease changes serialized through the viewer's release.
        // The HTTP viewer cancels independently and owns native pending work.
        let mut held = self.held.lock().await;
        if let Some(lapsed) = held.take_if(|held| !held.live()) {
            release(&lapsed).await;
        }
        held.clone()
            .ok_or_else(|| Error::new(Code::LeaseLost, "Take control first"))
    }

    /// Whether the person has control from here now.
    pub(super) async fn controlling(&self) -> bool {
        self.held.lock().await.as_ref().is_some_and(Held::live)
    }

    /// After a request on `used`: a renewal that went through keeps it
    /// another lease (input does not, as only a renewal extends the
    /// viewer's). It is forgotten and let go of when refused for any reason
    /// the viewer ends it on; a busy viewer, input it would not read, or an
    /// answer that never came (`menubar`) leave it to its lease. Only that
    /// same control is touched: one taken since stays.
    pub(super) async fn settle(&self, used: &Held, outcome: &Result<Value>, request: LeaseRequest) {
        // Retain the local lease lock through cleanup so a successor cannot
        // replace this lease until its release has been asked of the viewer.
        let mut held = self.held.lock().await;
        let Some(same) = held.as_mut().filter(|held| held.lease == used.lease) else {
            return;
        };

        match outcome {
            Ok(_) => match request {
                LeaseRequest::Renew => same.kept = Instant::now(),
                LeaseRequest::Input => {}
            },
            Err(error) => match LeaseRefusal::from(error.code) {
                LeaseRefusal::Busy | LeaseRefusal::Unread | LeaseRefusal::Unanswered => {}
                LeaseRefusal::Lost | LeaseRefusal::Failed => {
                    if let Some(ended) = held.take() {
                        release(&ended).await;
                    }
                }
            },
        }
    }

    /// Forget `used`, unless control taken since replaced it.
    async fn forget(&self, used: &Held) {
        let mut held = self.held.lock().await;
        if held.as_ref().is_some_and(|held| held.lease == used.lease) {
            held.take();
        }
    }
}

/// The view's input after checking it is only what the person may send:
/// pointer, move, scroll, typing and keys, at most as many as the helper
/// takes at once. The helper's own checks (points on its display, typing
/// within its time) come when it is sent.
pub(super) fn checked(actions: &Value) -> Result<&[Value]> {
    let list = actions
        .as_array()
        .filter(|list| (1..=ACTIONS).contains(&list.len()))
        .ok_or_else(|| Error::new(Code::Input, "Send 1 to 32 actions"))?;

    if list.iter().any(|action| {
        !atlas_core::actions::Kind::deserialize(action)
            .is_ok_and(atlas_core::actions::Kind::personal)
    }) {
        return Err(Error::new(
            Code::Input,
            "Only clicks, scrolls, typing and keys may be sent",
        ));
    }

    Ok(list)
}

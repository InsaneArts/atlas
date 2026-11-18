//! The picture right before an agent's input: one raw frame of the helper's
//! screen from its daemon, or of a VM's from its guest through its keeper,
//! asked for beside the input's own preflight, and encoded and saved only
//! after the input is sent.
//!
//! The input goes only once the picture is in hand, or once the daemon has
//! answered that it was cancelled: no capture for it begins after that, and
//! one already begun has ended. A daemon that answers neither in time is
//! left, and the input goes then too. A VM's is kept only once a proof
//! after it shows what the input's own proof showed, its window closed both
//! times and Atlas VM's picture fence where it was.
use super::{Actor, Client, Pinned, Verified, live::read_header};
use crate::{
    before::{self, Raw},
    link::Link,
};
use atlas_control::Visible;
use atlas_core::{
    actions::Action,
    error::{Code, Error, Result},
    protocol::SessionPin,
    screen::elements::Element,
    session::{Console, InputEpoch},
};
use atlas_session::compatibility::PICTURES;
use serde_json::json;
use std::{
    future::Future,
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    sync::oneshot,
    task::JoinHandle,
};

/// The longest an input waits for its picture, from when it was asked for,
/// its cancel's answer included.
pub(super) const WAIT: Duration = Duration::from_millis(150);
/// The last part of [`WAIT`], kept for the daemon to answer the picture's
/// cancel.
const CANCEL: Duration = Duration::from_millis(50);
/// The longest a question to the person waits for its picture: it is put
/// up without one after that.
const FOR_ASK: Duration = Duration::from_secs(1);
/// The longest the picture after an action is waited for, once due: a
/// helper that takes longer leaves the action without one, and its process
/// ends.
const AFTER: Duration = Duration::from_secs(5);

/// A picture being taken, when the input stops waiting for it, and the way
/// to cancel it then; dropping it leaves the picture.
pub(crate) struct Taking {
    task: JoinHandle<Result<Raw>>,
    deadline: Instant,
    cancel: Option<oneshot::Sender<()>>,
    /// The screen's privacy when it was asked for.
    admitted: Visible,
    /// The proof of the VM it is taken on, which a proof after it must show
    /// again before it is kept; none for the helper.
    proved: Option<Pinned>,
}

impl std::fmt::Debug for Taking {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Taking")
            .field("deadline", &self.deadline)
            .finish_non_exhaustive()
    }
}

impl Drop for Taking {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl Taking {
    /// The proof of the VM it is taken on; none for the helper.
    pub(super) fn proved(&self) -> Option<&Pinned> {
        self.proved.as_ref()
    }

    /// The picture, when it is in hand by its deadline, with the screen's
    /// privacy when it was asked for: it is cancelled [`CANCEL`] before,
    /// and may still come while the daemon answers. None when it failed,
    /// was cancelled, or nothing came by the deadline, which this never
    /// waits past.
    pub(super) async fn taken(mut self) -> Option<(Raw, Visible)> {
        let cancel_at = self.deadline.checked_sub(CANCEL).unwrap_or(self.deadline);
        let taken = match tokio::time::timeout_at(cancel_at.into(), &mut self.task).await {
            Ok(taken) => taken,
            Err(_) => {
                // Only the task, gone already, would not hear it.
                let _ = self.cancel.take()?.send(());
                tokio::time::timeout_at(self.deadline.into(), &mut self.task)
                    .await
                    .ok()?
            }
        };
        taken.ok()?.ok().map(|raw| (raw, self.admitted))
    }
}

impl Client {
    /// Start taking the picture before an input in the session of `epoch`,
    /// whose daemon is `capture_pid`, when it is an agent's recorded input,
    /// the settings keep pictures, and its command has none yet: one for
    /// each command, before its first input. It is asked for as soon as the
    /// input's session is known, beside the rest of its preflight, and the
    /// input stops waiting for it [`WAIT`] after that. None is taken while
    /// the person signs the helper in to an account.
    pub(super) fn picture_before(&self, epoch: &InputEpoch, capture_pid: u32) -> Option<Taking> {
        let (link, epoch) = (self.link.clone(), epoch.clone());
        self.picture_taken(None, move |cancelled| {
            frame(link, epoch, capture_pid, cancelled)
        })
    }

    /// [`Client::picture_before`] an input to the VM `verified` proved, from
    /// its guest through its keeper, in that very proof's session and
    /// generation: none while its guest may not record or gives no
    /// pictures, or Atlas VM keeps no picture fence to hold it to.
    pub(super) fn picture_before_on_vm(&self, verified: &Verified) -> Option<Taking> {
        let pinned = verified.machine()?;
        if !verified.session().capture
            || !verified.observation.has_capability(PICTURES)
            || pinned.fence.is_none()
        {
            return None;
        }
        let (link, epoch) = (self.link.clone(), pinned.epoch.input_epoch());
        let (generation, daemon) = (pinned.generation, pinned.epoch.capture_pid);
        self.picture_taken(Some(pinned.clone()), move |cancelled| {
            frame_on_vm(link, generation, epoch, daemon, cancelled)
        })
    }

    /// Start taking the picture before an input with `frame`, as
    /// [`Client::picture_before`] says when, on the VM's proof `proved`.
    fn picture_taken<F>(
        &self,
        proved: Option<Pinned>,
        frame: impl FnOnce(oneshot::Receiver<()>) -> F,
    ) -> Option<Taking>
    where
        F: Future<Output = Result<Raw>> + Send + 'static,
    {
        let deadline = Instant::now() + WAIT;
        let doing = self.doing.as_ref().filter(|_| self.actor == Actor::Agent)?;
        let admitted = self.controller.screen_visible().ok()?;
        if !self.activity().pictures_before_actions || !doing.before().take() {
            return None;
        }
        let (cancel, cancelled) = oneshot::channel();
        let task = tokio::spawn(frame(cancelled));
        Some(Taking {
            task,
            deadline,
            cancel: Some(cancel),
            admitted,
            proved,
        })
    }

    /// A picture of the helper's screen now, for a question to the person:
    /// taken once the command's input is cleaned up and its control let go,
    /// so the person taking over never waits for it; in the helper's session
    /// as verified afresh, never while the person has stopped agents or
    /// signs the helper in to an account, and whether or not the settings
    /// keep pictures of actions. None when none comes within [`FOR_ASK`], or
    /// a sign-in began or ended while it was taken; otherwise with the
    /// screen's privacy when it was asked for, which its question checks
    /// again before it goes up.
    pub(crate) async fn picture_for_ask(&self) -> Option<(Raw, Visible)> {
        if self.controller.stop_noted() {
            return None;
        }
        let admitted = self.controller.screen_visible().ok()?;
        let raw = tokio::time::timeout(FOR_ASK, async {
            let verified = self.verify().await.ok()?;
            let epoch = &verified.session().epoch;
            let (_cancel, cancelled) = oneshot::channel();
            frame(
                self.link.clone(),
                epoch.input_epoch(),
                epoch.capture_pid,
                cancelled,
            )
            .await
            .ok()
        })
        .await
        .ok()
        .flatten()?;
        self.controller.still_visible(admitted).ok()?;
        Some((raw, admitted))
    }

    /// Name `raw` on its command's rows from now on, with the facts of the
    /// input `actions` sent in the session of `epoch` and the name of its
    /// task, and encode and save it aside, off the input's path. A picture
    /// that cannot be saved is never shown: its rows name a file that is
    /// not there.
    pub(super) fn keep_picture(&self, raw: Raw, actions: &[Action], epoch: &InputEpoch) {
        let Some(doing) = &self.doing else {
            return;
        };
        // The key of a task split from its session, as its rows name it.
        let task = self.task.as_ref().filter(|task| task.name().is_some());
        let name = before::name(
            raw.at,
            self.host_session.as_ref().map(|session| session.key()),
            task.map(|task| task.key()),
        );
        let mut facts = facts::of_input(actions, epoch);
        if let Some(task) = task.and_then(|task| task.name()) {
            facts.insert("task".into(), json!(task.as_str()));
        }
        doing.before().named(name.clone(), facts);
        let keep = self.activity().keep();
        before::save_aside(self.records().to_path_buf(), name, raw, keep);
    }

    /// Take the picture after the action whose picture before is `before`,
    /// which ended at `ended`, in milliseconds since 1970: a moment later,
    /// once no next step of its session began within that moment, whose
    /// picture before then serves as this one's after. It runs in a process
    /// of its own, so no agent waits for it, and ends when the helper takes
    /// longer than [`AFTER`] for it.
    ///
    /// # Errors
    ///
    /// Returns the failure to verify the helper or to take or keep the
    /// picture; the action it follows stands either way. While the person
    /// signs the helper in to an account none is taken, and one a sign-in
    /// began or ended during is dropped: both succeed with nothing kept.
    pub async fn picture_after(&self, before: &str, ended: u64) -> Result<()> {
        let output = self.records().to_path_buf();
        let after = before::after_name(before)
            .ok_or_else(|| Error::new(Code::BadRequest, "Not a picture's name"))?;
        let due = std::time::UNIX_EPOCH + Duration::from_millis(ended) + before::NEXT_WITHIN;
        if let Ok(wait) = due.duration_since(std::time::SystemTime::now()) {
            tokio::time::sleep(wait).await;
        }
        let (folder, named) = (output.clone(), before.to_owned());
        let wanted =
            tokio::task::spawn_blocking(move || before::wants_after(&folder, &named, ended))
                .await
                .unwrap_or(false);
        if !wanted {
            return Ok(());
        }

        // Right before capturing, and again before it is kept.
        let Ok(admitted) = self.controller.screen_visible() else {
            return Ok(());
        };
        let taken = tokio::time::timeout(AFTER, async {
            let verified = self.verify().await?;
            let epoch = &verified.session().epoch;
            // Never cancelled: nothing waits for this picture but its time.
            let (_keep_open, cancelled) = oneshot::channel();
            frame(
                self.link.clone(),
                epoch.input_epoch(),
                epoch.capture_pid,
                cancelled,
            )
            .await
        });
        let raw = taken
            .await
            .map_err(|_| Error::new(Code::Capture, "The picture took too long"))??;
        if self.controller.still_visible(admitted).is_err() {
            return Ok(());
        }
        let keep = self.activity().keep();
        tokio::task::spawn_blocking(move || before::save(&output, &after, &raw, keep))
            .await
            .map_err(|_| Error::new(Code::Capture, "The picture could not be kept"))?
    }

    /// Note, for the facts of an agent's command's picture, the `element`
    /// its first input acts on, found before it is sent.
    pub(crate) fn picture_found(&self, element: &Element) {
        if self.actor != Actor::Agent {
            return;
        }
        if let Some(doing) = &self.doing {
            doing.before().found(facts::found(element));
        }
    }

    /// The facts of an agent's command's picture once `row` and its `reply`
    /// to `request` say how the command went, with the picture's name; none
    /// for a command with no picture. An input that typed notes first which
    /// field the helper says it went into.
    pub(super) fn picture_facts(
        &self,
        request: &serde_json::Value,
        row: &crate::activity_log::Row,
        reply: &serde_json::Value,
    ) -> Option<(String, serde_json::Value)> {
        if self.actor != Actor::Agent {
            return None;
        }
        let slot = self.doing.as_ref()?.before();
        let typing = request["actions"]
            .as_array()
            .is_some_and(|actions| actions.iter().any(|action| action["type"] == "type"));
        if typing {
            slot.typed_into(reply["field"]["role"].as_str());
        }
        let (name, input) = slot.facts()?;
        let taken = before::time(&name)?;
        let limit = self.activity().typed_text_limit;
        let finished =
            facts::finished(input, row, reply, slot.typed(), limit, taken, self.called());
        Some((name, finished))
    }
}

/// One raw frame from the verified helper daemon `daemon` on `link`,
/// proved to come from the session and display `epoch` names. Once
/// `cancelled` is sent, the daemon is asked to cancel it, and this ends
/// with the frame, should it come first, or the daemon's answer.
async fn frame(
    link: Link,
    epoch: InputEpoch,
    daemon: u32,
    cancelled: oneshot::Receiver<()>,
) -> Result<Raw> {
    let lane = link
        .socket()?
        .lane(daemon, tokio::time::Instant::now() + WAIT)
        .await?;
    let Some(mut socket) = lane else {
        return Err(Error::new(
            Code::Identity,
            "The picture did not come from the verified helper daemon",
        ));
    };
    let mut request = serde_json::to_vec(&frame_request(&epoch))?;
    request.push(b'\n');
    socket.write_all(&request).await?;
    received(socket, epoch, daemon, Console::kept(), cancelled).await
}

/// One raw frame of a VM's screen from its guest's daemon `daemon`,
/// through its keeper on `link` in the generation `generation` its proof
/// was taken in, proved to come from the session and display `epoch`
/// names, on the guest's own console. Once `cancelled` is sent, the guest
/// is asked to cancel it, as [`frame`] asks the helper.
async fn frame_on_vm(
    link: Link,
    generation: u64,
    epoch: InputEpoch,
    daemon: u32,
    cancelled: oneshot::Receiver<()>,
) -> Result<Raw> {
    let (socket, _) = link
        .send_bulk(&frame_request(&epoch), generation)
        .await?
        .into_stream();
    received(socket, epoch, daemon, Console::Own, cancelled).await
}

/// The request of a frame for the picture before an input, in the session
/// of `epoch`.
fn frame_request(epoch: &InputEpoch) -> serde_json::Value {
    let pin = SessionPin {
        security_session_id: epoch.security_session_id,
        display_id: epoch.display_id,
    };
    json!({"op": "frame", "maxDimension": before::SIDE, "session": pin})
}

/// The frame asked for on `socket`, from the daemon `daemon`, proved to
/// come from the session and display `epoch` names, its screen where
/// `console` says. Once `cancelled` is sent, one byte asks to cancel it.
async fn received<S: AsyncRead + AsyncWrite + Unpin>(
    mut socket: S,
    epoch: InputEpoch,
    daemon: u32,
    console: Console,
    cancelled: oneshot::Receiver<()>,
) -> Result<Raw> {
    let header = tokio::select! {
        header = read_header(&mut socket) => header?,
        Ok(()) = cancelled => {
            // One byte cancels it; the daemon's answer is a refusal, or
            // the frame should it have come first.
            socket.write_all(&[0]).await?;
            read_header(&mut socket).await?
        }
    };
    let at = atlas_core::time::timestamp();
    let invalid = || Error::new(Code::Capture, "The picture's header is invalid");
    let side = |key: &str| {
        header[key]
            .as_u64()
            .filter(|side| (1..=u64::from(before::SIDE)).contains(side))
            .and_then(|side| u32::try_from(side).ok())
            .ok_or_else(invalid)
    };
    let (width, height) = (side("width")?, side("height")?);
    if header["daemonPID"] != daemon {
        return Err(invalid());
    }
    let proof = InputEpoch::from_proof_kept(&header["session"], daemon, console)?;
    if proof
        != (InputEpoch {
            process_id: daemon,
            ..epoch
        })
    {
        return Err(Error::session_changed());
    }

    let mut rgb = vec![0; width as usize * height as usize * 3];
    socket.read_exact(&mut rgb).await?;
    Ok(Raw {
        rgb,
        width,
        height,
        at,
        display: (epoch.width, epoch.height),
    })
}

pub(super) mod facts;

#[cfg(test)]
#[path = "picture_tests.rs"]
mod tests;

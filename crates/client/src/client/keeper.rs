//! What a client asks of a VM desktop through its session keeper: its status
//! with a fresh proof of it, never ready yet, and the person's Stop and
//! Resume. Everything else stays refused in the one wording for what this
//! build cannot do on a VM.
use super::{Actor, Client};
use crate::{
    link::{Keeper, Link},
    vm::proof::Observed,
};
use atlas_core::{
    config::desktops::{Admission, Chosen},
    error::{Code, Error, Result},
    session::MachineSession,
};
use serde_json::{Value, json};

/// A VM's proof as one command took it: what its keeper observed, and the
/// session that proves or why it does not.
struct Proved {
    observed: Observed,
    session: Result<MachineSession>,
}

/// `error`, a VM's status that failed, with the handover an update is still
/// making of it, `unfinished`, when one is: its code and words kept.
fn handing_over(error: Error, unfinished: Option<String>) -> Error {
    match unfinished {
        Some(why) => error.with("station", json!({"handover": why})),
        None => error,
    }
}

impl Client {
    /// The keeper this client reaches, or the reason it reaches none in the
    /// one wording, with why when it is known. A client made while the keeper
    /// was not there looks again: a long-lived one, the viewer's, meets the
    /// keeper once it starts.
    async fn keeper(&self) -> Result<Keeper> {
        let link = match (&self.link, &self.selected.desktop) {
            (Link::Unreachable(unreachable), Chosen::Vm(config)) => {
                let (selected, vm, timeout) = (
                    self.selected.clone(),
                    config.id.clone(),
                    unreachable.timeout,
                );
                tokio::task::spawn_blocking(move || crate::link::probe(&selected, &vm, timeout))
                    .await
                    .map_err(|_| self.unreachable())?
            }
            (link, _) => link.clone(),
        };
        match link {
            Link::Keeper(keeper) => Ok(keeper),
            Link::Unreachable(unreachable) => {
                let refused = self.unreachable();
                Err(match unreachable.why {
                    Some(why) => refused.with("why", json!(why)),
                    None => refused,
                })
            }
            Link::Unix(_) | Link::Session(_) => Err(self.unreachable()),
        }
    }

    /// What the person is told while an update is still handing this VM's
    /// session to the new build; none for any other desktop, and when none
    /// is under way.
    #[must_use]
    pub fn handover_unfinished(&self) -> Option<String> {
        let Chosen::Vm(config) = &self.selected.desktop else {
            return None;
        };
        crate::vm::handover::unfinished(&self.selected.name, &config.id)
    }

    /// A VM proved now, in one generation of its session: a challenge of
    /// this command's own, what `keeper` observed for it, and the VM's
    /// pairing read afresh. Nothing of it is kept.
    ///
    /// # Errors
    ///
    /// Fails when the keeper gives no observation of one generation that
    /// answers this command's challenge; a refused proof is [`Proved`]'s.
    async fn verify_machine(&self, keeper: &Keeper) -> Result<Proved> {
        let challenge = atlas_core::files::token();
        let asked = json!({"op": "observe", "challenge": challenge,
            "deadline_ms": keeper.timeout.as_millis()});
        let answer = keeper.one(&asked).await?;
        if answer["ok"] != true {
            return Err(observe_refused(&answer, &self.selected.name));
        }
        let observed = Observed::decode(&answer, &challenge)?;
        let reading = keeper.clone();
        let paired = tokio::task::spawn_blocking(move || reading.paired())
            .await
            .unwrap_or_else(|_| Err(Error::new(Code::State, "The pairing could not be read.")));
        let session = paired.and_then(|paired| observed.prove(&self.desktop(), &paired));
        Ok(Proved { observed, session })
    }

    /// A VM's status: its keeper's hello, its guest's own and a fresh proof
    /// of it, read through the keeper. It is never ready yet, proven or not.
    pub(super) async fn vm_status(&self) -> Result<Value> {
        // A VM still being added answers nothing but the person's Stop.
        if self.admission() != Admission::Open {
            return Err(self.selected.unreachable());
        }
        // An update still handing it over is said on every path, the
        // keeper's own failure kept.
        let handing = |error| handing_over(error, self.handover_unfinished());
        let keeper = self.keeper().await.map_err(handing)?;
        let hello = keeper.one(&json!({"op": "hello"})).await.map_err(handing)?;
        let (observed, session) = match self.verify_machine(&keeper).await {
            Ok(proved) => (Some(proved.observed), proved.session),
            Err(refused) => (None, Err(refused)),
        };
        let guest = match &observed {
            Some(observed) => observed.status.clone(),
            None => {
                let read = json!({"op": "read", "request": {"op": "hello"},
                    "deadline_ms": keeper.timeout.as_millis()});
                match keeper.ask(&read).await {
                    Ok(mut lines) => lines.pop().unwrap_or_else(|| json!(late(self))),
                    Err(error) => json!(error),
                }
            }
        };
        // The station's own Stop, and why: an update's says so.
        let controller = self.controller.clone();
        let station = match tokio::task::spawn_blocking(move || controller.stop_revision()).await {
            Ok(Ok(Some(revision))) => json!({"state": "stopped", "why": revision.why()}),
            Ok(Ok(None)) => json!({"state": "clear"}),
            Ok(Err(_)) | Err(_) => json!({"state": "unknown"}),
        };
        let generation = observed.as_ref().map(|observed| observed.generation);
        let mid_update = !session.as_ref().is_ok_and(|session| session.input)
            && crate::vm::proof::mid_update(&guest);
        let (proof, not_ready) = match &session {
            Ok(session) if mid_update => (
                json!({"epoch": session.epoch, "capture": session.capture,
                    "input": session.input, "generation": generation,
                    "inputWhy": crate::vm::proof::MID_UPDATE}),
                crate::vm::proof::MID_UPDATE.to_owned(),
            ),
            Ok(session) => (
                json!({"epoch": session.epoch, "capture": session.capture,
                    "input": session.input, "generation": generation}),
                format!(
                    "{} is proven, but this build of Atlas cannot act on a VM yet.",
                    self.selected.name
                ),
            ),
            Err(refused) => (
                json!({"code": refused.code, "error": refused.error}),
                refused.error.clone(),
            ),
        };
        Ok(json!({
            "ok": true,
            "proven": session.is_ok(),
            "proof": proof,
            "ready": false,
            "notReady": not_ready,
            "desktop": {"kind": "vm", "name": self.selected.name},
            "keeper": hello,
            "guest": guest,
            "runner": observed.as_ref().map(|observed| json!(observed.runner)),
            "station": {"stop": station, "handover": self.handover_unfinished()},
        }))
    }

    /// The person's Stop on a VM: kept here first, then said through the
    /// keeper, delivered only once the guest answered it applied.
    pub(super) async fn hold_vm(&self) -> Result<Value> {
        self.controller.note_stop(true)?;
        let told = match self.keeper().await {
            Ok(keeper) => keeper.one(&json!({"op": "stop"})).await,
            Err(error) => Err(error),
        };
        let delivered = told.as_ref().is_ok_and(|told| told["outcome"] == "stopped");
        let mut reply = json!({"ok": true, "held": true, "kept": true, "delivered": delivered});
        if !delivered {
            let why = match &told {
                Ok(_) => format!("{} has not confirmed this Stop.", self.selected.name),
                Err(error) => error.error.clone(),
            };
            reply["warnings"] = json!([format!("{why} Agents are stopped here all the same.")]);
        }
        if let Ok(told) = told {
            reply["seq"] = told["seq"].clone();
            reply["outcome"] = told["outcome"].clone();
        }
        Ok(reply)
    }

    /// The person's Resume on a VM, from the viewer alone, with the keeper's
    /// Resume `capability` the viewer read in the station folder: the Stop
    /// noted here as read now goes with it, and the keeper lifts that Stop
    /// once the guest resumed. A Stop given meanwhile stays.
    ///
    /// # Errors
    ///
    /// Refuses anyone but the viewer, a VM whose keeper is not reached, and a
    /// Resume the keeper refused; fails when the Stop cannot be read.
    pub async fn resume_vm(&self, capability: &str) -> Result<Value> {
        if self.actor != Actor::Viewer {
            return Err(Error::new(
                Code::Stopped,
                "Only the person resumes agents, from the Atlas menu bar",
            ));
        }
        if self.admission() != Admission::Open {
            return Err(self.selected.unreachable());
        }
        let keeper = self.keeper().await?;
        let revision = self.controller.stop_revision()?;
        let told = keeper
            .one(&json!({"op": "resume", "capability": capability, "revision": revision}))
            .await?;
        if told["ok"] != true {
            return Err(Error::refused(
                Code::Stopped,
                &told,
                "The session keeper refused the Resume",
            ));
        }
        Ok(resumption(&told))
    }
}

/// What the keeper's answer `told` to the person's Resume came to. Only a
/// Stop lifted, or one already gone, lets agents go on: a newer Stop, a lift
/// that failed or an answer not understood holds them, and says so.
fn resumption(told: &Value) -> Value {
    let lifted = told["lifted"] == "cleared" || told["lifted"] == "already-clear";
    let resumed = told["outcome"] == "resumed" && lifted;
    let mut reply = json!({
        "ok": true,
        "held": !resumed,
        "seq": told["seq"],
        "outcome": told["outcome"],
        "lifted": told["lifted"],
    });
    if !resumed {
        let why = if told["outcome"] != "resumed" {
            "The VM did not confirm this Resume; agents stay stopped.".to_owned()
        } else if told["lifted"] == "stop-held" {
            "A Stop given while this Resume waited still holds agents.".to_owned()
        } else {
            format!(
                "The Stop kept here could not be lifted ({}); agents stay stopped.",
                told["lifted"]["error"]
                    .as_str()
                    .unwrap_or("its outcome is unknown")
            )
        };
        reply["warnings"] = json!([why]);
    }
    reply
}

/// The keeper's refusal `refused` of an observation of the VM `name`, its
/// code kept; one from a keeper that does not observe says to update.
fn observe_refused(refused: &Value, name: &str) -> Error {
    let code = refused["code"].as_str().and_then(Code::parse);
    if code == Some(Code::BadRequest) && refused["capabilities"].is_array() {
        return Error::new(
            Code::Update,
            format!("{name}'s session keeper is from an older Atlas and cannot prove it."),
        )
        .with(
            "next",
            json!("Update Atlas so the keeper and this command are one build."),
        );
    }
    Error::refused(
        code.unwrap_or(Code::Transport),
        refused,
        "The VM's session keeper did not observe it.",
    )
}

fn late(client: &Client) -> Error {
    Error::new(
        Code::Transport,
        format!(
            "{} did not answer through its keeper.",
            client.selected.name
        ),
    )
}

#[cfg(test)]
#[path = "keeper_tests.rs"]
mod tests;

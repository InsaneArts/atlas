//! What a client asks of a VM desktop through its session keeper, before the
//! VM's proof: its status, never ready, and the person's Stop and Resume.
//! Everything else stays refused in the one wording for what this build
//! cannot do on a VM.
use super::{Actor, Client};
use crate::link::{Keeper, Link};
use atlas_core::{
    config::desktops::{Admission, Chosen},
    error::{Code, Error, Result},
};
use serde_json::{Value, json};

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

    /// A VM's status: its keeper's hello and its guest's own, read through
    /// the keeper, never proven, so never ready.
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
        let read = json!({"op": "read", "request": {"op": "hello"},
            "deadline_ms": keeper.timeout.as_millis()});
        let guest = match keeper.ask(&read).await {
            Ok(mut lines) => lines.pop().unwrap_or_else(|| json!(late(self))),
            Err(error) => json!(error),
        };
        // The station's own Stop, and why: an update's says so.
        let controller = self.controller.clone();
        let station = match tokio::task::spawn_blocking(move || controller.stop_revision()).await {
            Ok(Ok(Some(revision))) => json!({"state": "stopped", "why": revision.why()}),
            Ok(Ok(None)) => json!({"state": "clear"}),
            Ok(Err(_)) | Err(_) => json!({"state": "unknown"}),
        };
        Ok(json!({
            "ok": true,
            "proven": false,
            "desktop": {"kind": "vm", "name": self.selected.name},
            "keeper": hello,
            "guest": guest,
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

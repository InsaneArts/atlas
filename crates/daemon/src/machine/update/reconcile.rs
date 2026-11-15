//! Each unresolved update carried to its end by the daemon itself, apart
//! from any lane: read from the installer's current state, never from a
//! reply kept from before; released only once this daemon is verified;
//! settled only by what the installer says. It runs at the daemon's start
//! and whenever an update is asked, tried again every while, and woken when
//! its input's standing changes.
use super::{Gate, Phase, Update, Updates};
use crate::machine::{changes::Changes, vault::Vault};
use atlas_core::error::Result;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};

/// Set when this build was made never to release a guest's update, so a
/// rollback can be checked live; `scripts/release.sh` refuses to release one.
const HOLDS_RELEASE: bool = option_env!("ATLAS_TEST_HOLD_RELEASE").is_some();

/// The first wait between tries, and the longest.
const FIRST: Duration = Duration::from_secs(10);
const LONGEST: Duration = Duration::from_secs(60);

/// How input or capture stood: ready; failing only for want of the
/// person's consent; or failing otherwise.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Health {
    Ready,
    Permission,
    Other,
}

impl Health {
    /// Whether it is no worse than `before`: ready, or wanting the same
    /// consent it wanted then. Any other failure needs a success now.
    #[must_use]
    pub fn no_worse_than(self, before: Self) -> bool {
        matches!(
            (before, self),
            (_, Self::Ready) | (Self::Permission, Self::Permission)
        )
    }
}

/// What reconciliation asks of the guest, given apart so tests stand in.
pub(crate) trait System: Send + Sync {
    /// The installer's reply to `request`.
    fn ask(&self, request: &Value) -> Result<Value>;
    /// The code directory hash of what this daemon runs.
    fn own_code(&self) -> Option<Vec<u8>>;
    /// Whether input is quiescent, and how it stands.
    fn input(&self) -> (bool, Health);
    /// How one capture of the main display goes now.
    fn capture(&self) -> Health;
    /// Whether this daemon serves its station: started up, recovered, and
    /// taking connections.
    fn serving(&self) -> bool;
}

/// Whether `reply` is the installer's own refusal, kept by it for good: an
/// operational failure it reports carries an error code, and may pass.
fn refused(reply: &Value) -> bool {
    reply["ok"] == false && reply.get("code").is_none()
}

/// The daemon's reconciler of its updates.
pub struct Reconciler<V> {
    updates: Arc<Updates<V>>,
    changes: Arc<Changes<V>>,
    gate: Arc<Gate>,
    system: Arc<dyn System>,
    wake: tokio::sync::Notify,
}

impl<V: Vault + Send + Sync + 'static> Reconciler<V> {
    pub(crate) fn new(
        updates: Arc<Updates<V>>,
        changes: Arc<Changes<V>>,
        gate: Arc<Gate>,
        system: Arc<dyn System>,
    ) -> Arc<Self> {
        Arc::new(Self {
            updates,
            changes,
            gate,
            system,
            wake: tokio::sync::Notify::new(),
        })
    }

    /// Look again now, as a new update or input's standing asks.
    pub fn wake(&self) {
        self.wake.notify_one();
    }

    /// Carry every unresolved update to its end, for as long as the daemon
    /// lives.
    pub async fn run(self: Arc<Self>) {
        let mut wait = FIRST;
        loop {
            let this = self.clone();
            let open = tokio::task::spawn_blocking(move || this.round())
                .await
                .unwrap_or(true);
            if open {
                tokio::select! {
                    () = tokio::time::sleep(wait) => {}
                    () = self.wake.notified() => {}
                }
                wait = (wait * 2).min(LONGEST);
            } else {
                wait = FIRST;
                self.wake.notified().await;
            }
        }
    }

    /// One round: each unresolved update looked at once. Whether any is
    /// still open.
    pub(crate) fn round(&self) -> bool {
        let admitted = self.gate.admitted();
        let Ok(updates) = self.updates.all() else {
            return true;
        };
        // Anything that failed, a settlement told to no change among it, is
        // tried again after a while.
        let mut failed = false;
        for update in &updates {
            if update.published {
                continue;
            }
            if update.phase.settled() {
                failed |= self.publish(update).is_err();
            } else {
                failed |= self.reconcile(update).is_err();
            }
        }
        if self.updates.unresolved() {
            return true;
        }
        self.gate.resolved(admitted);
        failed
    }

    /// One update, as the installer's state says it is now.
    fn reconcile(&self, update: &Update) -> Result<()> {
        let state = self.system.ask(&json!({
            "op": "transaction",
            "transaction": update.transaction,
        }))?;
        if state["ok"] == false && state["error"] == "No such update." {
            return self.without_transaction(update);
        }
        if state["ok"] == false {
            // A failure to read it, which may pass.
            return Ok(());
        }
        self.at(update, &state)
    }

    /// The installer knows no transaction for it: what its install request
    /// came to says whether anything began.
    fn without_transaction(&self, update: &Update) -> Result<()> {
        let outcome = self
            .system
            .ask(&json!({"op": "outcome", "request": update.install}))?;
        if refused(&outcome) {
            // Refused for certain, as it keeps it: nothing began.
            let why = outcome["error"].as_str().map(str::to_owned);
            return self.settle(update, Phase::Refused, why);
        }
        if outcome.get("phase").is_some() {
            return self.at(update, &outcome);
        }
        // Never asked, or asked and cut short before its transaction: the
        // same install, which the installer continues.
        let Some(manifest) = &update.manifest else {
            return Ok(());
        };
        let answer = self.system.ask(&json!({
            "op": "install",
            "request": update.install,
            "transaction": update.transaction,
            "source": update.source,
            "manifest": manifest,
        }))?;
        if refused(&answer) {
            let why = answer["error"].as_str().map(str::to_owned);
            return self.settle(update, Phase::Refused, why);
        }
        Ok(())
    }

    /// The transaction stands at `state`.
    fn at(&self, update: &Update, state: &Value) -> Result<()> {
        let why = state["why"].as_str().map(str::to_owned);
        match state["phase"].as_str() {
            Some("released") => self.settle(update, Phase::Released, why),
            Some("rolledBack") => self.settle(update, Phase::RolledBack, why),
            Some("aborted") => self.settle(update, Phase::Refused, why),
            Some("placed") if state["restart"] == "done" && self.runs(state) => {
                if HOLDS_RELEASE || !self.verified(update) {
                    return Ok(());
                }
                let released = self.system.ask(&json!({
                    "op": "release",
                    "request": update.release,
                    "transaction": update.transaction,
                }))?;
                if released["phase"] == "released" {
                    let why = released["why"].as_str().map(str::to_owned);
                    return self.settle(update, Phase::Released, why);
                }
                Ok(())
            }
            // Placed for another build (this one before its restart), or
            // under way: it waits.
            _ => Ok(()),
        }
    }

    /// Whether this daemon runs the code `state` says was placed.
    fn runs(&self, state: &Value) -> bool {
        let Some(own) = self.system.own_code() else {
            return false;
        };
        let own: String = own.iter().map(|byte| format!("{byte:02x}")).collect();
        state["cdhash"].as_str() == Some(own.as_str())
    }

    /// Whether this daemon may release: its input quiescent, and input and
    /// capture no worse than when the update was asked.
    fn verified(&self, update: &Update) -> bool {
        let (quiescent, input) = self.system.input();
        self.system.serving()
            && quiescent
            && input.no_worse_than(update.input)
            && self.system.capture().no_worse_than(update.capture)
    }

    /// Settle the update as `phase`, then publish it to its change.
    fn settle(&self, update: &Update, phase: Phase, why: Option<String>) -> Result<()> {
        self.updates.settle(&update.change, phase, why.clone())?;
        let mut settled = update.clone();
        settled.phase = phase;
        settled.why = why;
        self.publish(&settled)
    }

    /// Publish what the settled `update` came to as its change's outcome,
    /// then mark it published: repeated, so a crash between the writes is
    /// finished by the next round. A change no longer kept was told and
    /// acknowledged.
    fn publish(&self, update: &Update) -> Result<()> {
        if matches!(
            self.changes.of(&update.change)?,
            Some(crate::machine::changes::State::Intended)
        ) {
            self.changes.done(&update.change, &outcome(update))?;
        }
        self.updates.published(&update.change)
    }
}

/// The reply a settled `update` gives as its change's outcome.
#[must_use]
pub fn outcome(update: &Update) -> Value {
    json!({
        "ok": update.phase == Phase::Released,
        "op": "update",
        "update": update.phase,
        "why": update.why,
    })
}

#[cfg(test)]
#[path = "reconcile_tests.rs"]
mod tests;

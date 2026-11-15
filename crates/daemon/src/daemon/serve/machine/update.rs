//! An update of the guest's Atlas, as its station asks it on the lane of
//! its change: admitted only while agents are stopped and idle and the
//! lane's authority is live, and kept before the installer is asked
//! anything; carried to its end by the daemon's own reconciliation, never
//! by the lane, which answers pending.
use super::Machine;
use crate::machine::{
    changes::{Asked, State},
    stop::Kept,
    update::{Admission, Phase, System as _, Update, outcome, pruned},
};
use atlas_core::{
    error::{Code, Error},
    files::Token,
    stage::Snapshot,
};
use atlas_session::{
    authority::Authority,
    lane::{LaneId, Lanes},
};
use serde_json::{Value, json};
use std::{path::PathBuf, sync::Arc};

/// What a lane is answered while its update is not settled.
fn pending(update: &Update) -> Value {
    json!({"ok": true, "op": "update", "pending": true, "update": update.phase})
}

/// The lane an update was asked on, and the session that gave it.
pub(super) struct Holding {
    pub(super) authority: Authority,
    pub(super) lanes: Arc<Lanes>,
    pub(super) lane: LaneId,
}

impl Holding {
    /// Whether the session still gives control and the lane is neither
    /// cancelled nor retired.
    fn live(&self) -> bool {
        self.authority.check().is_ok()
            && !self.lanes.cancelled(self.lane)
            && self.lanes.retired().is_none()
    }
}

/// An answer that says what it came to cannot be told.
fn uncertain(why: &str) -> Value {
    atlas_core::protocol::interrupted(Code::Uncertain, why)
}

/// What admitting an update came to.
enum Admitted {
    /// Kept: it is the reconciler's from here.
    Kept(Update),
    /// Refused before anything was written.
    Refused(String),
    /// Its record's write began and did not finish: whether it was kept
    /// cannot be told, so it holds as kept.
    Uncertain,
}

impl Machine {
    /// The update `change`, asked with `request` on the lane `lane` of
    /// `lanes` under `authority`, to the bundle staged at `staged`:
    /// answered from its record when asked before, else admitted or refused.
    pub(super) async fn update(
        &self,
        change: Token,
        staged: String,
        request: Value,
        holding: Holding,
    ) -> Value {
        let (changes, asked) = (self.paired.changes.clone(), change.clone());
        let kept =
            tokio::task::spawn_blocking(move || changes.ask(&asked, "update", &request)).await;
        match kept {
            Ok(Ok(Asked::Begun)) => {}
            Ok(Ok(Asked::Again(State::Done { reply }))) => return reply,
            Ok(Ok(Asked::Again(State::Intended))) => {
                // Asked before, with this very request: its own record says
                // how it stands.
                let (updates, asked) = (self.paired.updates.clone(), change.clone());
                match tokio::task::spawn_blocking(move || updates.of(&asked)).await {
                    Ok(Ok(Some(update))) => {
                        self.reconciler.wake();
                        if update.phase.settled() {
                            return outcome(&update);
                        }
                        return pending(&update);
                    }
                    // Never recorded: nothing reached the installer, so it
                    // is admitted now, unless that is under way already.
                    Ok(Ok(None)) => {}
                    _ => return uncertain("This update's record cannot be read"),
                }
            }
            // The same ID with another request, or a store past its bound:
            // refused for certain, as nothing could have run.
            Ok(Err(error)) if matches!(error.code, Code::BadRequest | Code::Busy) => {
                return atlas_core::protocol::refused(error);
            }
            _ => return uncertain("The change records cannot be read"),
        }
        match self.gate.begin_admission(&change) {
            Admission::Began => {}
            Admission::Already => {
                return json!({"ok": true, "op": "update", "pending": true, "update": "admitting"});
            }
            Admission::Held => {
                // Perhaps held by this very update, admitted since its record
                // was read: kept, it answers from that record.
                let (updates, asked) = (self.paired.updates.clone(), change.clone());
                return match tokio::task::spawn_blocking(move || updates.of(&asked)).await {
                    Ok(Ok(Some(update))) => {
                        self.reconciler.wake();
                        if update.phase.settled() {
                            outcome(&update)
                        } else {
                            pending(&update)
                        }
                    }
                    Ok(Ok(None)) => {
                        self.refuse(
                            &change,
                            "An update or a Resume is under way; ask again once it is done.",
                        )
                        .await
                    }
                    _ => uncertain("This update's record cannot be read"),
                };
            }
        }
        match self
            .admit(change.clone(), PathBuf::from(staged), holding)
            .await
        {
            Admitted::Kept(update) => {
                self.gate.end_admission(true);
                self.reconciler.wake();
                pending(&update)
            }
            Admitted::Refused(why) => {
                let reply = self.refuse(&change, &why).await;
                self.gate.end_admission(false);
                reply
            }
            Admitted::Uncertain => {
                self.gate.end_admission(true);
                self.reconciler.wake();
                uncertain("Whether the update was kept cannot be told; ask again")
            }
        }
    }

    /// Admit the update `change` and keep it, before anything is asked of
    /// the installer.
    async fn admit(&self, change: Token, staged: PathBuf, holding: Holding) -> Admitted {
        let stopped = "Stop agents first: an update is taken only while they are stopped.";
        if !self.daemon.halted.borrow().halted {
            return Admitted::Refused(stopped.into());
        }
        if self.daemon.keepers.keeping() || self.daemon.keepers.unsettled() {
            return Admitted::Refused("Agents' work still runs, or its end is not known.".into());
        }
        if self.jobs.as_ref().is_some_and(|jobs| !jobs.idle()) {
            return Admitted::Refused(
                "A background job still runs, or its end is not known.".into(),
            );
        }
        let guest = crate::machine::update::Guest {
            input: self.daemon.input.clone(),
            serving: self.serving.clone(),
        };
        let (stop, updates) = (self.paired.stop.clone(), self.paired.updates.clone());
        tokio::task::spawn_blocking(move || {
            if !matches!(stop.kept(), Ok(Kept::Stopped { .. })) {
                return Admitted::Refused(stopped.into());
            }
            let (quiescent, input) = guest.input();
            if !quiescent {
                return Admitted::Refused("The guest's input is not settled yet.".into());
            }
            match updates.all() {
                Ok(kept) if kept.iter().any(|update| update.change == change) => {
                    // Kept by an earlier asking whose answer was lost.
                    return kept
                        .into_iter()
                        .find(|update| update.change == change)
                        .map_or(Admitted::Uncertain, Admitted::Kept);
                }
                Ok(kept) if kept.iter().any(|update| !update.phase.settled()) => {
                    return Admitted::Refused("Another update is under way.".into());
                }
                Ok(_) => {}
                Err(_) => return Admitted::Refused("This guest's updates cannot be told.".into()),
            }
            let manifest = match Snapshot::of(&staged) {
                Ok(manifest) => manifest,
                Err(error) => return Admitted::Refused(error.error),
            };
            let update = Update {
                change,
                source: staged,
                manifest: Some(manifest),
                transaction: Token::new(),
                install: Token::new(),
                release: Token::new(),
                input,
                capture: guest.capture(),
                phase: Phase::Asked,
                why: None,
                published: false,
            };
            // The hand-off, in memory and short: from here the update is the
            // reconciler's, however its lane or its session ends.
            if !holding.live() {
                return Admitted::Refused(
                    "Its lane or its control session ended before the update was kept.".into(),
                );
            }
            let kept = update.clone();
            match updates.keep(|before| {
                let mut updates = before.to_vec();
                updates.push(kept);
                Some(pruned(updates))
            }) {
                Err(_) => Admitted::Uncertain,
                Ok(()) => Admitted::Kept(update),
            }
        })
        .await
        .unwrap_or(Admitted::Uncertain)
    }

    /// Refuse the update `change` for `why`, its change settled so.
    async fn refuse(&self, change: &Token, why: &str) -> Value {
        let reply = atlas_core::protocol::refused(Error::new(Code::BadRequest, why));
        let (changes, change, kept) = (self.paired.changes.clone(), change.clone(), reply.clone());
        let _ = tokio::task::spawn_blocking(move || changes.done(&change, &kept)).await;
        reply
    }
}

#[cfg(test)]
#[path = "update_tests.rs"]
mod tests;

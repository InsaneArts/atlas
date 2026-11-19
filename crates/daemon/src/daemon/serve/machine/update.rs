//! An update of the guest's Atlas, as its station asks it on the lane of
//! its change: admitted only while agents are stopped and idle and the
//! lane's authority is live, only of the very bundle its station staged,
//! by the manifest digest it names, and kept before the installer is asked
//! anything; carried to its end by the daemon's own reconciliation, never
//! by the lane, which answers pending.
//!
//! A change that was never asked can be voided: kept refused, so a stage
//! or an update a station never heard the answer of is never carried out
//! later under that name.
//!
//! A refusal is said as certain only once it is kept as what the change
//! came to; one that could not be kept is said uncertain. An update refused
//! as `changed` is never admitted after, by an asking that overlapped it or
//! by one that came once its refusal could not be kept: the change's
//! outcome is read again under the admission, and this daemon remembers
//! such a refusal until it is kept.
use super::Machine;
use crate::machine::{
    changes::{Asked, State, VOID, VOIDS},
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
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, PoisonError},
};

/// The updates this daemon refused as `changed` whose refusal is not kept
/// yet: each is refused so again, never admitted, until it is.
#[derive(Debug, Default)]
pub(super) struct Unkept(Mutex<BTreeSet<Token>>);

impl Unkept {
    fn lock(&self) -> std::sync::MutexGuard<'_, BTreeSet<Token>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn holds(&self, change: &Token) -> bool {
        self.lock().contains(change)
    }

    fn keep(&self, change: &Token) {
        self.lock().insert(change.clone());
    }

    fn forget(&self, change: &Token) {
        self.lock().remove(change);
    }
}

/// Where a test holds an asking of an update between reading its change's
/// outcome and taking the admission, so another asking can overlap it.
#[cfg(test)]
pub(super) mod pauses {
    use atlas_core::files::Token;
    use std::{
        collections::BTreeMap,
        sync::{Arc, Mutex, PoisonError},
    };
    use tokio::sync::Semaphore;

    /// One change's pause: a permit for each asking that reached it, and
    /// one for each the test lets go on, in the order they came.
    #[derive(Debug)]
    pub(in crate::daemon) struct Pause {
        pub(in crate::daemon) arrived: Semaphore,
        pub(in crate::daemon) going: Semaphore,
    }

    static PAUSES: Mutex<BTreeMap<Token, Arc<Pause>>> = Mutex::new(BTreeMap::new());

    /// Hold every asking of the update `change` at its pause from now.
    pub(in crate::daemon) fn hold(change: &Token) -> Arc<Pause> {
        let pause = Arc::new(Pause {
            arrived: Semaphore::new(0),
            going: Semaphore::new(0),
        });
        PAUSES
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(change.clone(), pause.clone());
        pause
    }

    /// Wait here while a test holds `change`.
    pub(super) async fn at(change: &Token) {
        let pause = PAUSES
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(change)
            .cloned();
        if let Some(pause) = pause {
            pause.arrived.add_permits(1);
            if let Ok(going) = pause.going.acquire().await {
                going.forget();
            }
        }
    }
}

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

/// What a lane is answered when the bundle staged for its update is no
/// longer the one its station staged: refused for certain, nothing
/// installed.
fn changed() -> Value {
    let mut reply = atlas_core::protocol::refused(Error::new(
        Code::State,
        "The bundle staged for this update is not the one its station staged; nothing was \
         installed.",
    ));
    reply["update"] = json!("changed");
    reply
}

/// What a lane is answered when what its update came to could not be kept:
/// it stays asked, for a retry to hear.
fn unkept() -> Value {
    uncertain("What this update came to could not be kept; ask again")
}

/// Why a staged bundle is not the one an update is bound to.
#[derive(Debug, PartialEq, Eq)]
enum Unbound {
    /// It cannot be read as a snapshot, as the words say.
    Unreadable(String),
    /// It is another bundle than its station staged.
    Changed,
}

/// The snapshot of the bundle staged at `staged`, while it is still the
/// bundle whose manifest digest is `expect`: the very bundle its station
/// staged, each file's contents as its manifest digests them.
fn bound(staged: &Path, expect: &str) -> Result<Snapshot, Unbound> {
    let manifest = Snapshot::of(staged).map_err(|error| Unbound::Unreadable(error.error))?;
    if manifest.digest == expect {
        Ok(manifest)
    } else {
        Err(Unbound::Changed)
    }
}

/// What admitting an update came to.
enum Admitted {
    /// Kept: it is the reconciler's from here.
    Kept(Update),
    /// Refused before anything was written.
    Refused(String),
    /// Refused before anything was written: its staged bundle is not the
    /// one its station staged.
    Changed,
    /// Its record's write began and did not finish: whether it was kept
    /// cannot be told, so it holds as kept.
    Uncertain,
}

impl Machine {
    /// The update `change`, asked with `request` on the lane `lane` of
    /// `lanes` under `authority`, to the bundle staged at `staged` whose
    /// manifest digest is `expect`: answered from its record when asked
    /// before, else admitted or refused.
    pub(super) async fn update(
        &self,
        change: Token,
        staged: String,
        expect: String,
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
        #[cfg(test)]
        pauses::at(&change).await;
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
        // Read again under the admission: an asking that overlapped this one
        // may have settled the change since it was read, refused as changed
        // among others, and is never admitted past.
        if let Some(settled) = self.settled_since(&change).await {
            self.gate.end_admission(false);
            return settled;
        }
        // Refused as changed already, its refusal not yet kept.
        if self.unkept.holds(&change) {
            let reply = self.refused_changed(&change).await;
            self.gate.end_admission(false);
            return reply;
        }
        match self
            .admit(change.clone(), PathBuf::from(staged), expect, holding)
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
            Admitted::Changed => {
                self.unkept.keep(&change);
                let reply = self.refused_changed(&change).await;
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

    /// Admit the update `change` of the bundle staged at `staged`, bound to
    /// the manifest digest `expect`, and keep it, before anything is asked
    /// of the installer.
    async fn admit(
        &self,
        change: Token,
        staged: PathBuf,
        expect: String,
        holding: Holding,
    ) -> Admitted {
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
            // Bound to what its station staged: anything placed there since,
            // another signed Atlas included, is refused before it is kept.
            let manifest = match bound(&staged, &expect) {
                Ok(manifest) => manifest,
                Err(Unbound::Unreadable(why)) => return Admitted::Refused(why),
                Err(Unbound::Changed) => return Admitted::Changed,
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

    /// Void the change `change`, asked with `request`: kept refused for good
    /// when it was never asked, so a stage or an update under that name is
    /// refused from then on, as one asked with another request is. One asked
    /// before is refused for certain, never voided: what it came to is its
    /// own to tell. Answered as kept, or uncertain when that cannot be told.
    pub(super) async fn void(&self, change: Token, request: Value) -> Value {
        let changes = self.paired.changes.clone();
        tokio::task::spawn_blocking(move || {
            // Each void retires its name for good: past the bound, none more.
            let full = matches!(changes.of(&change), Ok(None))
                && changes.voided().is_ok_and(|voided| voided >= VOIDS);
            if full {
                return atlas_core::protocol::refused(Error::new(
                    Code::Busy,
                    "This desktop keeps as many voided changes as it can, so this one is not \
                     voided.",
                ));
            }
            match changes.ask(&change, VOID, &request) {
                Ok(Asked::Begun | Asked::Again(State::Intended)) => {
                    let voided = json!({"ok": true, "op": "void", "voided": true});
                    match changes.done(&change, &voided) {
                        Ok(()) => voided,
                        Err(_) => {
                            uncertain("Whether the change was voided cannot be told; ask again")
                        }
                    }
                }
                Ok(Asked::Again(State::Done { reply })) => reply,
                // Asked before as something else: it may have been carried out.
                Err(error) if matches!(error.code, Code::BadRequest | Code::Busy) => {
                    atlas_core::protocol::refused(error)
                }
                Err(_) => uncertain("The change records cannot be read"),
            }
        })
        .await
        .unwrap_or_else(|_| uncertain("Whether the change was voided cannot be told; ask again"))
    }

    /// Refuse the update `change` for `why`, its change settled so.
    async fn refuse(&self, change: &Token, why: &str) -> Value {
        let reply = atlas_core::protocol::refused(Error::new(Code::BadRequest, why));
        self.refused(change, reply).await
    }

    /// Refuse the update `change` as `changed`: certain once that is kept as
    /// what it came to, and forgotten from what this daemon remembers then;
    /// uncertain, and still remembered, while it cannot be kept.
    async fn refused_changed(&self, change: &Token) -> Value {
        let reply = changed();
        if !self.kept(change, &reply).await {
            return unkept();
        }
        self.unkept.forget(change);
        reply
    }

    /// Refuse the update `change` with `reply`, said only once it is kept as
    /// what the change came to; uncertain when it cannot be, the change left
    /// as asked.
    async fn refused(&self, change: &Token, reply: Value) -> Value {
        if self.kept(change, &reply).await {
            reply
        } else {
            unkept()
        }
    }

    /// Whether `reply` is kept as what the change `change` came to.
    async fn kept(&self, change: &Token, reply: &Value) -> bool {
        let (changes, change, reply) = (self.paired.changes.clone(), change.clone(), reply.clone());
        let done = tokio::task::spawn_blocking(move || changes.done(&change, &reply)).await;
        matches!(done, Ok(Ok(())))
    }

    /// What the change `change` came to, read again: its reply once it is
    /// settled since it was last read, and a refusal that it cannot be told
    /// when its record cannot be read or is no longer kept. None while it
    /// is still only asked.
    async fn settled_since(&self, change: &Token) -> Option<Value> {
        let (changes, asked) = (self.paired.changes.clone(), change.clone());
        match tokio::task::spawn_blocking(move || changes.of(&asked)).await {
            Ok(Ok(Some(State::Intended))) => None,
            Ok(Ok(Some(State::Done { reply }))) => Some(reply),
            Ok(Ok(None)) => Some(uncertain(
                "This change was settled while it was asked again; ask under another",
            )),
            _ => Some(uncertain("The change records cannot be read")),
        }
    }
}

#[cfg(test)]
#[path = "update_tests.rs"]
mod tests;

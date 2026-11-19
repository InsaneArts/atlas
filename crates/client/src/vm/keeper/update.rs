//! The keeper's update of its VM's guest to the keeper's own build: asked
//! by a client (`update-guest`), and carried on by every generation until
//! it is done. The keeper alone stages and asks it, from the very image it
//! runs ([`super::program::Program::bundle`]), and only while the station's
//! Stop kept for an update holds agents, the guest confirmed it, and no
//! maintenance is kept; it never stops agents itself. A guest is only ever
//! brought to this keeper's build: a newer one is told to have this Mac
//! updated.
//!
//! Each step is kept in the VM's station folder first ([`record`]): every
//! change is named there, durably, before it is claimed as the guest's, and
//! every answer kept there, durably, before its claim is cleared. The
//! record's writes are the owner's alone, one at a time, and each outlives
//! its generation: the next generation's owner waits for it. Every change
//! is said under the keeper's turn, which a Resume, a maintenance's hold
//! and a handover take too, judged afresh right before it is said: this
//! keeper's own image, not handed over, the Stop, the maintenance record.
//! Each holds a ticket for its lane, so a maintenance's hold drains it.
//!
//! A change whose answer did not come is reconciled from what the guest kept
//! of it, and said again only by a keeper of the record's own build, at most
//! once each generation. A record another build began is reconciled by what
//! the guest says alone; one whose change the guest never answered is voided
//! there once it can no longer be said, so it is never carried out later.
//! The update is asked only while agents are stopped; once they resume, an
//! update not yet asked never is, and the staged copy is removed as a change
//! of the update's own. A fresh update begun in place of another build's
//! keeps that one's copy to remove.
mod record;
mod staging;

use super::{
    Keeper, Session, blocking, held,
    lanes::{Ticket, Work},
    read, recover,
};
use atlas_control::{Ownership, Pending, StopWhy, executors::Executor};
use atlas_core::{
    error::{Code, Error},
    files::Token,
    outcomes::Outcome,
};
use atlas_session::{
    compatibility::{CHANGES, COPIES, STAGE, UPDATE_BOUND, UPDATES},
    generation::Generation,
    lane::LaneKind,
};
use record::{Leftover, Phase, Record, Removal, Staged, Target};
use serde_json::{Value, json};
use std::{collections::BTreeSet, sync::Arc, time::Duration};
use tokio::sync::MutexGuard;

/// The keeper's capability to update its guest to its own build.
pub(super) const CAPABILITY: &str = "update-guest-v1";

/// What the guest must serve to be updated from this keeper.
const NEEDS: [&str; 5] = [CHANGES, STAGE, COPIES, UPDATES, UPDATE_BOUND];

/// How long one of the update's changes has to be answered, and a read of
/// what one came to.
const ASKING: Duration = Duration::from_secs(30);

/// How often an update under way is looked at again.
const LOOKING: Duration = Duration::from_secs(2);

/// How often a client asking for an update looks at how it stands.
const WATCHING: Duration = Duration::from_millis(200);

/// How long a claim, or its clearing, waits for the pending changes' lock.
const CLAIMING: Duration = Duration::from_secs(2);

/// How long the desktop's exclusion is waited for before a step is left
/// for the next round.
const EXCLUDING: Duration = Duration::from_secs(3);

/// The owner's writes of its record: one at a time, each to its end
/// wherever the work that began it goes, and the record last seen kept for
/// good, from which alone an outcome is said.
#[derive(Debug)]
pub(super) struct Writes {
    one: Arc<tokio::sync::Semaphore>,
    lasting: std::sync::Mutex<Option<Record>>,
}

impl Default for Writes {
    fn default() -> Self {
        Self {
            one: Arc::new(tokio::sync::Semaphore::new(1)),
            lasting: std::sync::Mutex::new(None),
        }
    }
}

impl Writes {
    fn kept(&self) -> std::sync::MutexGuard<'_, Option<Record>> {
        self.lasting
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn lasting(&self, record: &Record) {
        *self.kept() = Some(record.clone());
    }

    fn forget_lasting(&self) {
        *self.kept() = None;
    }

    /// Whether `record` is the one last seen kept for good.
    fn lasts(&self, record: &Record) -> bool {
        self.kept().as_ref() == Some(record)
    }
}

/// What the keeper that owns its guest's update remembers beside the
/// record, for its own life.
#[derive(Debug, Default)]
pub(super) struct Owner {
    /// The change it last said, and in which generation: none is said
    /// again in the same one.
    said: Option<(Token, Generation)>,
    /// Whether the record as read now is known to last: not until this
    /// keeper kept it itself, and not after a keeping that failed.
    durable: bool,
    /// The update the guest answered it took, in this keeper's life.
    taken: Option<Token>,
    /// The removals of staged copies this keeper began, which it may say
    /// whatever build began the update.
    begun: BTreeSet<Token>,
}

impl Owner {
    fn said_in(&self, change: &Token, generation: Generation) -> bool {
        self.said
            .as_ref()
            .is_some_and(|(said, when)| said == change && *when == generation)
    }
}

/// What one look at the update came to.
#[derive(Debug, PartialEq, Eq)]
enum Progress {
    /// A step was taken: look again at once.
    Moved,
    /// It waits on the guest, the station's Stop or a Resume.
    Waiting,
    /// No update is kept, or one is done.
    Idle,
}

/// How the station's Stop stands for the live generation's guest.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Standing {
    /// Kept for an update, and the guest applied it.
    Confirmed,
    /// Kept for an update, and the guest has not confirmed it.
    Unconfirmed,
    /// Kept for another reason, the person's own among them.
    Other,
    /// Not kept: agents resumed.
    Lifted,
    /// It cannot be read.
    Unknown,
}

/// What a change is said for, which decides when it may be.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Saying {
    /// A stage or an update of this keeper's own build, only while the
    /// Stop kept for it is confirmed.
    Update,
    /// A removal of a staged copy, only once agents resumed.
    Removal,
    /// A void, whenever no maintenance is kept.
    Void,
}

/// What an update's outcome is when it was never taken by the guest.
fn not_asked(why: &str) -> Value {
    json!({"ok": false, "op": "update", "update": "notAsked", "why": why})
}

/// What an update's outcome is when its bundle was not placed.
fn not_staged(reply: &Value) -> Value {
    json!({"ok": false, "op": "update", "update": "notStaged", "why": reply["error"]})
}

/// Whether `reply`, a final reply, says for certain what its change came to.
fn certain(reply: &Value) -> bool {
    reply["uncertain"] != true
}

/// Whether `reply` is the guest's refusal of a change asked under a name
/// already taken by another request: it says nothing of what that one came
/// to.
fn asked_otherwise(reply: &Value) -> bool {
    reply["ok"] == false
        && reply["code"] == "bad-request"
        && reply["error"]
            .as_str()
            .is_some_and(|said| said.contains("with another request"))
}

/// Whether `reply` is the guest's word that a change was voided.
fn voided(reply: &Value) -> bool {
    reply["ok"] == true && reply["op"] == "void" && reply["voided"] == true
}

/// A refusal of `update-guest`, as a client reads one.
fn refused(error: &Error) -> Value {
    serde_json::to_value(error).unwrap_or_default()
}

/// Answer a client's `update-guest`: begin bringing the guest to this
/// keeper's own build, unless an update is kept already, and say how it
/// stands once the guest took the update, or it is done, or `deadline`
/// passes. Every generation carries it on from there.
pub(super) async fn asked(keeper: &Keeper, deadline: Duration) -> Value {
    if let Some(held) = keeper.update_held().await {
        return refused(&held);
    }
    let Some(target) = keeper.target() else {
        return refused(&keeper.not_own());
    };
    if keeper.program.replaced() != Some(false) {
        return refused(&keeper.replaced());
    }
    {
        let mut owner = keeper.owner.lock().await;
        let kept = match keeper.read_record().await {
            Ok(kept) => kept,
            Err(error) => return refused(&error),
        };
        match kept {
            // This keeper's own: carried on, never begun again.
            Some(record) if record.target == target => {}
            // Another build's, waiting on the guest: reconciled by what the
            // guest says alone, and none begun until it answered.
            Some(record) if record.phase.outcome().is_none() => {
                keeper.owner_waking.notify_one();
                return json!({"ok": true, "op": "update-guest", "state": "owed",
                    "why": format!("An earlier update of {}'s Atlas is still being settled; \
                        run atlas update again once it is.", keeper.name)});
            }
            earlier => {
                let session = keeper.shared.live();
                if let Some(session) = &session {
                    keeper.reported_in(session).await;
                }
                let earlier = earlier.map(Record::left).unwrap_or_default();
                match keeper
                    .begin(session.as_deref(), target, earlier, &mut owner)
                    .await
                {
                    Ok(true) => {}
                    Ok(false) => {
                        return json!({"ok": true, "op": "update-guest", "state": "current"});
                    }
                    Err(error) => return refused(&error),
                }
            }
        }
    }
    keeper.owner_waking.notify_one();
    let until = tokio::time::Instant::now() + deadline;
    loop {
        let record = match keeper.read_record().await {
            Ok(record) => record,
            Err(error) => return refused(&error),
        };
        // Until the guest answered it took the update, or the update is
        // settled: an owner at work is looked at again later.
        let waiting = record.as_ref().is_some_and(|record| match &record.phase {
            Phase::Intended | Phase::Staging { .. } | Phase::Staged { .. } => true,
            Phase::Asked { update, .. } => keeper
                .owner
                .try_lock()
                .map_or(true, |owner| owner.taken.as_ref() != Some(update)),
            Phase::Settled { .. } | Phase::Cleaning { .. } | Phase::Done { .. } => false,
        });
        if !waiting || keeper.shared.live().is_none() || tokio::time::Instant::now() >= until {
            // An outcome is said only from a record known to last.
            let durable = record
                .as_ref()
                .is_some_and(|record| keeper.update_writes.lasts(record));
            return keeper.said(record.as_ref(), durable);
        }
        tokio::time::sleep(WATCHING).await;
    }
}

impl Keeper {
    /// Carry on the update kept for the VM in `session`, for its life:
    /// every [`LOOKING`], and at once when a client begins one.
    pub(super) async fn updating(&self, session: &Session) {
        loop {
            {
                let mut owner = self.owner.lock().await;
                while self.advance(session, &mut owner).await == Progress::Moved {}
            }
            tokio::select! {
                () = tokio::time::sleep(LOOKING) => {}
                () = self.owner_waking.notified() => {}
            }
        }
    }

    /// Begin an update of the guest of `session` to `target`, this keeper's
    /// build, keeping the copies `earlier` updates left to remove: kept as
    /// intended, before anything is claimed. Whether one was begun: none
    /// when the guest runs that build already. A VM not running has its
    /// update kept for its next generation.
    ///
    /// # Errors
    ///
    /// Refuses a guest that does not take an update from this keeper, one
    /// newer than it or whose build is not known, agents not stopped for an
    /// update and confirmed, and a record that cannot be kept.
    async fn begin(
        &self,
        session: Option<&Session>,
        target: Target,
        earlier: Vec<Leftover>,
        owner: &mut Owner,
    ) -> Result<bool, Error> {
        if let Some(session) = session {
            if !self.older(session)? {
                return Ok(false);
            }
            if self.standing().await != Standing::Confirmed {
                return Err(self.not_stopped());
            }
        }
        let record = Record {
            target,
            phase: Phase::Intended,
            earlier,
        };
        if !self.keep(owner, &record).await {
            return Err(Error::new(
                Code::Io,
                format!(
                    "The update of {}'s Atlas could not be kept, so nothing was staged.",
                    self.name
                ),
            ));
        }
        Ok(true)
    }

    /// Whether the guest of `session` runs an older build than this
    /// keeper's, which an update brings to this one; not when it runs this
    /// one.
    ///
    /// # Errors
    ///
    /// Refuses a guest that does not take an update from this keeper, and
    /// one newer than it or whose build is not known.
    fn older(&self, session: &Session) -> Result<bool, Error> {
        use crate::vm::update::Verdict;
        if let Some(refused) = self.unserved(session, &NEEDS, |name| {
            format!("{name}'s Atlas cannot be updated from this Mac, so nothing was staged.")
        }) {
            return Err(refused);
        }
        match self.update_facts(session).verdict() {
            Verdict::Older => Ok(true),
            Verdict::Current => Ok(false),
            Verdict::Newer => Err(Error::new(
                Code::Update,
                format!(
                    "{}'s Atlas is newer than this Mac's, so it is not updated.",
                    self.name
                ),
            )
            .with("next", json!("Update Atlas on this Mac."))),
            Verdict::Unknown => Err(Error::new(
                Code::Update,
                format!(
                    "Which build {}'s Atlas is cannot be told, so it is not updated.",
                    self.name
                ),
            )
            .with(
                "next",
                json!(format!("Run atlas doctor --desktop {}.", self.name)),
            )),
        }
    }

    /// Take the update kept one step on in `session`.
    async fn advance(&self, session: &Session, owner: &mut Owner) -> Progress {
        let record = match self.read_record().await {
            Ok(Some(record)) => record,
            Ok(None) => return Progress::Idle,
            // Unreadable: nothing it may name is claimed, cleared or asked.
            Err(_) => return Progress::Waiting,
        };
        // What it says now may be a keeping whose sync failed, here or in
        // the keeper before: it is made to last before anything relies on it.
        if !owner.durable && !self.keep(owner, &record).await {
            return Progress::Waiting;
        }
        self.clear_leftovers(&record).await;
        if self.earlier_step(session, owner, &record).await {
            return Progress::Moved;
        }
        let own = self.own(&record.target);
        let generation = session.control.generation();
        match record.phase.clone() {
            Phase::Intended => self.stage(session, owner, record, own).await,
            Phase::Staging { stage, digest } => {
                let never = || not_staged(&json!({"error": "The guest never took it."}));
                // A void the guest kept, its reply lost, is a void.
                let mut answer = self
                    .answered(session, &stage)
                    .await
                    .map(|answer| if voided(&answer) { never() } else { answer });
                let mut void = false;
                if answer.is_none() && !owner.said_in(&stage, generation) {
                    let standing = self.standing().await;
                    if own && standing == Standing::Confirmed {
                        match self
                            .staged(session, owner, &record, (&stage, digest.as_deref()))
                            .await
                        {
                            Sent::Answered(reply) => answer = Some(reply),
                            // Bound to another snapshot: never said again.
                            Sent::Unbound => void = true,
                            Sent::Unsent | Sent::Lost => {}
                        }
                    } else if !own || standing == Standing::Lifted {
                        void = true;
                    }
                }
                // It can no longer be said: once the guest never took it, it
                // is voided there, so none comes later.
                if void {
                    answer = self.voided(session, owner, &stage).await.map(|()| never());
                }
                let Some(answer) = answer else {
                    return Progress::Waiting;
                };
                self.stage_answered(owner, record, stage, &answer).await
            }
            Phase::Staged { stage, staged } => {
                let standing = self.standing().await;
                if !own || standing == Standing::Lifted {
                    let why = if own {
                        "Agents resumed before the update was asked."
                    } else {
                        "A keeper of another build began this update."
                    };
                    let next = Record {
                        phase: Phase::Settled {
                            stage,
                            update: None,
                            outcome: not_asked(why),
                        },
                        ..record
                    };
                    return self.moved(owner, &next).await;
                }
                if standing != Standing::Confirmed || self.update_held().await.is_some() {
                    return Progress::Waiting;
                }
                let next = Record {
                    phase: Phase::Asked {
                        stage,
                        staged,
                        update: Token::new(),
                    },
                    ..record
                };
                self.moved(owner, &next).await
            }
            Phase::Asked {
                stage,
                staged,
                update,
            } => {
                let never = || not_asked("The guest never took it; it is void there.");
                // A void the guest kept, its reply lost, is a void.
                let mut answer = self
                    .answered(session, &update)
                    .await
                    .map(|answer| if voided(&answer) { never() } else { answer });
                if answer.is_none() && !owner.said_in(&update, generation) {
                    let standing = self.standing().await;
                    if own && standing == Standing::Confirmed {
                        let request = json!({"op": "update", "change": update,
                            "staged": staged.path, "expect": staged.digest});
                        let said = self
                            .said_change(
                                session,
                                owner,
                                (&update, &request),
                                (Saying::Update, Some(&record.target)),
                            )
                            .await;
                        match said {
                            Sent::Answered(reply) if reply["pending"] == true => {
                                owner.taken = Some(update.clone());
                            }
                            Sent::Answered(reply) if !asked_otherwise(&reply) => {
                                answer = Some(reply);
                            }
                            Sent::Answered(_) | Sent::Unsent | Sent::Lost | Sent::Unbound => {}
                        }
                    } else if !own || standing == Standing::Lifted {
                        answer = self.voided(session, owner, &update).await.map(|()| never());
                    }
                }
                let Some(outcome) = answer else {
                    return Progress::Waiting;
                };
                let next = Record {
                    phase: Phase::Settled {
                        stage,
                        update: Some(update.clone()),
                        outcome,
                    },
                    ..record
                };
                self.answered_and_cleared(owner, &next, &[update]).await
            }
            Phase::Settled {
                stage,
                update,
                outcome,
            } => {
                // A copy staged while agents are stopped is removed only
                // once they resume, and never during a maintenance.
                if self.standing().await != Standing::Lifted || self.update_held().await.is_some() {
                    return Progress::Waiting;
                }
                let issued = record.phase.issued();
                let node = match self.node_of(session, &stage).await {
                    Ok(Some(node)) => node,
                    Ok(None) => {
                        let next = Record {
                            phase: Phase::Done { issued, outcome },
                            ..record
                        };
                        return self.moved(owner, &next).await;
                    }
                    Err(()) => return Progress::Waiting,
                };
                let remove = Token::new();
                owner.begun.insert(remove.clone());
                let next = Record {
                    phase: Phase::Cleaning {
                        stage,
                        update,
                        outcome,
                        node,
                        remove,
                    },
                    ..record
                };
                self.moved(owner, &next).await
            }
            Phase::Cleaning {
                stage,
                update,
                outcome,
                node,
                remove,
            } => {
                let removal = Removal {
                    node: node.clone(),
                    remove: remove.clone(),
                };
                match self.removed(session, owner, (&stage, &removal), own).await {
                    Removed::Answered(removed) => {
                        let mut outcome = outcome;
                        outcome["cleanup"] = removed["outcome"].clone();
                        let next = Record {
                            phase: Phase::Done {
                                issued: record.phase.issued(),
                                outcome,
                            },
                            ..record
                        };
                        self.answered_and_cleared(owner, &next, &[remove]).await
                    }
                    // Never taken: the copy is still there, for a removal
                    // of this keeper's own.
                    Removed::Void => {
                        let next = Record {
                            phase: Phase::Settled {
                                stage,
                                update,
                                outcome,
                            },
                            ..record
                        };
                        self.answered_and_cleared(owner, &next, &[remove]).await
                    }
                    Removed::Waiting => Progress::Waiting,
                }
            }
            Phase::Done { .. } => Progress::Idle,
        }
    }

    /// Take the first copy an earlier update left one step on, once agents
    /// resumed and no maintenance is kept: whether a step was taken.
    async fn earlier_step(&self, session: &Session, owner: &mut Owner, record: &Record) -> bool {
        let Some(left) = record.earlier.first().cloned() else {
            return false;
        };
        if self.standing().await != Standing::Lifted || self.update_held().await.is_some() {
            return false;
        }
        let mut next = record.clone();
        match &left.removal {
            None => match self.node_of(session, &left.stage).await {
                Ok(None) => {
                    next.earlier.remove(0);
                }
                Ok(Some(node)) => {
                    let remove = Token::new();
                    owner.begun.insert(remove.clone());
                    next.earlier[0].removal = Some(Removal { node, remove });
                }
                Err(()) => return false,
            },
            Some(removal) => {
                match self
                    .removed(session, owner, (&left.stage, removal), false)
                    .await
                {
                    Removed::Answered(_) => {
                        next.earlier.remove(0);
                        return self
                            .answered_and_cleared(
                                owner,
                                &next,
                                &[left.stage.clone(), removal.remove.clone()],
                            )
                            .await
                            == Progress::Moved;
                    }
                    Removed::Void => {
                        next.earlier[0].removal = None;
                        return self
                            .answered_and_cleared(
                                owner,
                                &next,
                                std::slice::from_ref(&removal.remove),
                            )
                            .await
                            == Progress::Moved;
                    }
                    Removed::Waiting => return false,
                }
            }
        }
        self.moved(owner, &next).await == Progress::Moved
    }

    /// What the removal `removal` of the copy `stage` came to: said by a
    /// keeper of the update's build (`own`), or by the one that began it,
    /// once a generation; voided once neither may say it. A removal the
    /// guest kept voided, its reply lost, is a void too: the copy stays.
    async fn removed(
        &self,
        session: &Session,
        owner: &mut Owner,
        (stage, removal): (&Token, &Removal),
        own: bool,
    ) -> Removed {
        let remove = &removal.remove;
        if let Some(answer) = self.answered(session, remove).await {
            return if voided(&answer) {
                Removed::Void
            } else {
                Removed::Answered(answer)
            };
        }
        if owner.said_in(remove, session.control.generation()) {
            return Removed::Waiting;
        }
        if own || owner.begun.contains(remove) {
            let request = json!({"op": "remove-copy", "change": remove, "copy": stage,
                "node": removal.node});
            return match self
                .said_change(session, owner, (remove, &request), (Saying::Removal, None))
                .await
            {
                Sent::Answered(answer) => Removed::Answered(answer),
                Sent::Unsent | Sent::Lost | Sent::Unbound => Removed::Waiting,
            };
        }
        match self.voided(session, owner, remove).await {
            Some(()) => Removed::Void,
            None => Removed::Waiting,
        }
    }

    /// Begin the stage of the update `record` keeps as intended, in
    /// `session`: dropped instead when agents resumed, the record is another
    /// build's, or the guest says it runs this build or a newer one, as
    /// nothing of it was claimed or asked; the copies earlier updates left
    /// stay kept. A guest whose build is not known yet is waited for.
    async fn stage(
        &self,
        session: &Session,
        owner: &mut Owner,
        record: Record,
        own: bool,
    ) -> Progress {
        use crate::vm::update::Verdict;
        let standing = self.standing().await;
        let verdict = self.update_facts(session).verdict();
        if verdict == Verdict::Unknown && own && standing != Standing::Lifted {
            return Progress::Waiting;
        }
        if !own || standing == Standing::Lifted || verdict != Verdict::Older {
            if !record.earlier.is_empty() {
                let next = Record {
                    phase: Phase::Done {
                        issued: Vec::new(),
                        outcome: not_asked("Nothing of this update was asked."),
                    },
                    ..record
                };
                return self.moved(owner, &next).await;
            }
            return if self.forget_record().await {
                Progress::Moved
            } else {
                Progress::Waiting
            };
        }
        if standing != Standing::Confirmed || self.update_held().await.is_some() {
            return Progress::Waiting;
        }
        let next = Record {
            phase: Phase::Staging {
                stage: Token::new(),
                digest: None,
            },
            ..record
        };
        self.moved(owner, &next).await
    }

    /// Stage this keeper's own bundle as `stage` of the update `record`
    /// keeps, in `session`: its snapshot bound to `stage` in the record
    /// first, once and for good (`bound`, when it was before), and never
    /// said under that name with another; said under the keeper's turn and
    /// the desktop's exclusion, both judged again, the exclusion held until
    /// the guest's first answer.
    async fn staged(
        &self,
        session: &Session,
        owner: &mut Owner,
        record: &Record,
        (stage, bound): (&Token, Option<&str>),
    ) -> Sent {
        let program = self.program.clone();
        let held = match tokio::task::spawn_blocking(move || program.bundle()).await {
            Ok(Ok((_, held))) => held,
            // Never staged, as nothing claimed it: it never will be from
            // this bundle, which is not the image this keeper runs.
            Ok(Err(super::program::Unheld::NotOwn)) if !self.claimed_now(stage).await => {
                return Sent::Answered(
                    serde_json::to_value(self.not_own().certain()).unwrap_or_default(),
                );
            }
            Ok(Err(_)) | Err(_) => return Sent::Unsent,
        };
        let digest = held.snapshot().digest.clone();
        match bound {
            Some(bound) if bound != digest => return Sent::Unbound,
            Some(_) => {}
            None => {
                let next = Record {
                    phase: Phase::Staging {
                        stage: stage.clone(),
                        digest: Some(digest),
                    },
                    ..record.clone()
                };
                if !self.keep(owner, &next).await {
                    return Sent::Unsent;
                }
            }
        }
        let target = Some(&record.target);
        let Some((turn, ticket)) = self.authorized(session, target, Saying::Update).await else {
            return Sent::Unsent;
        };
        let Some(exclusion) = self.excluded(Saying::Update, target).await else {
            return Sent::Unsent;
        };
        let Some(_claim) = self.claim(stage).await else {
            return Sent::Unsent;
        };
        session.carried.keep(stage.clone());
        owner.said = Some((stage.clone(), session.control.generation()));
        let still = || self.still(Saying::Update, target);
        let sent = staging::staged(
            session,
            (stage, Arc::new(held)),
            (turn, exclusion, &ticket),
            &still,
        )
        .await;
        if sent == staging::Staging::Unsent {
            owner.said = None;
        }
        match sent {
            staging::Staging::Answered(answer) if !asked_otherwise(&answer) => {
                Sent::Answered(answer)
            }
            staging::Staging::Unsent => Sent::Unsent,
            staging::Staging::Answered(_) | staging::Staging::Lost => Sent::Lost,
        }
    }

    /// What the guest answered of the stage `stage`, kept in `record` with
    /// its next step before its claim is cleared.
    async fn stage_answered(
        &self,
        owner: &mut Owner,
        record: Record,
        stage: Token,
        answer: &Value,
    ) -> Progress {
        let placed = answer["ok"] == true
            && answer["path"].as_str().is_some_and(|path| !path.is_empty())
            && answer["digest"].is_string();
        let phase = if placed {
            Phase::Staged {
                stage: stage.clone(),
                staged: Staged {
                    path: answer["path"].as_str().unwrap_or_default().to_owned(),
                    digest: answer["digest"].as_str().unwrap_or_default().to_owned(),
                },
            }
        } else if certain(answer) {
            Phase::Done {
                issued: vec![stage.clone()],
                outcome: if answer["op"] == "update" {
                    answer.clone()
                } else {
                    not_staged(answer)
                },
            }
        } else {
            return Progress::Waiting;
        };
        let next = Record { phase, ..record };
        self.answered_and_cleared(owner, &next, &[stage]).await
    }

    /// Void `change` in `session`, a change of the update's whose answer the
    /// guest never gave: whether the guest never took it, so it never will.
    /// None while that is not known: it took it, or the void was not
    /// answered.
    async fn voided(&self, session: &Session, owner: &mut Owner, change: &Token) -> Option<()> {
        let request = json!({"op": "void", "change": change});
        match self
            .said_change(session, owner, (change, &request), (Saying::Void, None))
            .await
        {
            Sent::Answered(reply) => voided(&reply).then_some(()),
            Sent::Unsent | Sent::Lost | Sent::Unbound => None,
        }
    }

    /// Say the change `change`, `request`, in `session`, for `saying` (an
    /// update to `target`): under the keeper's turn and the desktop's
    /// exclusion, its ticket taken, claimed as the guest's first, and judged
    /// again once its lane is open, right before it is written. Its final
    /// reply when it is certain.
    async fn said_change(
        &self,
        session: &Session,
        owner: &mut Owner,
        (change, request): (&Token, &Value),
        (saying, target): (Saying, Option<&Target>),
    ) -> Sent {
        let Some((_turn, ticket)) = self.authorized(session, target, saying).await else {
            return Sent::Unsent;
        };
        let Some(_exclusion) = self.excluded(saying, target).await else {
            return Sent::Unsent;
        };
        let Some(_claim) = self.claim(change).await else {
            return Sent::Unsent;
        };
        let kind = LaneKind::Change {
            change: change.clone(),
        };
        let Ok(Ok(opened)) =
            tokio::time::timeout(super::change::OPENING, session.desktop.open(kind)).await
        else {
            return Sent::Unsent;
        };
        if !self.still(saying, target) {
            session.desktop.cancel(opened.lane);
            return Sent::Unsent;
        }
        if saying != Saying::Void {
            session.carried.keep(change.clone());
        }
        owner.said = Some((change.clone(), session.control.generation()));
        let mut last = None;
        let carried = tokio::select! {
            carried = tokio::time::timeout(ASKING, super::carry(opened.stream, request, |line| {
                last = Some(line);
                true
            })) => carried.ok().and_then(Result::ok),
            () = ticket.stopped() => None,
        };
        if carried != Some(true) {
            session.desktop.cancel(opened.lane);
            return Sent::Lost;
        }
        match last.and_then(|line| serde_json::from_str::<Value>(&line).ok()) {
            Some(reply) if certain(&reply) => Sent::Answered(reply),
            Some(_) | None => Sent::Lost,
        }
    }

    /// The keeper's turn and a ticket for update work, while what `saying`
    /// says may be said in `session` now, judged afresh under the turn: not
    /// handed over, no maintenance kept (its record read again), a guest
    /// that takes updates, and for a stage or an update this keeper's own
    /// image, not replaced, building `target`, the Stop kept for it
    /// confirmed; for a removal, agents resumed. None otherwise.
    async fn authorized(
        &self,
        session: &Session,
        target: Option<&Target>,
        saying: Saying,
    ) -> Option<(MutexGuard<'_, ()>, Ticket<'_>)> {
        let turn = self.turn.lock().await;
        if self.handing.load(std::sync::atomic::Ordering::SeqCst)
            || self.unserved(session, &NEEDS, |_| String::new()).is_some()
        {
            return None;
        }
        let ticket = held::admitted(self, Work::Update).await.ok()?;
        self.may(saying, target).await.then_some((turn, ticket))
    }

    /// The desktop's exclusion, which the station's update and the person's
    /// maintenance take too, waited for a little: none while one of them
    /// holds it, the step left for the next round. Under it, what `saying`
    /// says is judged again, the maintenance record read afresh.
    async fn excluded(&self, saying: Saying, target: Option<&Target>) -> Option<Ownership> {
        let stop = self.stop.clone().waiting(EXCLUDING);
        let exclusion = blocking(move || stop.acquire_for_executor()).await.ok()?;
        (self.update_held().await.is_none() && self.may(saying, target).await).then_some(exclusion)
    }

    /// Whether what `saying` says may be said now: not handed over, and for
    /// a stage or an update this keeper's own image building `target` with
    /// the Stop kept for it confirmed; for a removal, agents resumed.
    async fn may(&self, saying: Saying, target: Option<&Target>) -> bool {
        if !self.still(saying, target) {
            return false;
        }
        match saying {
            Saying::Update => self.standing().await == Standing::Confirmed,
            Saying::Removal => self.standing().await == Standing::Lifted,
            Saying::Void => true,
        }
    }

    /// Whether what `saying` says may still be said, judged at once with no
    /// wait: not handed over, and for a stage or an update this keeper's
    /// own image, not replaced, building `target`.
    fn still(&self, saying: Saying, target: Option<&Target>) -> bool {
        !self.handing.load(std::sync::atomic::Ordering::SeqCst)
            && (saying != Saying::Update || target.is_some_and(|target| self.own(target)))
    }

    /// Whether this keeper may say an update to `target`: its own build, and
    /// its program not replaced, nor which file it runs from unknown.
    fn own(&self, target: &Target) -> bool {
        self.target().as_ref() == Some(target) && self.program.replaced() == Some(false)
    }

    /// What the guest of `session` kept of `change`, when it is certain.
    async fn answered(&self, session: &Session, change: &Token) -> Option<Value> {
        let answer = recover::outcome(session, change).await?;
        Outcome::remembered(&answer).filter(certain)
    }

    /// The folder node of the copy `stage` placed, as the guest lists it:
    /// none when it lists no such copy.
    async fn node_of(&self, session: &Session, stage: &Token) -> Result<Option<String>, ()> {
        let mut last = None;
        let asked = json!({"op": "copies", "copy": stage});
        read(session, LaneKind::Read, &asked, ASKING, |line| {
            last = Some(line);
            true
        })
        .await
        .map_err(|_| ())?;
        let listed: Value = last
            .and_then(|line| serde_json::from_str(&line).ok())
            .ok_or(())?;
        if listed["ok"] != true {
            return Err(());
        }
        Ok(listed["copies"].as_array().and_then(|copies| {
            copies
                .iter()
                .find(|copy| copy["copy"] == json!(stage))
                .and_then(|copy| copy["node"].as_str().map(str::to_owned))
        }))
    }

    /// Keep `next`, then clear the claims of `changes` whose answers it
    /// keeps: each claim stays while the keeping may not last.
    async fn answered_and_cleared(
        &self,
        owner: &mut Owner,
        next: &Record,
        changes: &[Token],
    ) -> Progress {
        if !self.keep(owner, next).await {
            return Progress::Waiting;
        }
        for change in changes {
            self.clear_claim(change).await;
        }
        Progress::Moved
    }

    /// Keep `next`: moved on once it is kept.
    async fn moved(&self, owner: &mut Owner, next: &Record) -> Progress {
        if self.keep(owner, next).await {
            Progress::Moved
        } else {
            Progress::Waiting
        }
    }

    /// The update kept, read once no write of it is under way.
    async fn read_record(&self) -> atlas_core::error::Result<Option<Record>> {
        let writing = self.update_writes.one.clone().acquire_owned().await;
        let folder = self.folder.clone();
        blocking(move || {
            let _writing = writing;
            Record::kept(&folder)
        })
        .await
    }

    /// Keep `record` durably: whether it lasts. Its write holds the
    /// owner's writing to its end, wherever this keeper's work goes
    /// meanwhile, so no write of a generation that ended lands after one
    /// of the next.
    async fn keep(&self, owner: &mut Owner, record: &Record) -> bool {
        // Not known to last from the moment a write may begin, whatever ends
        // this keeper's work before it is seen to succeed.
        owner.durable = false;
        self.update_writes.forget_lasting();
        let writing = self.update_writes.one.clone().acquire_owned().await;
        let (folder, kept) = (self.folder.clone(), record.clone());
        let lasts = blocking(move || {
            let _writing = writing;
            kept.keep(&folder)
        })
        .await
        .is_ok();
        if lasts {
            self.update_writes.lasting(record);
        }
        owner.durable = lasts;
        lasts
    }

    /// Forget the update kept, as [`Keeper::keep`] keeps one: whether it
    /// is gone for good.
    async fn forget_record(&self) -> bool {
        let writing = self.update_writes.one.clone().acquire_owned().await;
        let folder = self.folder.clone();
        blocking(move || {
            let _writing = writing;
            Record::forget(&folder)
        })
        .await
        .is_ok()
    }

    /// Clear the claims of every change `record` asked that it no longer
    /// waits on, its answer kept already: one whose clearing failed before.
    async fn clear_leftovers(&self, record: &Record) {
        let stop = self.stop.clone();
        let Ok(pending) = blocking(move || stop.pending_within(Duration::ZERO)).await else {
            return;
        };
        let open = record.phase.open().cloned();
        let answered = record
            .phase
            .issued()
            .into_iter()
            .filter(|change| Some(change) != open.as_ref())
            .chain(record.earlier.iter().map(|left| left.stage.clone()));
        for change in answered {
            if pending.iter().any(|claim| claim == change.as_str()) {
                self.clear_claim(&change).await;
            }
        }
    }

    /// Wait, within the time a report is given, until the guest of
    /// `session` reported in that generation, which says its build.
    async fn reported_in(&self, session: &Session) {
        let generation = session.control.generation();
        let _ = tokio::time::timeout(super::REPORT, async {
            while self.shared.state().reported(generation).is_none() {
                tokio::time::sleep(WATCHING).await;
            }
        })
        .await;
    }

    /// Whether `change` is claimed now; a claim that cannot be read counts
    /// as one.
    async fn claimed_now(&self, change: &Token) -> bool {
        let stop = self.stop.clone();
        blocking(move || stop.pending_within(CLAIMING))
            .await
            .map_or(true, |pending| {
                pending.iter().any(|claim| claim == change.as_str())
            })
    }

    /// Claim `change` as the guest's.
    async fn claim(&self, change: &Token) -> Option<Pending> {
        let (stop, change) = (self.stop.clone(), change.as_str().to_owned());
        blocking(move || stop.quarantine_by_within(&change, Executor::Guest, CLAIMING))
            .await
            .ok()
    }

    /// Clear the claim of `change`: whether it is cleared.
    async fn clear_claim(&self, change: &Token) -> bool {
        let (stop, change) = (self.stop.clone(), change.as_str().to_owned());
        blocking(move || stop.clear_if_within(&change, CLAIMING))
            .await
            .is_ok()
    }

    /// How the station's Stop stands for the live generation's guest.
    async fn standing(&self) -> Standing {
        let stop = self.stop.clone();
        match blocking(move || stop.stop_revision()).await {
            Ok(None) => Standing::Lifted,
            Ok(Some(revision)) if revision.why() != StopWhy::Update => Standing::Other,
            Ok(Some(revision)) if self.applied().revision.as_ref() == Some(&revision) => {
                Standing::Confirmed
            }
            Ok(Some(_)) => Standing::Unconfirmed,
            Err(_) => Standing::Unknown,
        }
    }

    /// Why nothing of an update is asked now: a maintenance kept, ending or
    /// unreadable. None otherwise.
    async fn update_held(&self) -> Option<Error> {
        let (kept, _) = held::kept(self).await;
        kept.update_refused(&self.name)
    }

    /// The build this keeper runs, as an update's target: none when its
    /// digest was not read.
    fn target(&self) -> Option<Target> {
        let build = self.program.build();
        Some(Target {
            build: build.digest?,
            version: build.version?,
        })
    }

    fn not_stopped(&self) -> Error {
        Error::new(
            Code::Stopped,
            format!(
                "Agents on {} are not stopped for an update, so its Atlas is not updated.",
                self.name
            ),
        )
        .with("next", json!("Run atlas update to stop them for it."))
    }

    fn replaced(&self) -> Error {
        Error::new(
            Code::Busy,
            format!(
                "This keeper runs a build Atlas has since replaced; the new keeper updates {}.",
                self.name
            ),
        )
    }

    fn not_own(&self) -> Error {
        Error::new(
            Code::Update,
            "The Atlas on this Mac is not the build this keeper runs, so nothing was staged.",
        )
    }

    /// What `update-guest` answers of `record`, the update kept now, and
    /// whether it is known to last: an outcome only once it is.
    fn said(&self, record: Option<&Record>, durable: bool) -> Value {
        let state = |state: &str| json!({"ok": true, "op": "update-guest", "state": state});
        let Some(record) = record else {
            return state("current");
        };
        let current = self.shared.live().is_some_and(|session| {
            self.update_facts(&session).verdict() == crate::vm::update::Verdict::Current
        });
        match record.phase.outcome().filter(|_| durable) {
            // Released, and its guest's next generation says it runs this
            // keeper's build.
            Some(outcome) if outcome["ok"] == true && current => {
                let mut said = state("updated");
                said["outcome"] = outcome.clone();
                said
            }
            Some(outcome) if outcome["ok"] == true => {
                let mut said = state("refused");
                said["why"] = json!(format!(
                    "{}'s Atlas was updated, but it does not say it runs this keeper's build.",
                    self.name
                ));
                said["outcome"] = outcome.clone();
                said
            }
            Some(outcome) => {
                let mut said = state("refused");
                said["why"] = if outcome["why"].is_string() {
                    outcome["why"].clone()
                } else {
                    outcome["error"].clone()
                };
                said["outcome"] = outcome.clone();
                said
            }
            None if !matches!(
                record.phase,
                Phase::Intended | Phase::Staging { .. } | Phase::Staged { .. }
            ) =>
            {
                state("updating")
            }
            None => {
                let mut said = state("owed");
                said["why"] = json!(format!(
                    "{}'s Atlas is updated once it runs and its agents are stopped for it.",
                    self.name
                ));
                said
            }
        }
    }
}

/// What saying one of the update's changes came to.
#[derive(Debug)]
enum Sent {
    /// The guest's first answer, certain: pending, or what it came to.
    Answered(Value),
    /// Nothing of it went: its lane never opened, or it was not judged
    /// sayable, so it may be said again, or voided, at once.
    Unsent,
    /// It may have gone, and no certain answer came.
    Lost,
    /// A stage bound to another snapshot than the bundle's now: never said
    /// under its name.
    Unbound,
}

/// What a removal of a staged copy came to.
enum Removed {
    /// The guest's answer, certain.
    Answered(Value),
    /// The guest never took it, and now never will.
    Void,
    /// Not known yet.
    Waiting,
}

/// Whether an update kept in the station folder `folder` has a stage or an
/// update the guest may still take: staging, staged or asked, or a record
/// that cannot be read, which may be any of them.
pub(crate) fn unsettled(folder: &std::path::Path) -> bool {
    match Record::kept(folder) {
        Ok(Some(record)) => matches!(
            record.phase,
            Phase::Staging { .. } | Phase::Staged { .. } | Phase::Asked { .. }
        ),
        Ok(None) => false,
        Err(_) => true,
    }
}

/// The changes an update kept in `folder` names, which only its owner
/// settles; an error when the record cannot be read, which may name any.
pub(super) fn owned(folder: &std::path::Path) -> atlas_core::error::Result<Vec<Token>> {
    Ok(Record::kept(folder)?
        .map(|record| record.issued())
        .unwrap_or_default())
}

#[cfg(test)]
#[path = "update_tests.rs"]
mod tests;

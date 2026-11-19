//! The keeper's recovery of the station's changes on its VM whose command
//! ended without an answer: at each hello and every [`RECOVERING`], each
//! change the guest carries out that no live process holds the claim of is
//! asked of the guest. Only a certain answer, or one the guest proves ended,
//! clears its claim, and the guest is then told to forget it. An uncertain
//! answer waits for that proof; anything else stays pending as it is. A
//! claim another executor carries out (Atlas VM, the station) is never
//! looked at, nor is one an update of the guest's Atlas names: its owner
//! keeps each answer before it clears a claim ([`super::update`]). While
//! that update's record cannot be read, nothing is.
use super::{Keeper, Session, change::live_in, read};
use atlas_control::executors::Executor;
use atlas_core::{
    error::{Code, Error},
    files::Token,
    outcomes::Outcome,
};
use atlas_session::{
    compatibility::{CHANGES, ENDED, judge},
    lane::LaneKind,
};
use serde_json::{Value, json};
use std::time::Duration;

/// The keeper's capability to recover its VM's changes when asked.
pub(super) const CAPABILITY: &str = "recover-v1";

/// How often the changes no command holds are looked at again.
const RECOVERING: Duration = Duration::from_secs(10);

/// How long the guest has to say what one change came to.
const ASKING: Duration = Duration::from_secs(10);

/// The most changes one recovery asks after; the rest wait for the next.
const ROUND: usize = 64;

/// The longest an asked recovery waits to read the station's pending
/// changes, which another process may hold for a moment.
const READING: Duration = Duration::from_secs(2);

/// What one recovery came to: the changes whose claims it cleared, and
/// the claims it left pending.
#[derive(Debug, Default)]
pub(super) struct Recovered {
    pub(super) cleared: Vec<Token>,
    pub(super) kept: Vec<String>,
}

impl Keeper {
    /// Recover in `session` the changes its guest carries out whose claim
    /// no live process holds, waiting at most `wait` to read the station's
    /// pending changes: none when they cannot be read by then, which
    /// recovers nothing. A change whose final reply a test build kept back
    /// is recovered only when the keeper is `asked` to.
    pub(super) async fn recovered(
        &self,
        session: &Session,
        wait: Duration,
        asked: bool,
    ) -> Option<Recovered> {
        let _one = self.recovering.lock().await;
        let stop = self.stop.clone();
        let unheld = super::blocking(move || stop.unheld_by(Executor::Guest, wait))
            .await
            .ok()?;
        // An update's changes are its owner's to settle, and a record of
        // one that cannot be read may name any: none is looked at then.
        let folder = self.folder.clone();
        let Ok(owned) = super::blocking(move || super::update::owned(&folder)).await else {
            return Some(Recovered {
                cleared: Vec::new(),
                kept: unheld,
            });
        };
        let (owned, unheld): (Vec<String>, Vec<String>) = unheld
            .into_iter()
            .partition(|claim| owned.iter().any(|change| change.as_str() == claim));
        let declared = session.control.declared();
        if judge(declared, &[CHANGES]).refusal().is_some() {
            return Some(Recovered {
                cleared: Vec::new(),
                kept: owned.into_iter().chain(unheld).collect(),
            });
        }
        let proves_ended = judge(declared, &[ENDED]).refusal().is_none();
        let mut recovered = Recovered {
            cleared: Vec::new(),
            kept: owned,
        };
        for (index, claim) in unheld.into_iter().enumerate() {
            let change = Token::parse(&claim)
                .filter(|_| index < ROUND)
                .filter(|_| asked || !self.kept_back.holds(&claim));
            let settled = match &change {
                Some(change) => settles(outcome(session, change).await.as_ref(), proves_ended),
                None => false,
            };
            // Told to the guest as every change carried is, once its claim
            // reads cleared, whatever cuts this short.
            if let Some(change) = change.filter(|_| settled) {
                session.carried.keep(change.clone());
                if self.cleared(&claim).await {
                    self.kept_back.forget(&change);
                    recovered.cleared.push(change);
                    continue;
                }
            }
            recovered.kept.push(claim);
        }
        self.leftover_resume(session, &mut recovered).await;
        self.settling(session).await;
        Some(recovered)
    }

    /// Settle the add's own Resume left claimed by an add cut short once
    /// the VM was added: by the guest's word on it, once it came to anything
    /// or can no longer be carried out; kept claimed while that word is not
    /// known, never dropped. It is the station's own claim, so no other
    /// recovery looks at it.
    async fn leftover_resume(&self, session: &Session, recovered: &mut Recovered) {
        let (lifecycle, vm, stop) = (self.lifecycle.clone(), self.vm.clone(), self.stop.clone());
        let left = super::blocking(move || {
            let record = lifecycle.record(&vm)?;
            let change = record
                .filter(|record| record.state == crate::vm::lifecycle::record::State::Created)
                .and_then(|record| record.setup)
                .and_then(|setup| setup.resume);
            let pending = stop.pending_within(Duration::ZERO)?;
            Ok(change.filter(|change| pending.iter().any(|claim| claim == change.as_str())))
        })
        .await;
        let Ok(Some(change)) = left else {
            return;
        };
        let known = json!({
            "ok": true,
            "said": super::clients::said_in(session, &change),
            "outcome": outcome(session, &change).await,
        });
        let settles = crate::vm::add::word_of(&known)
            .is_some_and(|word| word != crate::vm::add::Word::Pending);
        if settles {
            session.carried.keep(change.clone());
            if self.cleared(change.as_str()).await {
                recovered.cleared.push(change);
                return;
            }
        }
        recovered.kept.push(change.as_str().to_owned());
    }

    /// Recover in `session` now, then every [`RECOVERING`], for its life.
    pub(super) async fn recovering(&self, session: &Session) {
        loop {
            // Never waiting: a busy round leaves it to the next.
            let _ = self.recovered(session, Duration::ZERO, false).await;
            tokio::time::sleep(RECOVERING).await;
        }
    }

    /// Clear the station's claim `claim`, never waiting for the marker's
    /// lock: whether it is cleared. One busy now stays, for the next round.
    async fn cleared(&self, claim: &str) -> bool {
        let (stop, claim) = (self.stop.clone(), claim.to_owned());
        super::blocking(move || stop.clear_if_within(&claim, Duration::ZERO))
            .await
            .is_ok()
    }
}

/// What the guest of `session` answers of `change`: none when it does not
/// answer whole in time.
pub(super) async fn outcome(session: &Session, change: &Token) -> Option<Value> {
    let mut last = None;
    let asked = json!({"op": "outcome", "change": change});
    read(session, LaneKind::Read, &asked, ASKING, |line| {
        last = Some(line);
        true
    })
    .await
    .ok()?;
    serde_json::from_str(&last?).ok()
}

/// Whether `answer`, what the guest says a change came to, settles it: a
/// final reply that is certain, or one the guest proves ended when it
/// declares it does.
fn settles(answer: Option<&Value>, proves_ended: bool) -> bool {
    let Some(reply) = answer.and_then(Outcome::remembered) else {
        return false;
    };
    reply["uncertain"] != true || (proves_ended && reply["ended"] == true)
}

/// Recover on the live generation, when it is `generation`, within
/// `deadline`: what it cleared, what it kept, each named with its guest as
/// who carries it out, or why nothing was recovered.
pub(super) async fn asked(keeper: &Keeper, generation: u64, deadline: Duration) -> Value {
    let session = match live_in(keeper, generation) {
        Ok(session) => session,
        Err(error) => return serde_json::to_value(&error).unwrap_or_default(),
    };
    let refused = |code, why: &str| serde_json::to_value(Error::new(code, why)).unwrap_or_default();
    match tokio::time::timeout(deadline, keeper.recovered(&session, READING, true)).await {
        Ok(Some(recovered)) => json!({
            "ok": true,
            "op": "recover",
            "cleared": recovered.cleared,
            "kept": recovered
                .kept
                .iter()
                .map(|change| json!({"change": change, "by": "guest"}))
                .collect::<Vec<_>>(),
        }),
        Ok(None) => refused(
            Code::Busy,
            "The station's pending changes could not be read; nothing was recovered.",
        ),
        Err(_) => refused(
            Code::Transport,
            "Recovery did not finish in time; what it cleared stays cleared.",
        ),
    }
}

#[cfg(test)]
#[path = "recover_tests.rs"]
mod tests;

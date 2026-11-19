//! The add's own Resume. Once its setup window takes no input again, the
//! add resumes its new VM once, under the grant that opened that window, so
//! the person allows the add once. It lifts only a Stop the guest set itself
//! before any agent was admitted (its session to this Mac lost, or its
//! restart to take Screen Recording), never the person's, only while this
//! Mac's Stop of the VM is clear, and only through a keeper and a guest that
//! keep what a Resume came to under its change. Otherwise the add is set up,
//! and the person's Resume ends it.
//!
//! Each of those holds where it is carried out, not only where it is looked
//! at: the keeper says the Resume only while this Mac's Stop is still the one
//! the add read, under the lock a Stop is noted under, and the guest lifts
//! only the very Stop of its own the add saw, refusing it, certain, once
//! another came since.
//!
//! The Resume is kept in the VM's record under a change of its own before it
//! is claimed or sent, and its guest keeps what it came to under that change
//! before it answers. Once it may have reached the guest, it is settled only
//! by the guest's word: what it came to, its work proven ended, or none in a
//! generation it was never said in, which can no longer carry it out. A
//! later asking refused, or a Stop, only ends the sending; the VM stays
//! being added, held by this add, until that word.
use super::{Adding, Granted, Means, Patience, Word, word_of};
use crate::vm::lifecycle::record::Step;
use atlas_core::{
    config::desktops::Selected,
    error::{Code, Error, Result},
    files::Token,
};
use atlas_session::control::GuestStop;
use serde_json::Value;

/// What the add says once its own Resume let agents go on.
pub(super) const ADDED: &str = "Setup is done. Agents can use the VM now.";
/// What it says as it asks it.
const RESUMING: &str = "Letting agents go on in the VM.";
/// The keeper's answer, or the guest's word on the Resume, asked again this
/// far apart, so many times, before the add stops to be asked again.
const ASKING: Patience = Patience::of(2, 30);
/// The guest and its keeper, looked at before the Resume is asked.
const LOOKING: Patience = Patience::of(2, 60);

/// What the add's own Resume came to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Came {
    /// The guest resumed it, and no Stop of this Mac's holds agents.
    Resumed,
    /// Agents still wait: it was refused, it never was carried out, or a
    /// Stop holds them.
    Held,
}

/// What the keeper's answer to one asking of the add's own Resume says.
enum Told {
    /// What the guest says it came to, for certain.
    Came(Came),
    /// Never said: no live session to say it on. It is asked again.
    Unsaid,
    /// Refused before it was said, and asking again would be too, as a Stop
    /// holds agents: this asking reached nothing, and none is asked again.
    Refused,
    /// It may have been said, and no answer came.
    Lost,
}

impl<M: Means> Adding<'_, M> {
    /// Resume the VM once, under the grant `_granted` this run took: whether
    /// agents may go on, which ends the add. One resumed or set up before is
    /// only settled.
    pub(super) fn resumed_itself(&mut self, _granted: &Granted) -> Result<bool> {
        let selected = self.selected()?;
        let kept = self
            .record
            .setup
            .as_ref()
            .and_then(|setup| setup.resume.clone());
        let ended = self.record.has(Step::Resumed);
        // Kept as what it came to already: only its claim is left.
        if ended || self.record.has(Step::Ready) {
            if let Some(change) = &kept {
                self.resume_settled(&selected, change)?;
            }
            return Ok(ended);
        }
        // Asked by an earlier run: the guest's word on it first, and one
        // that may still be carried out is never settled without it.
        let mut in_flight = false;
        if let Some(change) = &kept {
            match self.word_on(&selected, change)? {
                Word::Resumed => return self.came_to(&selected, change, Came::Resumed),
                Word::Refused => return self.came_to(&selected, change, Came::Held),
                Word::Pending => in_flight = true,
                Word::Never => {}
            }
        }
        let Some(only) = self.may_resume(&selected)? else {
            if in_flight {
                return Err(not_heard());
            }
            if let Some(change) = &kept {
                return self.came_to(&selected, change, Came::Held);
            }
            self.done(Step::Ready)?;
            return Ok(false);
        };
        let change = match kept {
            Some(change) => change,
            None => {
                let change = Token::new();
                self.setup().resume = Some(change.clone());
                self.keep()?;
                change
            }
        };
        self.means.claim(&self.vm(), &change)?;
        self.say(RESUMING);
        let came = self.asked(&selected, &change, &only)?;
        self.came_to(&selected, &change, came)
    }

    /// The guest's own Stop, as it says it now, when the VM may be resumed by
    /// the add: this Mac's Stop of it clear, its guest's own Stop one it set
    /// itself, and its keeper and its guest keeping what a Resume came to.
    fn may_resume(&self, selected: &Selected) -> Result<Option<GuestStop>> {
        let vm = self.vm();
        if self.means.stopped(&vm) {
            return Ok(None);
        }
        let (posture, keeps) =
            self.until(LOOKING, "The VM's guest is not answering yet.", || {
                let looked = self
                    .means
                    .posture(selected, &vm)
                    .and_then(|posture| Ok((posture, self.means.keeps_resumes(selected, &vm)?)));
                Ok(looked.ok())
            })?;
        Ok(posture.own_stop().filter(|_| keeps).cloned())
    }

    /// Ask the keeper the Resume `change`, for the guest's own Stop `only`,
    /// until what it came to is known: from the keeper's answer, or the
    /// guest's own word on every asking so far. One never said, or said in a
    /// generation gone since, is asked again as itself; once an asking is
    /// refused, none is, and only the guest's word settles what came before.
    fn asked(&self, selected: &Selected, change: &Token, only: &GuestStop) -> Result<Came> {
        let vm = self.vm();
        let mut sending = true;
        for at in 0..ASKING.tries {
            if at > 0 {
                self.means.wait(ASKING.every);
            }
            if sending {
                match self
                    .means
                    .resume(selected, &vm, change, only)
                    .map_or(Told::Lost, |told| told_of(&told))
                {
                    Told::Came(came) => return Ok(came),
                    Told::Refused => sending = false,
                    Told::Unsaid | Told::Lost => {}
                }
            }
            match self.word_on(selected, change) {
                Ok(Word::Resumed) => return Ok(Came::Resumed),
                Ok(Word::Refused) => return Ok(Came::Held),
                // Nothing of it can be carried out, and none is sent again.
                Ok(Word::Never) if !sending => return Ok(Came::Held),
                Ok(Word::Never | Word::Pending) | Err(_) => {}
            }
        }
        Err(not_heard())
    }

    /// What the guest says of the Resume `change`, as its keeper knows it.
    fn word_on(&self, selected: &Selected, change: &Token) -> Result<Word> {
        let unknown = |why: &str| {
            Error::new(
                Code::NotReady,
                format!(
                    "Atlas cannot tell yet whether the VM let agents go on ({why}). Run atlas \
                     desktop add vm again to go on."
                ),
            )
        };
        let vm = self.vm();
        let known = self
            .means
            .word(selected, &vm, change)
            .map_err(|error| unknown(error.error.trim_end_matches('.')))?;
        let word = word_of(&known).ok_or_else(|| {
            unknown(
                known["error"]
                    .as_str()
                    .unwrap_or("its guest did not say")
                    .trim_end_matches('.'),
            )
        })?;
        Ok(match word {
            // Resumed by the guest, but held by a Stop of this Mac's since.
            Word::Resumed if self.means.stopped(&vm) => Word::Refused,
            word => word,
        })
    }

    /// Keep what the Resume `change` came to, then settle it: the add ends
    /// once agents may go on; otherwise it is set up for the person.
    fn came_to(&mut self, selected: &Selected, change: &Token, came: Came) -> Result<bool> {
        let resumed = came == Came::Resumed;
        self.done(if resumed { Step::Resumed } else { Step::Ready })?;
        self.resume_settled(selected, change)?;
        Ok(resumed)
    }

    /// Clear the claim on the Resume `change`, tell the guest it may forget
    /// it, and forget it here.
    fn resume_settled(&mut self, selected: &Selected, change: &Token) -> Result<()> {
        self.means.settle(selected, &self.vm(), change)?;
        self.setup().resume = None;
        self.keep()
    }
}

/// Why the add stops while what its own Resume came to is not known: it is
/// kept, claimed, and the VM held, until the guest says.
fn not_heard() -> Error {
    Error::new(
        Code::NotReady,
        "Atlas has not heard yet whether the VM let agents go on. Run atlas desktop add vm again \
         to go on.",
    )
}

/// What the keeper's answer `told` says of one asking of the Resume.
fn told_of(told: &Value) -> Told {
    if told["ok"] != true {
        return Told::Refused;
    }
    if told["said"] == false {
        return Told::Unsaid;
    }
    let lifted = matches!(told["lifted"].as_str(), Some("cleared" | "already-clear"));
    match &told["outcome"] {
        outcome if outcome == "resumed" => {
            Told::Came(if lifted { Came::Resumed } else { Came::Held })
        }
        Value::Object(_) => Told::Came(Came::Held),
        _ => Told::Lost,
    }
}

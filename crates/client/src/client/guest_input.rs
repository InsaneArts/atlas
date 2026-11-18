//! An agent's input to a VM's guest, through its keeper. Each batch goes
//! under an owner of the guest's input service of its own, claimed on the
//! station before anything is taken and kept until the guest acknowledges
//! the owner let go, or recovery proves it: a command that dies meanwhile,
//! or a release refused or never said, leaves it pending, and every later
//! change waits. Input is taken, sent and let go of as three changes on the
//! guest's input lanes, all in the generation the command's proof was
//! taken in, each claimed as the guest's under a token of its own, which
//! its lane and its request name too. Nothing is sent while the guest's
//! input service is not ready for agents, and an action naming an element
//! is not built for a VM. The picture before it is taken in the generation
//! and session its proof pinned, and kept only once a proof after it shows
//! the same.
use super::{
    Client, Mutation, Verified,
    input::{native_refusal, released},
};
use atlas_control::{Pending, executors::Executor};
use atlas_core::{
    actions::Batch,
    error::{Code, Error, Holder, Result},
    files::Token,
    session::{ASKS, InputEpoch, TYPED_FIELDS},
};
use atlas_session::compatibility::INPUT_LANES;
use serde_json::{Value, json};
use std::time::Instant;

/// The longest a grant another command left on the guest is waited out:
/// the grant's own life, and a second to spare.
const GRANT_OUTLIVES: std::time::Duration =
    atlas_core::session::GRANT_TTL.saturating_add(std::time::Duration::from_secs(1));

impl Client {
    /// Refuse, in the one wording for what this build cannot do on a VM,
    /// `actions` that name an element by reference: those act on a look
    /// this build never takes on a VM. Nothing is proved or claimed first.
    ///
    /// # Errors
    ///
    /// As said; a batch that does not decode is refused where it is read.
    pub(super) fn refuse_references_on_vm(&self, actions: &Result<Batch<'_>>) -> Result<()> {
        match actions {
            Ok(batch)
                if batch
                    .actions()
                    .iter()
                    .any(|action| action.reference().is_some()) =>
            {
                Err(self.unreachable())
            }
            Ok(_) | Err(_) => Ok(()),
        }
    }

    /// Send an agent's `actions` to the VM `verified` proved, in that very
    /// proof's session: refused before anything is claimed while its guest
    /// takes no input from agents, in the guest's own words.
    ///
    /// # Errors
    ///
    /// Returns the refusal of the batch, the guest's or its input service's
    /// refusal, or the failure to deliver it; the error says when its outcome
    /// is uncertain, and the change it is about stays pending then.
    pub(super) async fn input_on_vm(
        &self,
        actions: Result<Batch<'_>>,
        verified: &Verified,
    ) -> Result<Value> {
        self.refuse_references_on_vm(&actions)?;
        self.reachable()?;
        let pinned = verified.machine().ok_or_else(|| self.unreachable())?;
        if let Some((code, held)) = &pinned.input_held {
            return Err(Error::new(*code, held.clone()));
        }
        let generation = self.generation_offering(verified, INPUT_LANES, |name| {
            format!("{name}'s Atlas is from an older build that takes no input from this Mac.")
        })?;
        let native = &verified.observation.native;
        // An input service older than this one may call a password field a
        // plain one: no text typed through it is kept.
        if !native.has_capability(TYPED_FIELDS)
            && let Some(doing) = &self.doing
        {
            doing.before().typed_into(None);
        }
        let actions = actions?;
        // An older input service would ignore the accounts and send without
        // asking: while the person lists any, it must ask.
        if !native.has_capability(ASKS) && crate::asking::policy(self).is_some() {
            return Err(Error::new(
                Code::Update,
                "The VM's input service cannot ask the person before sending: update Atlas so \
                 the VM's Atlas and this command are one build",
            ));
        }
        actions.check(&verified.session().epoch.display)?;
        self.controller.interrupt().check()?;

        let epoch = pinned.epoch.input_epoch();
        // The picture before it is asked for once its session is known,
        // beside taking the guest's input, and the input waits for it no
        // longer than the helper's.
        let picture = self.picture_before_on_vm(verified);
        let (owner, barrier) = self.taken_on_vm(&epoch, generation).await?;
        // An agent's input is judged by the person's accounts while they
        // list any.
        let (asking, permits) = crate::asking::policy_for(self, actions.actions());
        let mutation = Mutation::Input {
            actions,
            owner: owner.as_str(),
            epoch,
            picture,
            app: self.doing.is_some(),
            expect: None,
            asking,
        };
        let result = self.mutate_vm(mutation, generation, None).await;
        // A permit is spent by the request that took it, unless that request
        // is proven to have sent nothing.
        crate::asking::return_permits(self, permits, &result);
        let cleanup = self.release_on_vm(owner.as_str(), generation).await;
        if cleanup.is_ok() {
            self.let_go(&barrier, &owner).await;
        }
        released(result, cleanup).map_err(|error| self.pending_on_vm(error))
    }

    /// `outcome`, of an agent's input on this VM, with what an agent does
    /// about a failure in this VM's own words wherever the step every
    /// desktop shares would name the helper or its settings.
    pub(super) fn advised_on_vm(&self, outcome: Result<Value>) -> Result<Value> {
        self.advised_on_vm_for(outcome, "Accessibility")
    }

    /// [`Client::advised_on_vm`] of a command that needs `setting` in the
    /// guest's Privacy & Security, for a `permission` refusal; a Second
    /// Login's outcome as it is.
    pub(crate) fn advised_on_vm_for(&self, outcome: Result<Value>, setting: &str) -> Result<Value> {
        if !matches!(self.desktop().kind(), atlas_core::desktop::Kind::Vm(_)) {
            return outcome;
        }
        match outcome {
            Err(error) if error.details.contains_key("next") => Err(error),
            Err(error) => match self.next_on_vm(error.code, error.holder, error.uncertain, setting)
            {
                Some(next) => Err(error.with("next", json!(next))),
                None => Err(error),
            },
            Ok(mut reply) => {
                if reply["ok"] == false
                    && reply.get("next").is_none()
                    && let Some(code) = reply["code"].as_str().and_then(Code::parse)
                {
                    let holder = reply["holder"].as_str().and_then(Holder::parse);
                    if let Some(next) =
                        self.next_on_vm(code, holder, reply["uncertain"] == true, setting)
                    {
                        reply["next"] = json!(next);
                    }
                }
                Ok(reply)
            }
        }
    }

    /// What an agent does about a failure with `code` on this VM, when the
    /// step every desktop shares would name the helper: a permission is
    /// the guest's `setting`, allowed in the VM's own window.
    fn next_on_vm(
        &self,
        code: Code,
        holder: Option<Holder>,
        uncertain: bool,
        setting: &str,
    ) -> Option<String> {
        let name = &self.selected.name;
        if code == Code::Permission && !uncertain {
            return Some(format!(
                "Ask the person to allow Atlas in Privacy & Security, {setting}, in {name}'s \
                 window."
            ));
        }
        let step = atlas_core::error::advice::step(Some(code), holder, uncertain);
        step.contains("helper").then(|| {
            let owned = format!("{name}'s");
            [
                ("the helper account", name.as_str()),
                ("the helper's", owned.as_str()),
                ("the helper", name.as_str()),
                ("The helper's", owned.as_str()),
                ("The helper", name.as_str()),
            ]
            .into_iter()
            .fold(step.to_owned(), |step, (helper, vm)| {
                step.replace(helper, vm)
            })
        })
    }

    /// The owner `owner`, whose claim is `barrier`, let go of as its guest
    /// acknowledged: its claim cleared, then the guest told it may forget
    /// it. A claim that cannot be cleared stays for recovery.
    async fn let_go(&self, barrier: &Pending, owner: &Token) {
        if self.controller.clear_if(barrier).is_ok() {
            self.settle_guest(std::slice::from_ref(owner)).await;
        }
    }

    /// Take the guest's input in the session `epoch` names and the
    /// generation `generation`, under an owner claimed on the station before
    /// it is asked for: that owner and its claim, which goes only once the
    /// guest acknowledges the owner let go. A definite busy from another
    /// agent is tried again, about ten times a second for what is left of
    /// the controller's wait and no longer than a grant can outlive its
    /// holder, each try under an owner and a change of its own: an owner the
    /// guest refused for certain took nothing, and is let go of at once. A
    /// busy from the person, and every other answer, ends it, and a saved
    /// Stop or a signal's stop ends the retries.
    ///
    /// # Errors
    ///
    /// The refusal of the last try. One whose outcome is unknown is followed
    /// by its owner's release, and its claim stays pending unless the guest
    /// acknowledges that.
    async fn taken_on_vm(&self, epoch: &InputEpoch, generation: u64) -> Result<(Token, Pending)> {
        let mut until = None;
        loop {
            let owner = Token::new();
            let barrier = self
                .controller
                .quarantine_by(owner.as_str(), Executor::Guest)?;
            let acquire = Mutation::Acquire {
                owner: owner.as_str(),
                epoch: epoch.clone(),
            };
            let refused = match self.mutate_vm(acquire, generation, None).await {
                Ok(reply) if reply["ok"] == true => return Ok((owner, barrier)),
                Ok(reply) => self.pending_on_vm(native_refusal(&reply)),
                Err(error) => error,
            };
            if refused.uncertain {
                // The guest may now hold input for `owner`: let it go, or
                // leave what that came to pending as well.
                if self.release_on_vm(owner.as_str(), generation).await.is_ok() {
                    self.let_go(&barrier, &owner).await;
                }
                return Err(refused);
            }
            self.let_go(&barrier, &owner).await;
            if refused.code != Code::Busy || refused.holder != Some(Holder::Agent) {
                return Err(refused);
            }
            let until = *until.get_or_insert_with(|| {
                Instant::now() + self.controller.remaining().min(GRANT_OUTLIVES)
            });
            let left = until.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Err(refused);
            }
            tokio::time::sleep(atlas_control::poll(left)).await;
            if self.stop_marked() {
                return Err(self.stopped());
            }
            self.controller.interrupt().check()?;
        }
    }

    /// Let go of the guest's input `owner` took, in the generation
    /// `generation`, and release whatever it left held: acknowledged only
    /// when the guest says it released it.
    ///
    /// # Errors
    ///
    /// Returns the guest's or the keeper's refusal, or the failure to hear
    /// its answer: the owner is not known to be let go of then.
    async fn release_on_vm(&self, owner: &str, generation: u64) -> Result<Value> {
        let reply = self
            .mutate_vm(Mutation::Release { owner }, generation, None)
            .await?;
        if reply["ok"] != true {
            return Err(native_refusal(&reply));
        }
        Ok(reply)
    }
}

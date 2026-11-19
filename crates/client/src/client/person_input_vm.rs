//! The person's own control of a VM's guest input, for the viewer service
//! alone: their control taken under an owner of the guest's input service,
//! their input sent under it, kept a while longer, and let go of. Each is a
//! change the VM's keeper carries only with its capability for the person,
//! which no agent's command has, and each goes in the generation the
//! person's proof was taken in: a new one ends their control. The guest
//! admits them under either Stop, and no Stop ends them, as the person may
//! act on a stopped VM. The owner is claimed here before anything is taken,
//! and its claim goes only once the guest acknowledges it let go: a release
//! refused or unanswered leaves it pending, and agents wait until the
//! keeper's recovery proves the guest let it go. A change whose answer is
//! lost stays pending in the same way. A page the person signs in on opens
//! the same way, under the owner their control holds.
use super::{Actor, Client, Mutation, input::native_refusal, mutate::ForPerson};
use atlas_control::{Pending, SharedLease, executors::Executor};
use atlas_core::{
    actions::Batch,
    desktop::Kind,
    error::{Code, Error, Result},
    files::Token,
    session::Epoch,
};
use atlas_session::compatibility::PERSON_INPUT;
use serde_json::Value;
use std::time::Instant;

/// A VM proved for the person's control of it: the session they see, and
/// the generation every change of their control goes in.
#[derive(Debug)]
pub struct PersonProof {
    epoch: Epoch,
    generation: u64,
}

impl PersonProof {
    /// The session the proof showed.
    #[must_use]
    pub fn epoch(&self) -> &Epoch {
        &self.epoch
    }
}

/// The person's owner of a VM's guest input, taken in one generation, and
/// its claim here, kept until the guest acknowledges it let go.
#[derive(Debug)]
pub struct GuestOwner {
    owner: Token,
    generation: u64,
    claim: Pending,
}

impl GuestOwner {
    /// The generation it was taken in: every change under it goes there.
    #[must_use]
    pub fn generation(&self) -> u64 {
        self.generation
    }
}

impl Client {
    /// Refuse anyone but the person's own surface, and any desktop but a VM.
    fn persons_alone(&self) -> Result<()> {
        if !matches!(self.desktop().kind(), Kind::Vm(_)) {
            return Err(self.unreachable());
        }
        if self.actor != Actor::Viewer {
            return Err(Error::new(
                Code::Denied,
                format!(
                    "Only the person takes control of {}, from Atlas's own window.",
                    self.selected.name
                ),
            ));
        }
        Ok(())
    }

    /// The VM proved now for the person's control: proved afresh in one
    /// generation of its keeper's session, open to no one but Atlas, with
    /// no update handing it over and no maintenance holding it, its guest's
    /// input service taking input and serving the person's. Neither the
    /// person's Stop nor the guest's holds it, and it never starts the VM.
    ///
    /// # Errors
    ///
    /// Refuses anyone but the viewer and any desktop but a VM; refuses a VM
    /// that does not prove, is held as above, or whose input service takes
    /// no input yet, in the guest's own words; and one whose guest serves no
    /// person's control, with what to do next. Nothing is sent.
    pub async fn person_proof_on_vm(&self) -> Result<PersonProof> {
        self.persons_alone()?;
        let verified = self.verify_vm_reading().await?;
        let pinned = verified.machine().ok_or_else(|| self.unreachable())?;
        if let Some((code, held)) = &pinned.input_held {
            return Err(Error::new(*code, held.clone()));
        }
        let generation = self.generation_offering(&verified, PERSON_INPUT, |name| {
            format!("{name}'s Atlas is from an older build that the person cannot control.")
        })?;
        Ok(PersonProof {
            epoch: verified.session().epoch.clone(),
            generation,
        })
    }

    /// Take the VM's guest input for the person, under `owner`, their
    /// lease's token, in the session and generation `proof` showed, the
    /// keeper's `capability` for the person carrying it: the owner, kept as
    /// their lease's. Any holder is refused at once, never waited out.
    ///
    /// # Errors
    ///
    /// The guest's refusal, or the failure to hear it. One whose outcome is
    /// unknown is followed by the owner's release, and its claim stays
    /// pending unless the guest acknowledges that.
    pub async fn person_acquire_on_vm(
        &self,
        owner: &str,
        proof: &PersonProof,
        capability: &str,
    ) -> Result<GuestOwner> {
        self.persons_alone()?;
        let token = Token::parse(owner)
            .ok_or_else(|| Error::new(Code::Input, "Control is taken under a lease's token"))?;
        let claim = self.controller.quarantine_by(owner, Executor::Guest)?;
        let acquire = Mutation::Acquire {
            owner,
            epoch: proof.epoch.input_epoch(),
            person: true,
        };
        let refused = match self
            .mutate_vm_as(acquire, proof.generation, None, Some(carried(capability)))
            .await
        {
            Ok(reply) if reply["ok"] == true => {
                return Ok(GuestOwner {
                    owner: token,
                    generation: proof.generation,
                    claim,
                });
            }
            Ok(reply) => self.pending_on_vm(native_refusal(&reply)),
            Err(error) => error,
        };
        // The guest may now hold input for the owner: let it go, or leave
        // what that came to pending as well.
        if !refused.uncertain
            || self
                .release_for_person(owner, proof.generation, capability)
                .await
                .is_ok()
        {
            self.let_go(&claim, &token).await;
        }
        Err(refused)
    }

    /// Send the person's `actions`, as they saw the frame of `epoch` at
    /// `observed_at`, under `owner`, their lease's token, in `generation`:
    /// authorized by `lease` still naming `owner` and that frame still
    /// fresh, right before it goes, and never checked against an app.
    ///
    /// # Errors
    ///
    /// The refusal of the batch, the lease's or the frame's, or the failure
    /// to deliver it; the error says when its outcome is uncertain, and it
    /// stays pending then.
    pub async fn person_input_on_vm(
        &self,
        (owner, generation): (&str, u64),
        (actions, epoch, observed_at): (&Value, &Epoch, Instant),
        lease: &SharedLease,
        capability: &str,
    ) -> Result<Value> {
        self.persons_alone()?;
        let actions = Batch::decode(actions)?;
        actions.check(&epoch.display)?;
        let input = Mutation::Input {
            actions,
            owner,
            epoch: epoch.input_epoch(),
            picture: None,
            app: false,
            expect: None,
            asking: None,
            person: true,
        };
        let person = ForPerson {
            lease: Some((lease, owner)),
            observed_at: Some(observed_at),
            ..carried(capability)
        };
        self.mutate_vm_as(input, generation, None, Some(person))
            .await
    }

    /// Keep the person's control under `owner` a while longer on the
    /// guest, in `generation`: a change, settled only once the guest
    /// answers it.
    ///
    /// # Errors
    ///
    /// The guest's refusal, as when its grant ran out or the generation
    /// ended, or the failure to hear it; one whose outcome is unknown stays
    /// pending.
    pub async fn person_renew_on_vm(
        &self,
        owner: &str,
        generation: u64,
        capability: &str,
    ) -> Result<Value> {
        self.persons_alone()?;
        let reply = self
            .mutate_vm_as(
                Mutation::Renew { owner },
                generation,
                None,
                Some(carried(capability)),
            )
            .await?;
        if reply["ok"] != true {
            return Err(native_refusal(&reply));
        }
        Ok(reply)
    }

    /// Open `url` in the browser named `app` on the guest for the person,
    /// under `owner`, the lease's token their control holds there, in
    /// `generation`: authorized by `lease` still naming `owner` right before
    /// it goes, and cancelled unsent once it does not. Whether the page
    /// opened.
    ///
    /// # Errors
    ///
    /// `lease-lost` once the lease ended, unsent; the guest's refusal, as
    /// when the person's control there ended; `spawn-failed` when the guest
    /// could not open it; the failure to hear it, its outcome then
    /// uncertain and kept pending until the guest answers.
    pub async fn person_open_on_vm(
        &self,
        (lease, owner): (&SharedLease, &str),
        generation: u64,
        (app, url): (&str, &str),
        capability: &str,
    ) -> Result<Value> {
        self.persons_alone()?;
        let open = Mutation::PersonOpen {
            app,
            url,
            lease: owner,
        };
        let person = ForPerson {
            lease: Some((lease, owner)),
            ..carried(capability)
        };
        let reply = self
            .mutate_vm_as(open, generation, None, Some(person))
            .await?;
        if reply["ok"] != true {
            return Err(native_refusal(&reply));
        }
        if reply["exitCode"] != 0 {
            return Err(Error::new(
                Code::SpawnFailed,
                format!("{} could not open {app}.", self.selected.name),
            ));
        }
        Ok(serde_json::json!({"ok": true, "opened": true}))
    }

    /// Let go of the person's `owner` on the guest, and whatever it left
    /// held: its claim cleared once the guest acknowledges it. Cleanup, so
    /// it goes whatever stop came.
    ///
    /// # Errors
    ///
    /// The guest's refusal, or the failure to hear it: the owner is not
    /// known to be let go of then, and its claim stays pending until the
    /// keeper's recovery proves it.
    pub async fn person_release_on_vm(&self, owner: GuestOwner, capability: &str) -> Result<Value> {
        let released = self
            .release_for_person(owner.owner.as_str(), owner.generation, capability)
            .await?;
        self.let_go(&owner.claim, &owner.owner).await;
        Ok(released)
    }

    /// Let go of the person's `owner` in `generation`: acknowledged only
    /// when the guest says it released it.
    async fn release_for_person(
        &self,
        owner: &str,
        generation: u64,
        capability: &str,
    ) -> Result<Value> {
        let release = Mutation::Release {
            owner,
            person: true,
        };
        let reply = self
            .mutate_vm_as(release, generation, None, Some(carried(capability)))
            .await?;
        if reply["ok"] != true {
            return Err(native_refusal(&reply));
        }
        Ok(reply)
    }
}

/// A change of the person's carried with the keeper's `capability`, its
/// lease checked by its caller.
fn carried(capability: &str) -> ForPerson<'_> {
    ForPerson {
        capability,
        lease: None,
        observed_at: None,
    }
}

#[cfg(test)]
#[path = "person_input_vm_tests.rs"]
mod tests;

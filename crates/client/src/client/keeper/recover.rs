//! The person's Recover of a VM desktop, which `atlas restart` on a VM is
//! too. It holds the desktop's exclusion from its first look to its reply.
//! Every change Atlas VM carries out there is settled from what Atlas VM
//! answers alone, whether the VM runs or not. Every change its guest carries
//! out is settled from what the guest says it came to, once the VM is
//! proved; a VM that does not prove keeps them until it runs, and is never
//! started. Nothing is asked again, and every change left pending is said
//! with who carries it out, as is whatever kept either part from finishing.
use super::PENDING_READ;
use crate::client::Client;
use atlas_control::{Ownership, executors::Executor};
use atlas_core::{
    config::desktops::Admission,
    error::{Code, Error, Result},
};
use serde_json::{Value, json};
use std::time::Duration;

/// How long the person's recovery of a VM waits for its keeper, and the
/// moment of it left for the keeper's own answer.
const RECOVERY: Duration = Duration::from_secs(60);
const RECOVERY_MARGIN: Duration = Duration::from_secs(2);

/// Who carries out a change, as a recovery names it: the desktop itself
/// for its own services.
fn by(executor: Option<Executor>) -> &'static str {
    match executor {
        Some(Executor::Guest) => "guest",
        Some(Executor::Vm) => "vm",
        Some(Executor::Station) => "station",
        Some(Executor::Daemon | Executor::Input) | None => "desktop",
    }
}

impl Client {
    /// Recover a VM, as the person's Recover does, under its exclusion: the
    /// changes Atlas VM answered cleared, then, the VM proved afresh, those
    /// whose command ended without its guest's answer asked of the guest
    /// through its keeper in the generation the proof was taken in. Only a
    /// certain answer, or one the guest proves ended, clears a guest's
    /// claim. Every other change stays pending, and is said as kept with who
    /// carries it out. A Stop does not hold it: it changes nothing on the
    /// guest.
    ///
    /// # Errors
    ///
    /// Refuses a VM still being added, under the person's maintenance, or
    /// handed over by an update, and one whose exclusion another command
    /// holds or a command still runs on.
    pub(in crate::client) async fn recover_vm(&self) -> Result<Value> {
        if self.admission() != Admission::Open {
            return Err(self.selected.unreachable());
        }
        self.recoverable()?;
        let _exclusion = self.recovery_exclusion()?;
        // Looked at again under it: a maintenance or a handover begun while
        // it was waited for is seen now, and none begins until it goes.
        self.recoverable()?;
        // Every claim it cleared is said; what it could not settle stays
        // pending, said kept, and why it stopped short is said too.
        let atlas_vm = crate::vm::start::settled_from_outcomes(self).await;
        let guest = self.guest_recovered().await;
        let controller = self.controller.clone();
        let kept = tokio::task::spawn_blocking(move || controller.pending_by(PENDING_READ))
            .await
            .map_or(json!("unknown"), |pending| {
                pending.map_or(json!("unknown"), |pending| {
                    pending
                        .into_iter()
                        .map(|(change, executor)| json!({"change": change, "by": by(executor)}))
                        .collect()
                })
            });
        let mut cleared: Vec<Value> = atlas_vm
            .cleared
            .iter()
            .map(|change| json!({"change": change, "by": "vm"}))
            .collect();
        if let Ok(changes) = &guest {
            cleared.extend(
                changes
                    .iter()
                    .map(|change| json!({"change": change, "by": "guest"})),
            );
        }
        let guest = match guest {
            Ok(_) => json!({"recovered": true}),
            Err(why) => json!({"recovered": false, "why": why}),
        };
        let mut recovered = json!({
            "ok": true,
            "op": "recover",
            "desktop": {"kind": "vm", "name": self.selected.name},
            "cleared": cleared,
            "kept": kept,
            "guest": guest,
        });
        if let Some(failed) = atlas_vm.failed {
            recovered["station"] = json!({"why": format!(
                "Not every change Atlas VM answered on {} was settled: {}",
                self.selected.name, failed.error
            )});
        }
        Ok(recovered)
    }

    /// Refuse a recovery of this VM while the person's maintenance of it is
    /// kept, or an update still hands its session over.
    fn recoverable(&self) -> Result<()> {
        if let Some(held) = self.maintenance_held() {
            return Err(held);
        }
        if let Some(why) = self.handover_unfinished() {
            return Err(Error::new(Code::Busy, why));
        }
        Ok(())
    }

    /// The VM's exclusion for its recovery, refused while another command
    /// holds it past this command's wait or a command an agent started still
    /// runs there, in the VM's words.
    fn recovery_exclusion(&self) -> Result<Ownership> {
        self.controller.acquire(true).map_err(|mut refused| {
            if refused
                .details
                .get("running")
                .is_some_and(|running| running == "command")
            {
                let name = &self.selected.name;
                refused.error = format!("A command an agent started is still running on {name}.");
                refused = refused.with(
                    "next",
                    json!(format!(
                        "Wait for the command running on {name} to finish, then recover."
                    )),
                );
            }
            refused
        })
    }

    /// The changes this VM's guest carries out that its keeper cleared from
    /// what the guest answered, once the VM is proved; else why none were
    /// asked after. Nothing starts the VM.
    async fn guest_recovered(&self) -> std::result::Result<Vec<Value>, String> {
        let mut keeper = match self.keeper().await {
            Ok(keeper) => keeper,
            Err(error) => return Err(self.unproved(error).await),
        };
        let proved = match self.verify_machine(&keeper).await {
            Ok(proved) => proved,
            Err(error) => return Err(self.unproved(error).await),
        };
        if let Err(error) = proved.session {
            return Err(self.unproved(error).await);
        }
        keeper.timeout = RECOVERY;
        let asked = json!({"op": "recover", "generation": proved.observed.generation,
            "deadline_ms": (RECOVERY - RECOVERY_MARGIN).as_millis()});
        let answer = keeper.one(&asked).await.map_err(|error| error.error)?;
        if answer["ok"] != true {
            return Err(atlas_core::transport::refusal(&answer).error);
        }
        Ok(answer["cleared"].as_array().cloned().unwrap_or_default())
    }

    /// Why this VM's guest was asked nothing, `error` being why it did not
    /// prove: that it is not running, when Atlas VM confirms it stopped.
    async fn unproved(&self, error: Error) -> String {
        if crate::vm::start::stopped_now(self).await.is_some() {
            let name = &self.selected.name;
            return format!("{name} is not running, so its guest's changes wait until it runs.");
        }
        error.error
    }
}

#[cfg(test)]
#[path = "recover_tests.rs"]
mod tests;

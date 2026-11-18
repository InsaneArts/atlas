//! What a client asks of a VM desktop through its session keeper: its status
//! with a fresh proof of it, and whether it is ready for agents; the proof a
//! command acts in, and its guest's own Stop; the guest's word on work it
//! started that may still run; the person's Stop and Resume; and the
//! recovery of changes whose command ended without an answer. Everything
//! else stays refused in the one wording for what this build cannot do on a
//! VM.
use super::{Actor, Client, Pinned, Verified};
use crate::{
    link::{Keeper, Link},
    vm::{maintenance::Kept, proof::Observed},
};
use atlas_core::{
    config::desktops::{Admission, Chosen},
    error::{Code, Error, Result},
    files::Token,
    session::{MachineSession, Observation, Session},
};
use atlas_session::compatibility::{Declared, MACHINE, PROTOCOL, Usable, judge};
use serde_json::{Value, json};

/// How long a status waits for the station's pending changes to be read.
const PENDING_READ: std::time::Duration = std::time::Duration::from_millis(500);

/// How long the person's recovery of a VM waits for its keeper, and the
/// moment of it left for the keeper's own answer.
const RECOVERY: std::time::Duration = std::time::Duration::from_secs(60);
const RECOVERY_MARGIN: std::time::Duration = std::time::Duration::from_secs(2);

/// How long a status waits for the guest's word on its leftover work.
const LEFTOVERS_READ: std::time::Duration = std::time::Duration::from_secs(2);

/// Whether work a VM's guest started may still run there, as the guest
/// itself says: what the station asks before it takes the desktop.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Leftovers {
    Running,
    None,
    /// Not known: held as running is.
    Unknown,
}

impl Leftovers {
    /// Whether work may still run: unless the guest said none does.
    #[must_use]
    pub fn may_run(self) -> bool {
        self != Self::None
    }
}

/// A VM's proof as one command took it: what its keeper observed, and the
/// session that proves or why it does not.
struct Proved {
    observed: Observed,
    session: Result<MachineSession>,
}

/// `error`, a VM's status that failed, with the handover an update is still
/// making of it, `unfinished`, when one is, and the person's maintenance of
/// it as its record says it, `maintenance`, when one is kept or cannot be
/// read: its code and words kept.
fn handing_over(error: Error, unfinished: Option<String>, maintenance: &Value) -> Error {
    let mut station = serde_json::Map::new();
    if let Some(why) = unfinished {
        station.insert("handover".to_owned(), json!(why));
    }
    if maintenance["state"] != "none" {
        station.insert("maintenance".to_owned(), maintenance.clone());
    }
    if station.is_empty() {
        return error;
    }
    error.with("station", Value::Object(station))
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

    /// What this VM's lifecycle record says of the person's maintenance of
    /// it: none for any other desktop.
    pub(crate) fn maintenance(&self) -> Kept {
        let Chosen::Vm(config) = &self.selected.desktop else {
            return Kept::None;
        };
        Kept::read(self.lifecycle.as_ref(), &config.id).0
    }

    /// Why the person's maintenance of this VM holds agents now; none
    /// while none is kept, and for any other desktop.
    pub(crate) fn maintenance_held(&self) -> Option<Error> {
        self.maintenance().held(&self.selected.name)
    }

    /// A VM proved now, in one generation of its session: a challenge of
    /// this command's own, what `keeper` observed for it, and the VM's
    /// pairing read afresh. Nothing of it is kept.
    ///
    /// # Errors
    ///
    /// Fails when the keeper gives no observation of one generation that
    /// answers this command's challenge; a refused proof is [`Proved`]'s.
    async fn verify_machine(&self, keeper: &Keeper) -> Result<Proved> {
        let challenge = atlas_core::files::token();
        let asked = json!({"op": "observe", "challenge": challenge,
            "deadline_ms": keeper.timeout.as_millis()});
        let answer = keeper.one(&asked).await?;
        if answer["ok"] != true {
            return Err(observe_refused(&answer, &self.selected.name));
        }
        let observed = Observed::decode(&answer, &challenge)?;
        let reading = keeper.clone();
        let paired = tokio::task::spawn_blocking(move || reading.paired())
            .await
            .unwrap_or_else(|_| Err(Error::new(Code::State, "The pairing could not be read.")));
        let held = self.maintenance_held();
        let session = paired.and_then(|paired| observed.prove(&self.desktop(), &paired, held));
        Ok(Proved { observed, session })
    }

    /// Whether the VM's keeper holds a session to its guest now and the
    /// guest proves itself in it, its Stop aside: what a command waits for
    /// once the VM it needs is started.
    pub(crate) async fn proven_now(&self) -> bool {
        let Ok(keeper) = self.keeper().await else {
            return false;
        };
        match keeper.one(&json!({"op": "hello"})).await {
            Ok(hello) if hello["live"] == true => {}
            Ok(_) | Err(_) => return false,
        }
        self.verify_machine(&keeper)
            .await
            .is_ok_and(|proved| proved.session.is_ok())
    }

    /// A VM verified now for one of the commands this build runs on it (an
    /// agent's `run`, a job's start or stop): proved afresh in one
    /// generation of its keeper's session, its guest's own Stop clear, its
    /// agents not stopped, open to no one but Atlas, and no update handing
    /// its session over. The command keeps the proof's generation for every
    /// change it makes.
    ///
    /// # Errors
    ///
    /// Refuses a VM that does not prove, or proves but is held as above:
    /// stopped while its guest, or the update handing it over, holds
    /// agents.
    pub(crate) async fn verify_vm(&self) -> Result<Verified> {
        if let Some(why) = self.handover_unfinished() {
            return Err(Error::new(Code::Stopped, why));
        }
        if let Some(held) = self.maintenance_held() {
            return Err(held);
        }
        let keeper = self.keeper().await?;
        let proved = self.verify_machine(&keeper).await?;
        let machine = proved.session?;
        let status = &proved.observed.status;
        if let Some(held) = guest_held(&self.selected.name, status) {
            return Err(held);
        }
        admitted(status)?;
        Ok(verified_vm(machine, &proved.observed))
    }

    /// A VM verified now for a look at its screen: as [`Client::verify_vm_reading`],
    /// and its guest allowed to record its screen, else refused in its own
    /// words.
    ///
    /// # Errors
    ///
    /// As [`Client::verify_vm_reading`], and `permission` while the guest may
    /// not record.
    pub(crate) async fn verify_vm_looking(&self) -> Result<Verified> {
        self.vm_looking(true).await
    }

    /// A VM verified now for a look that changes nothing, a file's read
    /// among them: proved afresh in one generation of its keeper's session,
    /// open to no one but Atlas, and no update handing its session over.
    /// Neither the person's Stop nor the guest's holds it, and it never
    /// starts the VM.
    ///
    /// # Errors
    ///
    /// Refuses a VM that does not prove, is open to more than Atlas, or is
    /// being handed over by an update.
    pub(crate) async fn verify_vm_reading(&self) -> Result<Verified> {
        self.vm_looking(false).await
    }

    /// [`Client::verify_vm_reading`], and the guest allowed to record its
    /// screen when `capture`.
    async fn vm_looking(&self, capture: bool) -> Result<Verified> {
        if let Some(why) = self.handover_unfinished() {
            return Err(Error::new(Code::Busy, why));
        }
        if let Some(held) = self.maintenance_held() {
            return Err(held);
        }
        let keeper = self.keeper().await?;
        let proved = self.verify_machine(&keeper).await?;
        let machine = proved.session?;
        let status = &proved.observed.status;
        admitted(status)?;
        if capture && !machine.capture {
            return Err(crate::capability::vm_capture_held(status));
        }
        Ok(verified_vm(machine, &proved.observed))
    }

    /// Prove the VM again once a picture was taken on the proof `before`:
    /// refused unless this proof shows the same epoch, the same generation
    /// and Atlas VM's same run and picture fence, its window closed again,
    /// so no opening of the window came between the two.
    ///
    /// # Errors
    ///
    /// The proof's own refusal; `session-changed` when anything named above
    /// differs, or Atlas VM keeps no fence.
    pub(crate) async fn reproved(&self, before: &Pinned) -> Result<()> {
        let keeper = self.keeper().await?;
        let proved = self.verify_machine(&keeper).await?;
        let machine = proved.session?;
        let fence = proved.observed.runner.fence();
        let same = machine.epoch == before.epoch
            && proved.observed.generation == before.generation
            && fence.is_some()
            && fence == before.fence;
        if !same {
            return Err(self.vm_changed());
        }
        Ok(())
    }

    /// Refuse a look at this VM, proved as `pinned`, while Atlas VM keeps
    /// no picture fence: no proof could then tell whether its window was
    /// opened while it was looked at.
    ///
    /// # Errors
    ///
    /// `update`, in the VM's words, when Atlas VM keeps none.
    pub(crate) fn fenced(&self, pinned: &Pinned) -> Result<()> {
        if pinned.fence.is_some() {
            return Ok(());
        }
        Err(Error::new(
            Code::Update,
            format!(
                "Atlas VM, which runs {}, is from an older build that cannot say whether its \
                 window was opened, so agents look at nothing there.",
                self.selected.name
            ),
        )
        .with(
            "next",
            json!("Update Atlas so Atlas VM and this command are one build."),
        ))
    }

    /// The refusal of a picture of this VM whose two proofs differ.
    pub(super) fn vm_changed(&self) -> Error {
        Error::new(
            Code::SessionChanged,
            format!(
                "{} changed, or its window was opened, since Atlas checked it; look again",
                self.selected.name
            ),
        )
    }

    /// [`Client::verify_vm`], refusing a guest whose daemon does not offer
    /// `capability` in `older`'s words, given the VM's name: nothing is sent
    /// to it.
    pub(crate) async fn verify_vm_offering(
        &self,
        capability: &str,
        older: impl FnOnce(&str) -> String,
    ) -> Result<Verified> {
        let verified = self.verify_vm().await?;
        self.generation_offering(&verified, capability, older)?;
        Ok(verified)
    }

    /// The generation `verified`, a VM's proof, was taken in, which every
    /// change a command makes on that proof carries; refused when the guest
    /// it proved does not offer `capability`, in `older`'s words, given the
    /// VM's name, or when it proved no VM.
    ///
    /// # Errors
    ///
    /// Refuses as said; nothing is sent.
    pub(crate) fn generation_offering(
        &self,
        verified: &Verified,
        capability: &str,
        older: impl FnOnce(&str) -> String,
    ) -> Result<u64> {
        let generation = verified
            .machine
            .as_ref()
            .map(|machine| machine.generation)
            .ok_or_else(|| self.unreachable())?;
        if !verified.observation.has_capability(capability) {
            return Err(Error::new(Code::Update, older(&self.selected.name)).with(
                "next",
                json!("Update Atlas so the VM's Atlas and this command are one build."),
            ));
        }
        Ok(generation)
    }

    /// Whether work the VM's guest started may still run there, as the
    /// guest says when asked through its keeper. Second Login keeps its own
    /// ledger on this Mac (the controller's run groups and the helper's
    /// jobs), so it is never asked this.
    ///
    /// # Errors
    ///
    /// Refuses Second Login and a VM still being added in the one wording;
    /// fails when the keeper cannot be reached or does not carry the read.
    /// A guest that answers anything but `running` or `none` is unknown.
    pub async fn leftovers(&self) -> Result<Leftovers> {
        if !matches!(self.desktop().kind(), atlas_core::desktop::Kind::Vm(_))
            || self.admission() != Admission::Open
        {
            return Err(self.unreachable());
        }
        let mut keeper = self.keeper().await?;
        keeper.timeout = keeper.timeout.min(LEFTOVERS_READ);
        let read = json!({"op": "read", "request": {"op": "leftovers"},
            "deadline_ms": keeper.timeout.as_millis()});
        let answer = keeper.one(&read).await?;
        if answer["ok"] != true {
            return Err(atlas_core::transport::refusal(&answer));
        }
        Ok(match answer["leftovers"].as_str() {
            Some("running") => Leftovers::Running,
            Some("none") => Leftovers::None,
            _ => Leftovers::Unknown,
        })
    }

    /// Why the VM's guest holds agents now, as its own hello read through
    /// the keeper says; none when its Stop is clear and its agents go on.
    /// A hello that cannot be read holds them as a Stop not known.
    pub(super) async fn guest_stopped(&self) -> Option<Error> {
        let hello = match self.link.call(&json!({"op": "hello"})).await {
            Ok(hello) if hello["ok"] == true => hello,
            Ok(_) | Err(_) => json!({"stop": {"state": "unknown"}}),
        };
        guest_held(&self.selected.name, &hello)
    }

    /// The refusal of an agent's change while the person's Stop holds:
    /// named for the VM this client acts on, the helper's own otherwise.
    pub(crate) fn stopped(&self) -> Error {
        if !matches!(self.desktop().kind(), atlas_core::desktop::Kind::Vm(_)) {
            return Error::stopped();
        }
        Error::new(
            Code::Stopped,
            format!(
                "The person stopped agents on {}. Ask them before doing anything more there.",
                self.selected.name
            ),
        )
    }

    /// Tell the VM's guest it may forget what `changes` came to, the
    /// station's claims on them cleared: best effort, as its keeper settles
    /// them by itself once it sees them unclaimed.
    pub(super) async fn settle_guest(&self, changes: &[Token]) {
        if let Link::Keeper(keeper) = &self.link {
            let _ = keeper
                .one(&json!({"op": "settle", "changes": changes}))
                .await;
        }
    }

    /// Recover a VM, as the person's Recover does: the VM proved afresh,
    /// then every change its guest carries out whose command ended without
    /// its answer asked of the guest through its keeper, in the generation
    /// the proof was taken in. Only a certain answer, or one the guest
    /// proves ended, clears a claim; every other change stays pending, and
    /// is said as kept. A Stop does not hold it: it changes nothing on the
    /// guest, and a claim Atlas VM or the station carries out is never
    /// looked at.
    ///
    /// # Errors
    ///
    /// Refuses a VM still being added, or handed over by an update; fails
    /// when it does not prove, or its keeper cannot read the station's
    /// pending changes or refuses.
    pub(super) async fn recover_vm(&self) -> Result<Value> {
        if self.admission() != Admission::Open {
            return Err(self.selected.unreachable());
        }
        if let Some(held) = self.maintenance_held() {
            return Err(held);
        }
        if let Some(why) = self.handover_unfinished() {
            return Err(Error::new(Code::Busy, why));
        }
        let mut keeper = self.keeper().await?;
        let proved = self.verify_machine(&keeper).await?;
        proved.session?;
        keeper.timeout = RECOVERY;
        let asked = json!({"op": "recover", "generation": proved.observed.generation,
            "deadline_ms": (RECOVERY - RECOVERY_MARGIN).as_millis()});
        let answer = keeper.one(&asked).await?;
        if answer["ok"] != true {
            return Err(atlas_core::transport::refusal(&answer));
        }
        let controller = self.controller.clone();
        let kept = tokio::task::spawn_blocking(move || controller.pending_within(PENDING_READ))
            .await
            .map_or(json!("unknown"), |pending| {
                pending.map_or(json!("unknown"), |pending| json!(pending))
            });
        Ok(json!({
            "ok": true,
            "op": "recover",
            "desktop": {"kind": "vm", "name": self.selected.name},
            "cleared": answer["cleared"],
            "kept": kept,
        }))
    }

    /// A VM's status: its keeper's hello, its guest's own and a fresh proof
    /// of it, read through the keeper, and whether it is ready for agents,
    /// as [`readiness`] judges it from all of these, or why not.
    pub(super) async fn vm_status(&self) -> Result<Value> {
        // The person's maintenance, read from the station's own record
        // whether or not its keeper answers, and an update still handing it
        // over, are said on every path, the keeper's own failure kept.
        let maintenance = self.maintenance();
        let said = maintenance.said(&self.selected.name);
        let handing = |error| handing_over(error, self.handover_unfinished(), &said);
        // A VM still being added answers nothing but the person's Stop.
        if self.admission() != Admission::Open {
            return Err(handing(self.selected.unreachable()));
        }
        let keeper = self.keeper().await.map_err(handing)?;
        let hello = keeper.one(&json!({"op": "hello"})).await.map_err(handing)?;
        // The person's maintenance holds agents before any proof: none is
        // asked, and what the person is told of it stands in for the proof's
        // refusal.
        let held = maintenance.held(&self.selected.name);
        let maintained = held.is_some();
        let (observed, session) = match held {
            Some(held) => {
                let reason = said["reason"].as_str().unwrap_or(&held.error);
                (None, Err(Error::new(held.code, reason)))
            }
            None => match self.verify_machine(&keeper).await {
                Ok(proved) => (Some(proved.observed), proved.session),
                Err(refused) => (None, Err(refused)),
            },
        };
        // A VM Atlas VM says is stopped is said so, not as a connection that
        // failed: a command that needs it starts it.
        let stopped = match (&observed, maintained) {
            (Some(_), _) | (None, true) => None,
            (None, false) => crate::vm::start::stopped_now(self).await,
        };
        let session = match &stopped {
            Some(stopped) => Err(Error::new(stopped.code, stopped.error.clone())),
            None => session,
        };
        let guest = match (&observed, &stopped) {
            (Some(observed), _) => observed.status.clone(),
            (None, Some(stopped)) => json!(stopped),
            (None, None) => {
                let read = json!({"op": "read", "request": {"op": "hello"},
                    "deadline_ms": keeper.timeout.as_millis()});
                match keeper.ask(&read).await {
                    Ok(mut lines) => lines.pop().unwrap_or_else(|| json!(late(self))),
                    Err(error) => json!(error),
                }
            }
        };
        // The station's own Stop, and why: an update's says so.
        let controller = self.controller.clone();
        let station = match tokio::task::spawn_blocking(move || controller.stop_revision()).await {
            Ok(Ok(Some(revision))) => json!({"state": "stopped", "why": revision.why()}),
            Ok(Ok(None)) => json!({"state": "clear"}),
            Ok(Err(_)) | Err(_) => json!({"state": "unknown"}),
        };
        // The changes whose outcome the station does not know yet, read
        // without queueing behind control.
        let controller = self.controller.clone();
        let pending = tokio::task::spawn_blocking(move || controller.pending_within(PENDING_READ))
            .await
            .map_or(json!("unknown"), |pending| {
                pending.map_or(json!("unknown"), |pending| json!(pending))
            });
        // Those of them that keep control from being taken, by the
        // controller's own rule: unknown when they cannot be read.
        let controller = self.controller.clone();
        let unresolved =
            tokio::task::spawn_blocking(move || controller.unresolved_within(PENDING_READ))
                .await
                .map_or(json!("unknown"), |unresolved| {
                    unresolved.map_or(json!("unknown"), |unresolved| json!(unresolved))
                });
        // The guest's own word on the work it started, which may outlive its
        // commands: unknown when it cannot be asked.
        let leftovers = self.leftovers().await.unwrap_or(Leftovers::Unknown);
        let generation = observed.as_ref().map(|observed| observed.generation);
        let mid_update = !session.as_ref().is_ok_and(|session| session.input)
            && crate::vm::proof::mid_update(&guest);
        // What the guest declared in the generation it was observed in, its
        // proof refused or not: none when no observation said it, as no
        // hello read apart from it stands in for it.
        let declared = observed
            .as_ref()
            .and_then(|observed| observed.declared.as_ref())
            .map_or(Value::Null, |declared| json!(declared));
        let mut proof = match &session {
            Ok(session) => json!({"epoch": session.epoch, "capture": session.capture,
                "input": session.input, "generation": generation}),
            Err(refused) => json!({"code": refused.code, "error": refused.error}),
        };
        if session.is_ok() && mid_update {
            proof["inputWhy"] = json!(crate::vm::proof::MID_UPDATE);
        }
        let mut status = json!({
            "ok": true,
            "proven": session.is_ok(),
            "proof": proof,
            "declared": declared,
            "desktop": {"kind": "vm", "name": self.selected.name},
            "keeper": hello,
            "stop": guest_stop(&guest),
            "work": guest_work(&guest),
            "leftovers": leftovers,
            "guest": guest,
            "runner": match (&observed, &stopped) {
                (Some(observed), _) => json!(observed.runner),
                (None, Some(_)) => json!({"ok": true, "power": "stopped", "confirmed": true}),
                (None, None) => Value::Null,
            },
            "station": {"stop": station, "handover": self.handover_unfinished(),
                "maintenance": said,
                "pending": pending, "unresolved": unresolved},
        });
        let judged = readiness_of(&status);
        status["ready"] = json!(judged.is_ok());
        if let Err(why) = judged {
            status["notReady"] = json!(why);
        }
        Ok(status)
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
        if let Some(refused) = self.maintenance().resume_refused(&self.selected.name) {
            return Err(refused);
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

/// Refuse a VM whose guest's `status` says it is open to more than Atlas,
/// in its own words.
fn admitted(status: &Value) -> Result<()> {
    if status["agents"]["admitted"] == true {
        return Ok(());
    }
    let held = status["agents"]["held"]
        .as_str()
        .unwrap_or("Agents wait until the desktop is closed to anyone but Atlas.");
    Err(Error::new(Code::Platform, held))
}

/// The VM `observed` proved as `machine`, as a command verified it: its
/// session, what its guest said, and the proof's epoch, generation and
/// Atlas VM's picture fence.
fn verified_vm(machine: MachineSession, observed: &Observed) -> Verified {
    let status = &observed.status;
    let epoch = &machine.epoch;
    let session = Session {
        epoch: atlas_core::session::Epoch {
            uid: epoch.uid,
            capture_pid: epoch.capture_pid,
            display: epoch.display.clone(),
            security_session_id: epoch.security_session_id,
            display_id: epoch.display_id,
            input_pid: epoch.input_pid,
        },
        capture: machine.capture,
        input: machine.input,
    };
    // Commands that need no input run whether or not it is held; input
    // is refused in the guest's own words before anything is claimed.
    let input_held = (!machine.input).then(|| {
        let held = crate::capability::vm_input_held(status);
        (held.code, held.error)
    });
    Verified {
        session,
        observation: Observation::decode(status),
        machine: Some(Pinned {
            epoch: machine.epoch,
            generation: observed.generation,
            input_held,
            fence: observed.runner.fence(),
        }),
    }
}

/// The keeper's refusal `refused` of an observation of the VM `name`, its
/// code kept; one from a keeper that does not observe says to update.
fn observe_refused(refused: &Value, name: &str) -> Error {
    let code = refused["code"].as_str().and_then(Code::parse);
    if code == Some(Code::BadRequest) && refused["capabilities"].is_array() {
        return Error::new(
            Code::Update,
            format!("{name}'s session keeper is from an older Atlas and cannot prove it."),
        )
        .with(
            "next",
            json!("Update Atlas so the keeper and this command are one build."),
        );
    }
    Error::refused(
        code.unwrap_or(Code::Transport),
        refused,
        "The VM's session keeper did not observe it.",
    )
}

/// Why the guest of the VM `name`, whose hello is `hello`, holds agents, in
/// its own why: its saved Stop not clear, an unknown one included, or its
/// agents stopped; none when they go on.
fn guest_held(name: &str, hello: &Value) -> Option<Error> {
    let resume = "until the person resumes them from the Atlas menu bar";
    let why = match (
        hello["stop"]["state"].as_str(),
        hello["stop"]["why"].as_str(),
    ) {
        (Some("clear"), _) if hello["agents"]["stopped"] == false => return None,
        (Some("clear"), _) => format!("{name} is stopping its agents, so it holds them {resume}."),
        (Some("stopped"), Some("person")) => {
            format!(
                "The person stopped agents on {name}. Ask them before doing anything more there."
            )
        }
        (Some("stopped"), Some("control-lost" | "lapsed")) => {
            format!("{name} lost its session with this Mac, so it holds agents {resume}.")
        }
        (Some("stopped"), Some("restarted")) => format!(
            "{name}'s Atlas started again after work it began may have run on, so it holds agents \
             {resume}."
        ),
        (Some("stopped"), Some("recording")) => format!(
            "{name}'s Atlas restarted to take Screen Recording, so it holds agents {resume}."
        ),
        (Some("stopped"), Some("revoked")) => {
            format!("{name}'s pairing with this Mac was revoked, so it holds agents.")
        }
        (Some("stopped"), _) => format!("{name} holds agents {resume}."),
        _ => format!("{name}'s own Stop cannot be read, so agents wait {resume}."),
    };
    Some(Error::new(Code::Stopped, why))
}

/// The guest's own Stop, as its hello `guest` says it: unknown when it
/// says none.
fn guest_stop(guest: &Value) -> Value {
    if guest["stop"]["state"].is_string() {
        guest["stop"].clone()
    } else {
        json!({"state": "unknown"})
    }
}

/// The work the guest's daemon says it carries now, as its hello `guest`
/// says it: unknown when it says none.
fn guest_work(guest: &Value) -> Value {
    if guest["work"]["state"].is_string() {
        guest["work"].clone()
    } else {
        json!({"state": "unknown"})
    }
}

/// One thing a VM must hold for agents to use it, in the order
/// [`readiness`] judges them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Rule {
    /// Its keeper holds a live session to it.
    Live,
    /// It proves itself to the command's own challenge: hands off, on its
    /// own console, by the paired keys.
    Proven,
    /// Its guest may record its screen and take input, and its Atlas is not
    /// part way through an update.
    Allowed,
    /// Neither the person's Stop nor the guest's holds agents.
    Clear,
    /// It is open to no one but Atlas.
    Admitted,
    /// No update is handing its session over.
    HandedOver,
    /// The person's maintenance does not hold it.
    Maintained,
    /// Its session keeper serves everything this build asks of it, pictures
    /// among them, and Atlas VM keeps a picture fence.
    Kept,
    /// Its Atlas serves everything this build asks of a VM.
    Served,
    /// No change whose claim no live process holds keeps control of it
    /// from being taken.
    Settled,
}

impl Rule {
    const ALL: [Self; 10] = [
        Self::Live,
        Self::Proven,
        Self::Allowed,
        Self::Clear,
        Self::Admitted,
        Self::HandedOver,
        Self::Maintained,
        Self::Kept,
        Self::Served,
        Self::Settled,
    ];

    /// Its name among doctor's checks.
    const fn name(self) -> &'static str {
        match self {
            Self::Live => "Session with the VM",
            Self::Proven => "VM proved",
            Self::Allowed => "Screen Recording and Accessibility allowed",
            Self::Clear => "Agents not stopped",
            Self::Admitted => "Open to no one but Atlas",
            Self::HandedOver => "No update handing it over",
            Self::Maintained => "Not under maintenance",
            Self::Kept => "Session keeper and Atlas VM serve this build",
            Self::Served => "Atlas in it serves this build",
            Self::Settled => "No action of unknown outcome",
        }
    }

    /// The rule that must hold before this one can be judged at all.
    const fn after(self) -> Option<Self> {
        match self {
            Self::Proven | Self::Clear | Self::Served => Some(Self::Live),
            Self::Allowed | Self::Admitted | Self::Kept => Some(Self::Proven),
            Self::Live | Self::HandedOver | Self::Maintained | Self::Settled => None,
        }
    }

    /// Whether it holds of `facts`, or why not, in the guest's own words
    /// where it gives them.
    fn judged(self, facts: &Facts<'_>) -> std::result::Result<(), String> {
        let (status, name) = (facts.status, facts.name);
        let guest = &status["guest"];
        match self {
            Self::Live if status["ok"] == true && status["keeper"]["live"] == true => Ok(()),
            Self::Live => {
                let why = if status["ok"] == true {
                    status["proof"]["error"]
                        .as_str()
                        .or_else(|| status["keeper"]["why"].as_str())
                } else {
                    status["why"].as_str().or_else(|| status["error"].as_str())
                };
                Err(why.map_or_else(
                    || format!("Atlas has no session with {name} now."),
                    str::to_owned,
                ))
            }
            Self::Proven if status["proven"] == true => Ok(()),
            Self::Proven => Err(status["proof"]["error"].as_str().map_or_else(
                || format!("{name} has not proved itself to Atlas."),
                str::to_owned,
            )),
            Self::Allowed if status["proof"]["capture"] != true => {
                Err(crate::capability::vm_capture_held(guest).error)
            }
            Self::Allowed
                if status["proof"]["input"] != true || crate::vm::proof::mid_update(guest) =>
            {
                Err(crate::capability::vm_input_held(guest).error)
            }
            Self::Allowed => Ok(()),
            Self::Clear => match station_held(name, &status["station"]["stop"]) {
                Some(why) => Err(why),
                None => guest_held(name, guest).map_or(Ok(()), |held| Err(held.error)),
            },
            Self::Admitted => admitted(guest).map_err(|refused| refused.error),
            Self::HandedOver => status["station"]["handover"]
                .as_str()
                .map_or(Ok(()), |why| Err(why.to_owned())),
            Self::Maintained => {
                crate::vm::maintenance::holds(name, &status["station"]["maintenance"])
                    .map_or(Ok(()), Err)
            }
            Self::Kept => kept(name, status),
            Self::Served if status["declared"].is_null() => Err(format!(
                "What {name}'s Atlas serves is not known; run it again."
            )),
            Self::Served => match judge(facts.declared, &MACHINE) {
                Usable::Usable => Ok(()),
                Usable::MissingCapabilities { missing } => Err(format!(
                    "{name}'s Atlas is from another build that does not serve all this one needs \
                     ({}); update Atlas so both are one build.",
                    missing.join(", ")
                )),
                Usable::Incompatible { theirs } => Err(format!(
                    "{name}'s Atlas speaks protocol {theirs} and this Mac's {PROTOCOL}; update \
                     Atlas so both are one build."
                )),
            },
            Self::Settled => unsettled(name, facts.pending),
        }
    }

    /// Why it cannot be judged: the rule it comes after does not hold, or,
    /// once Atlas `reached` the VM, what it is judged by was not said.
    fn unknown(self, name: &str, reached: bool) -> Option<String> {
        match self.after()? {
            Self::Live if !reached => Some(format!("Not known until Atlas reaches {name}.")),
            _ => Some(format!("Not known until {name} is proved.")),
        }
    }
}

/// Why the session keeper and Atlas VM of the VM `name`, whose status is
/// `status`, keep it from agents: a keeper that does not serve all this
/// build asks of it, pictures among them, or does not say what the guest
/// declared where it observed it; or an Atlas VM that keeps no picture
/// fence. None when both serve it.
fn kept(name: &str, status: &Value) -> std::result::Result<(), String> {
    let serves = |capability: &str| {
        status["keeper"]["keeper"]["capabilities"]
            .as_array()
            .is_some_and(|served| served.iter().any(|served| served == capability))
    };
    if !crate::vm::keeper::CAPABILITIES
        .iter()
        .all(|capability| serves(capability))
        || status["declared"].is_null()
    {
        return Err(format!(
            "{name}'s session keeper is from an older build; run atlas update."
        ));
    }
    let runner = &status["runner"];
    if !runner["incarnation"].is_string() || !runner["fence"].is_u64() {
        return Err(format!(
            "{name}'s Atlas VM is from an older build and keeps no picture fence; run atlas update."
        ));
    }
    Ok(())
}

/// Why the Stop this Mac keeps for the VM `name`, as its status says it,
/// `stop`, holds agents: none when it is clear, and unknown is never clear.
fn station_held(name: &str, stop: &Value) -> Option<String> {
    let resume = "until the person resumes them from the Atlas menu bar";
    match (stop["state"].as_str(), stop["why"].as_str()) {
        (Some("clear"), _) => None,
        (Some("stopped"), Some("update")) => Some(format!(
            "An update of Atlas stopped agents on {name}; they wait {resume}."
        )),
        (Some("stopped"), _) => Some(format!(
            "The person stopped agents on {name}. Ask them before doing anything more there."
        )),
        _ => Some(format!(
            "The Stop this Mac keeps for {name} cannot be read, so agents wait {resume}."
        )),
    }
}

/// What [`readiness`] judges a VM by.
struct Facts<'a> {
    status: &'a Value,
    declared: &'a Declared,
    pending: &'a Value,
    /// The VM's name, as its status gives it.
    name: &'a str,
}

impl<'a> Facts<'a> {
    fn new(status: &'a Value, declared: &'a Declared, pending: &'a Value) -> Self {
        Self {
            status,
            declared,
            pending,
            name: status["desktop"]["name"].as_str().unwrap_or("the VM"),
        }
    }
}

/// Whether the VM whose status is `status`, whose guest declared
/// `declared`, and whose station holds `pending` (the changes that keep
/// control of it from being taken, as
/// [`atlas_control::Controller::unresolved_within`] reads them, else
/// `"unknown"`), is ready for agents: every [`Rule`] holding, in order. Why
/// not is the first that fails. Work its guest started that still runs,
/// and a change a live process is making now, never make it not ready.
pub(crate) fn readiness(
    status: &Value,
    declared: &Declared,
    pending: &Value,
) -> std::result::Result<(), String> {
    let facts = Facts::new(status, declared, pending);
    Rule::ALL
        .into_iter()
        .try_for_each(|rule| rule.judged(&facts))
}

/// [`readiness`] of the VM whose status is `status`, as it says what its
/// guest declared and what its station holds pending.
pub(crate) fn readiness_of(status: &Value) -> std::result::Result<(), String> {
    readiness(
        status,
        &declared_of(status),
        &status["station"]["unresolved"],
    )
}

/// Doctor's check of each [`Rule`] of the VM whose status is `status`, in
/// order: whether it holds, and why not; not known while one it comes after
/// does not hold.
pub(crate) fn readiness_checks(status: &Value) -> Vec<Value> {
    let declared = declared_of(status);
    let facts = Facts::new(status, &declared, &status["station"]["unresolved"]);
    let mut held = Vec::new();
    Rule::ALL
        .into_iter()
        .map(|rule| {
            let judged = match rule.after() {
                Some(before) if !held.contains(&before) => None,
                // What the guest declared is not known while no observation
                // said it: no hello read apart from one stands in for it.
                _ if rule == Rule::Served && status["declared"].is_null() => None,
                _ => Some(rule.judged(&facts)),
            };
            let (ok, next) = match judged {
                Some(Ok(())) => {
                    held.push(rule);
                    (Some(true), None)
                }
                Some(Err(why)) => (Some(false), Some(why)),
                None => (None, rule.unknown(facts.name, held.contains(&Rule::Live))),
            };
            json!({"name": rule.name(), "ok": ok, "nextStep": next})
        })
        .collect()
}

/// What the guest of the VM whose status is `status` declared, in the
/// generation it was observed in; nothing when no observation said it,
/// never an earlier hello's.
fn declared_of(status: &Value) -> Declared {
    serde_json::from_value(status["declared"].clone()).unwrap_or_else(|_| Declared {
        protocol: PROTOCOL,
        capabilities: Vec::new(),
        version: String::new(),
    })
}

/// Why changes the station holds on the VM `name`, `pending`, keep it from
/// agents: none when nothing does, and unknown is never nothing. A change a
/// process is proven to be making is work in progress, as a running command
/// is; one whose holder is gone, or cannot be told, keeps it.
fn unsettled(name: &str, pending: &Value) -> std::result::Result<(), String> {
    let Some(unresolved) = pending.as_array() else {
        return Err(format!(
            "Which of Atlas's changes on {name} are still unsettled cannot be read, so agents \
             wait."
        ));
    };
    let Some(left) = unresolved.iter().find(|change| change["holder"] != "live") else {
        return Ok(());
    };
    if left["holder"] != "gone" {
        return Err(format!(
            "Whether a change to {name} is still being made cannot be told, so agents wait."
        ));
    }
    Err(match left["executor"].as_str() {
        Some("vm") => format!(
            "Atlas VM has not said yet what a change it was making to {name} came to, so \
             agents wait until it does."
        ),
        Some("guest") => crate::host_session::settled_on_its_own(name),
        Some("station") => format!(
            "What a change Atlas made on this Mac for {name} came to is not known, so agents \
             wait."
        ),
        _ => format!("What a change on {name} came to is not known, so agents wait."),
    })
}

/// What the status of the VM `name` says of changes being made there now,
/// `pending`, for the person: none when no process is proven to be making
/// one.
pub(crate) fn under_way(name: &str, pending: &Value) -> Option<String> {
    pending
        .as_array()?
        .iter()
        .any(|change| change["holder"] == "live")
        .then(|| format!("A change to {name} is still being made."))
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

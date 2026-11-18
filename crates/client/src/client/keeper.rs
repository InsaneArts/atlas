//! What a client asks of a VM desktop through its session keeper: its status
//! with a fresh proof of it, never ready yet; the proof a command acts in,
//! and its guest's own Stop; the guest's word on work it started that may
//! still run; the person's Stop and Resume; and the recovery of changes
//! whose command ended without an answer. Everything else stays refused in
//! the one wording for what this build cannot do on a VM.
use super::{Actor, Client, Pinned, Verified};
use crate::{
    link::{Keeper, Link},
    vm::proof::Observed,
};
use atlas_core::{
    config::desktops::{Admission, Chosen},
    error::{Code, Error, Result},
    files::Token,
    session::{MachineSession, Observation, Session},
};
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
/// making of it, `unfinished`, when one is: its code and words kept.
fn handing_over(error: Error, unfinished: Option<String>) -> Error {
    match unfinished {
        Some(why) => error.with("station", json!({"handover": why})),
        None => error,
    }
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
        let session = paired.and_then(|paired| observed.prove(&self.desktop(), &paired));
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
    /// of it, read through the keeper. It is never ready yet, proven or not.
    pub(super) async fn vm_status(&self) -> Result<Value> {
        // A VM still being added answers nothing but the person's Stop.
        if self.admission() != Admission::Open {
            return Err(self.selected.unreachable());
        }
        // An update still handing it over is said on every path, the
        // keeper's own failure kept.
        let handing = |error| handing_over(error, self.handover_unfinished());
        let keeper = self.keeper().await.map_err(handing)?;
        let hello = keeper.one(&json!({"op": "hello"})).await.map_err(handing)?;
        let (observed, session) = match self.verify_machine(&keeper).await {
            Ok(proved) => (Some(proved.observed), proved.session),
            Err(refused) => (None, Err(refused)),
        };
        // A VM Atlas VM says is stopped is said so, not as a connection that
        // failed: a command that needs it starts it.
        let stopped = match &observed {
            Some(_) => None,
            None => crate::vm::start::stopped_now(self).await,
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
        // The guest's own word on the work it started, which may outlive its
        // commands: unknown when it cannot be asked.
        let leftovers = self.leftovers().await.unwrap_or(Leftovers::Unknown);
        let generation = observed.as_ref().map(|observed| observed.generation);
        let mid_update = !session.as_ref().is_ok_and(|session| session.input)
            && crate::vm::proof::mid_update(&guest);
        let (proof, not_ready) = match &session {
            Ok(session) if mid_update => (
                json!({"epoch": session.epoch, "capture": session.capture,
                    "input": session.input, "generation": generation,
                    "inputWhy": crate::vm::proof::MID_UPDATE}),
                crate::vm::proof::MID_UPDATE.to_owned(),
            ),
            Ok(session) => (
                json!({"epoch": session.epoch, "capture": session.capture,
                    "input": session.input, "generation": generation}),
                format!(
                    "{} is proven: agents run commands and send input there, but this build of \
                     Atlas cannot see its screen yet.",
                    self.selected.name
                ),
            ),
            Err(refused) => (
                json!({"code": refused.code, "error": refused.error}),
                refused.error.clone(),
            ),
        };
        Ok(json!({
            "ok": true,
            "proven": session.is_ok(),
            "proof": proof,
            "ready": false,
            "notReady": not_ready,
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
                "pending": pending},
        }))
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

//! The add's means as the system gives them.
use super::{Granted, Means, Pairing, Posture, Progress};
use crate::{
    link::Link,
    station::{Station, enrolment::Enrolments},
    vm::{
        StationHold, Vms,
        lifecycle::{Provenance, barrier::Admitted, grant, record::Record},
        runner::{Operation, Reply, Runner},
    },
};
use atlas_control::{Controller, executors::Executor};
use atlas_core::{
    config::desktops::Selected,
    desktop::DesktopId,
    error::{Code, Error, Result},
    files::Token,
};
use atlas_session::pairing::{Enrolment, StationRecords};
use atlas_session::{compatibility::RESUME_OUTCOME, control::GuestStop};
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

/// How long the guest is given to answer through its keeper.
const ASKING: Duration = Duration::from_secs(10);
/// How long the keeper is given to say the add's own Resume and hear what
/// it came to, and the moment past it for its own answer.
const RESUMING: Duration = Duration::from_secs(30);
const ANSWERING: Duration = Duration::from_secs(5);
/// How long the claim on the add's own Resume waits for the station's
/// pending changes, which another process may hold for a moment.
const CLAIMING: Duration = Duration::from_secs(5);

/// The keeper of `vm`, as `selected` names it, reached now.
fn keeper_of(selected: &Selected, vm: &DesktopId) -> Result<crate::link::Keeper> {
    match crate::link::probe(selected, vm, ASKING) {
        Link::Keeper(keeper) => Ok(keeper),
        Link::Unreachable(unreachable) => Err(Error::new(
            Code::Transport,
            unreachable
                .why
                .unwrap_or_else(|| format!("{} is not reachable yet", selected.name)),
        )),
        Link::Unix(_) | Link::Session(_) => Err(Error::new(Code::State, "That is no VM's keeper.")),
    }
}

/// The exclusion of the VM `vm`, which keeps this Mac's Stop of it and the
/// station's claims on its changes.
fn controller(vm: &DesktopId) -> Result<Controller> {
    Controller::new(&atlas_core::desktop::Desktop::from(atlas_core::vm::Vm {
        id: vm.clone(),
    }))
}

/// The system as it is.
pub(super) struct Real;

impl Means for Real {
    fn grant(&self, operation: &grant::Operation) -> Result<Granted> {
        grant::ask(operation.clone())?
            .take(operation)
            .map(|_| Granted::taken())
    }

    fn provenance(&self) -> Result<Provenance> {
        Provenance::own()
    }

    fn preflight(&self) -> Result<()> {
        crate::vm::lifecycle::preflight_station_key()
    }

    fn hold<'a>(&self, vm: &DesktopId, admitted: &'a Admitted) -> Result<Option<StationHold<'a>>> {
        crate::vm::hold(vm, admitted)
    }

    fn agent(&self, hold: &StationHold<'_>) -> Result<Value> {
        crate::vm::install_agent_held(hold)
    }

    fn keeper(&self, hold: &StationHold<'_>, settings: &Path) -> Result<Value> {
        crate::vm::keeper::install_keeper_held(hold, settings)
    }

    fn read(&self, vm: &DesktopId, operation: &Operation) -> Result<Reply> {
        Runner::of(vm)?.read(operation)
    }

    fn change(
        &self,
        vm: &DesktopId,
        record: &mut Record,
        keep: &dyn Fn(&Record) -> Result<()>,
        operation: &Operation,
    ) -> Result<Reply> {
        Runner::of(vm)?.change(record, keep, operation)
    }

    fn progress(&self, vm: &DesktopId) -> Result<Option<Progress>> {
        Progress::read(&Vms::own()?.folder(vm).join("Bootstrap.json"))
    }

    fn bundle(&self) -> Result<PathBuf> {
        atlas_core::app_bundle::installed_app()
    }

    fn enrolments(&self) -> &dyn Enrolments {
        &crate::station::enrolment::Keychain
    }

    fn pairing(&self, vm: &DesktopId) -> Result<Pairing> {
        Ok(match Station::own()?.pairing(vm)?.read()? {
            Some(Enrolment::Paired(_)) => Pairing::Paired,
            Some(Enrolment::Revoked { .. }) => Pairing::Revoked,
            Some(Enrolment::Pending { .. }) | None => Pairing::Unpaired,
        })
    }

    fn posture(&self, selected: &Selected, vm: &DesktopId) -> Result<Posture> {
        let keeper = keeper_of(selected, vm)?;
        let read = json!({"op": "read", "request": {"op": "hello"},
            "deadline_ms": ASKING.as_millis()});
        let hello = keeper.exchange(&read, ASKING)?.pop().ok_or_else(|| {
            Error::new(
                Code::Transport,
                format!("{}'s guest did not answer.", selected.name),
            )
        })?;
        if hello["ok"] != true {
            return Err(Error::new(
                Code::Transport,
                hello["error"].as_str().map_or_else(
                    || format!("{}'s guest did not answer.", selected.name),
                    str::to_owned,
                ),
            ));
        }
        Ok(Posture::of(&hello))
    }

    fn stopped(&self, vm: &DesktopId) -> bool {
        !matches!(
            controller(vm).and_then(|controller| controller.stop_revision()),
            Ok(None)
        )
    }

    fn keeps_resumes(&self, selected: &Selected, vm: &DesktopId) -> Result<bool> {
        let hello = keeper_of(selected, vm)?
            .exchange(&json!({"op": "hello"}), ASKING)?
            .pop()
            .unwrap_or_default();
        let names = |declared: &Value| {
            declared
                .as_array()
                .is_some_and(|names| names.iter().any(|name| name == RESUME_OUTCOME))
        };
        Ok(hello["ok"] == true
            && hello["live"] == true
            && names(&hello["keeper"]["capabilities"])
            && names(&hello["guest"]["capabilities"]))
    }

    fn claim(&self, vm: &DesktopId, change: &Token) -> Result<()> {
        // Its claim file goes as this returns; the marker keeps naming it,
        // the station's own, until the add clears it.
        controller(vm)?
            .quarantine_by_within(change.as_str(), Executor::Station, CLAIMING)
            .map(drop)
    }

    fn resume(
        &self,
        selected: &Selected,
        vm: &DesktopId,
        change: &Token,
        only: &GuestStop,
    ) -> Result<Value> {
        let keeper = keeper_of(selected, vm)?;
        let capability = crate::vm::keeper::resume_capability(vm)?;
        // This Mac's Stop clear, as the keeper reads it again under its turn
        // and once more as it says it.
        let asked = json!({"op": "resume", "capability": capability, "revision": null,
            "change": change, "only": only, "deadline_ms": RESUMING.as_millis()});
        keeper
            .exchange(&asked, RESUMING + ANSWERING)?
            .pop()
            .ok_or_else(|| Error::new(Code::Transport, "The keeper did not answer the Resume."))
    }

    fn word(&self, selected: &Selected, vm: &DesktopId, change: &Token) -> Result<Value> {
        keeper_of(selected, vm)?
            .exchange(
                &json!({"op": "resume-word", "change": change}),
                ASKING + ANSWERING,
            )?
            .pop()
            .ok_or_else(|| {
                Error::new(
                    Code::Transport,
                    format!("{}'s guest did not answer.", selected.name),
                )
            })
    }

    fn settle(&self, selected: &Selected, vm: &DesktopId, change: &Token) -> Result<()> {
        controller(vm)?.clear_known_within(change.as_str(), CLAIMING)?;
        // Best effort: unclaimed, the keeper settles it at the guest's next
        // hello all the same.
        if let Ok(keeper) = keeper_of(selected, vm) {
            let _ = keeper.exchange(&json!({"op": "settle", "changes": [change]}), ASKING);
        }
        Ok(())
    }

    fn wait(&self, how_long: Duration) {
        std::thread::sleep(how_long);
    }

    fn now(&self) -> Instant {
        Instant::now()
    }
}

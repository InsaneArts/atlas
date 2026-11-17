//! The add's means as the system gives them.
use super::{Means, Pairing, Posture, Progress};
use crate::{
    link::Link,
    station::{Station, enrolment::Enrolments},
    vm::{
        StationHold, Vms,
        lifecycle::{Provenance, barrier::Admitted, grant, record::Record},
        runner::{Operation, Reply, Runner},
    },
};
use atlas_core::{
    config::desktops::Selected,
    desktop::DesktopId,
    error::{Code, Error, Result},
};
use atlas_session::pairing::{Enrolment, StationRecords};
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

/// How long the guest is given to answer through its keeper.
const ASKING: Duration = Duration::from_secs(10);

/// The system as it is.
pub(super) struct Real;

impl Means for Real {
    fn grant(&self, operation: &grant::Operation) -> Result<()> {
        grant::ask(operation.clone())?.take(operation).map(|_| ())
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
        let keeper = match crate::link::probe(selected, vm, ASKING) {
            Link::Keeper(keeper) => keeper,
            Link::Unreachable(unreachable) => {
                return Err(Error::new(
                    Code::Transport,
                    unreachable
                        .why
                        .unwrap_or_else(|| format!("{} is not reachable yet", selected.name)),
                ));
            }
            Link::Unix(_) | Link::Session(_) => {
                return Err(Error::new(Code::State, "That is no VM's keeper."));
            }
        };
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

    fn wait(&self, how_long: Duration) {
        std::thread::sleep(how_long);
    }
}

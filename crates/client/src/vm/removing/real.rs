//! The removal's means as the system gives them.
use super::Means;
use crate::vm::{
    StationHold,
    lifecycle::{Provenance, barrier::Admitted, grant},
};
use atlas_core::{desktop::DesktopId, error::Result, files::Token};
use serde_json::Value;
use std::time::Duration;

/// The system as it is.
pub(super) struct Real;

impl Means for Real {
    fn grant(&self, operation: &grant::Operation) -> Result<Token> {
        grant::ask(operation.clone())?.take(operation)
    }

    fn provenance(&self) -> Result<Provenance> {
        Provenance::own()
    }

    fn hold<'a>(&self, vm: &DesktopId, admitted: &'a Admitted) -> Result<Option<StationHold<'a>>> {
        crate::vm::hold(vm, admitted)
    }

    fn remove(&self, hold: &StationHold<'_>, change: &Token) -> Result<Value> {
        crate::vm::remove_held(hold, change)
    }

    fn wait(&self, how_long: Duration) {
        std::thread::sleep(how_long);
    }
}

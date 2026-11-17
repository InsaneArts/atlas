//! Every desktop of this account, as `atlas desktop list` shows them: each
//! the settings list, by kind, and each VM whose add has not reached the
//! settings yet, as being added. A VM's power is Atlas VM's word, seen just
//! now or last known, apart from whether its keeper holds a session to its
//! guest; and its lifecycle is its record's.
use super::{
    lifecycle::{
        Lifecycle,
        record::{Record, State},
    },
    runner::{Operation, Reply, Runner},
};
use crate::link::Link;
use atlas_core::{
    config::desktops::{Admission, Chosen, Selected, Settings},
    desktop::DesktopId,
    error::{Error, Result},
};
use serde_json::{Value, json};
use std::{path::Path, time::Duration};

/// How long a VM's keeper is given to answer.
const ASKING: Duration = Duration::from_secs(3);

/// What the listing asks of the system, given apart so tests stand in.
trait Looks {
    /// Atlas VM's word on `vm`'s power.
    fn power(&self, vm: &DesktopId) -> Result<Reply>;
    /// Whether `vm`'s keeper, as `selected` names it, holds a session.
    fn reach(&self, selected: &Selected, vm: &DesktopId) -> Value;
}

/// Every desktop the settings at `settings` list, and every VM being added
/// to them.
///
/// # Errors
///
/// Fails when the settings are there but do not load, or the VMs'
/// lifecycle records cannot be read.
pub fn list(settings: &Path) -> Result<Value> {
    list_with(&Lifecycle::own()?, &Looking, settings)
}

fn list_with(lifecycle: &Lifecycle, looks: &impl Looks, path: &Path) -> Result<Value> {
    let settings = match std::fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(Error::writing(path, error)),
        Ok(_) => Some(Settings::load(path)?),
    };
    let records = if lifecycle.kept_any()? {
        lifecycle.records()?
    } else {
        Vec::new()
    };
    let record_of = |id: &DesktopId| records.iter().find(|record| record.id == *id);
    let mut desktops = Vec::new();
    if let Some(settings) = &settings {
        let default = &settings.default_desktop().name;
        for listed in settings.desktops() {
            let mut shown = json!({"name": listed.name, "default": listed.name == *default});
            match &listed.desktop {
                Chosen::User(_) => shown["kind"] = json!("user"),
                Chosen::Vm(config) => {
                    let selected = settings.selected(listed);
                    let record = record_of(&config.id);
                    if record.is_some_and(|record| record.state == State::Adding) {
                        selected.adding.set(Admission::Adding);
                    }
                    vm(&mut shown, &config.id, record, true, looks);
                    shown["reach"] = if selected.adding.get() == Admission::Adding {
                        json!({"reachable": false, "why": selected.unreachable().error})
                    } else {
                        looks.reach(&selected, &config.id)
                    };
                }
            }
            desktops.push(shown);
        }
    }
    let listed = |id: &DesktopId| {
        settings.as_ref().is_some_and(|settings| {
            settings
                .desktops()
                .iter()
                .any(|listed| matches!(&listed.desktop, Chosen::Vm(config) if config.id == *id))
        })
    };
    for record in records.iter().filter(|record| !listed(&record.id)) {
        let mut shown = json!({"name": record.name, "default": false});
        vm(&mut shown, &record.id, Some(record), false, looks);
        shown["reach"] = json!({"reachable": false, "why": "It is not in the settings yet."});
        desktops.push(shown);
    }
    Ok(json!({
        "ok": true,
        "settings": atlas_core::config::tilde(path),
        "desktops": desktops,
    }))
}

/// Fill `shown` with what is known of the VM `id`: its record's lifecycle
/// (being added when it has a record but is not `listed`), and its power.
fn vm(
    shown: &mut Value,
    id: &DesktopId,
    record: Option<&Record>,
    listed: bool,
    looks: &impl Looks,
) {
    shown["kind"] = json!("vm");
    shown["id"] = json!(id);
    shown["listed"] = json!(listed);
    shown["lifecycle"] = match record.map(|record| record.state) {
        Some(State::Adding) => json!("adding"),
        Some(State::Created) => json!("created"),
        Some(State::Removing) => json!("removing"),
        None => Value::Null,
    };
    if let Some(failure) = record.and_then(|record| record.failure.as_ref()) {
        shown["failure"] = json!(failure.error);
    }
    shown["power"] = match looks.power(id) {
        Ok(reply) if reply.ok && reply.power.is_some() => json!({
            "state": reply.power,
            "confirmed": reply.confirmed == Some(true),
        }),
        Ok(reply) => json!({
            "state": "unknown",
            "why": reply.error.unwrap_or_else(|| "Atlas VM did not say.".to_owned()),
        }),
        Err(error) => json!({"state": "unknown", "why": error.error}),
    };
}

/// The system as it is.
struct Looking;

impl Looks for Looking {
    fn power(&self, vm: &DesktopId) -> Result<Reply> {
        Runner::of(vm)?.read(&Operation::State)
    }

    fn reach(&self, selected: &Selected, vm: &DesktopId) -> Value {
        match crate::link::probe(selected, vm, ASKING) {
            Link::Keeper(keeper) => match keeper.exchange(&json!({"op": "hello"}), ASKING) {
                Ok(mut lines) => {
                    let live = lines.pop().is_some_and(|hello| hello["live"] == true);
                    let mut reach = json!({"reachable": live});
                    if !live {
                        reach["why"] = json!(format!(
                            "{}'s session keeper holds no session to it now",
                            selected.name
                        ));
                    }
                    reach
                }
                Err(error) => json!({"reachable": false, "why": error.error}),
            },
            Link::Unreachable(unreachable) => {
                json!({"reachable": false, "why": unreachable.why})
            }
            Link::Unix(_) | Link::Session(_) => json!({"reachable": false}),
        }
    }
}

#[cfg(test)]
#[path = "listing_tests.rs"]
mod tests;

//! Every desktop of this account, as `atlas desktop list` shows them: each
//! the settings list, by kind, and each VM whose add has not reached the
//! settings yet, as being added. A VM's power is Atlas VM's word, seen just
//! now or last known, apart from whether its keeper holds a session to its
//! guest; its lifecycle is its record's; and the rest of what is known of
//! it, its facts, is built where a status builds them.
use super::{
    facts::{Seen, Word, facts, reported_system},
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
    /// Atlas VM's answer to the read `operation` of `vm`.
    fn ask(&self, vm: &DesktopId, operation: &Operation) -> Result<Reply>;
    /// Whether `vm`'s keeper, as `selected` names it, holds a session, and
    /// its hello when it answered one.
    fn reach(&self, selected: &Selected, vm: &DesktopId) -> (Value, Value);
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
                    let (reach, keeper) = if selected.adding.get() == Admission::Adding {
                        let why = selected.unreachable().error;
                        (json!({"reachable": false, "why": why}), Value::Null)
                    } else {
                        looks.reach(&selected, &config.id)
                    };
                    let vm = Vm {
                        id: &config.id,
                        name: &listed.name,
                        record,
                        listed: true,
                    };
                    vm.show(&mut shown, lifecycle, &keeper, looks);
                    shown["reach"] = reach;
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
        let vm = Vm {
            id: &record.id,
            name: &record.name,
            record: Some(record),
            listed: false,
        };
        vm.show(&mut shown, lifecycle, &Value::Null, looks);
        shown["reach"] = json!({"reachable": false, "why": "It is not in the settings yet."});
        desktops.push(shown);
    }
    Ok(json!({
        "ok": true,
        "settings": atlas_core::config::tilde(path),
        "desktops": desktops,
    }))
}

/// A VM as the listing shows it: its record, when it has one, and whether
/// the settings list it.
struct Vm<'a> {
    id: &'a DesktopId,
    name: &'a str,
    record: Option<&'a Record>,
    listed: bool,
}

impl Vm<'_> {
    /// Fill `shown` with what is known of it: its record's lifecycle (being
    /// added when it has a record but is not listed), its power, and its
    /// facts, its keeper's hello `keeper` among what they are built from.
    fn show(&self, shown: &mut Value, lifecycle: &Lifecycle, keeper: &Value, looks: &impl Looks) {
        shown["kind"] = json!("vm");
        shown["id"] = json!(self.id);
        shown["listed"] = json!(self.listed);
        shown["lifecycle"] = match self.record.map(|record| record.state) {
            Some(State::Adding) => json!("adding"),
            Some(State::Created) => json!("created"),
            Some(State::Removing) => json!("removing"),
            None => Value::Null,
        };
        if let Some(failure) = self.record.and_then(|record| record.failure.as_ref()) {
            shown["failure"] = json!(failure.error);
        }
        let word = Word::asked(|operation| looks.ask(self.id, operation));
        shown["power"] = match &word.state {
            Ok(reply) if reply.ok && reply.power.is_some() => json!({
                "state": reply.power,
                "confirmed": reply.confirmed == Some(true),
            }),
            Ok(reply) => json!({
                "state": "unknown",
                "why": reply.error.as_deref().unwrap_or("Atlas VM did not say."),
            }),
            Err(error) => json!({"state": "unknown", "why": error.error}),
        };
        let status = json!({"keeper": keeper});
        let built = facts(&Seen {
            lifecycle,
            vm: self.id,
            name: self.name,
            record: self.record,
            word: &word,
            system: &reported_system(&status),
            atlas: crate::client::guest_update(self.name, &status),
        });
        if let (Some(shown), Value::Object(built)) = (shown.as_object_mut(), built) {
            shown.extend(built);
        }
    }
}

/// The system as it is.
struct Looking;

impl Looks for Looking {
    fn ask(&self, vm: &DesktopId, operation: &Operation) -> Result<Reply> {
        Runner::of(vm)?.read_within(operation, ASKING)
    }

    fn reach(&self, selected: &Selected, vm: &DesktopId) -> (Value, Value) {
        match crate::link::probe(selected, vm, ASKING) {
            Link::Keeper(keeper) => match keeper.exchange(&json!({"op": "hello"}), ASKING) {
                Ok(mut lines) => {
                    let hello = lines.pop().unwrap_or(Value::Null);
                    let live = hello["live"] == true;
                    let mut reach = json!({"reachable": live});
                    if !live {
                        reach["why"] = json!(format!(
                            "{}'s session keeper holds no session to it now",
                            selected.name
                        ));
                    }
                    (reach, hello)
                }
                Err(error) => (json!({"reachable": false, "why": error.error}), Value::Null),
            },
            Link::Unreachable(unreachable) => (
                json!({"reachable": false, "why": unreachable.why}),
                Value::Null,
            ),
            Link::Unix(_) | Link::Session(_) => (json!({"reachable": false}), Value::Null),
        }
    }
}

#[cfg(test)]
#[path = "listing_tests.rs"]
mod tests;

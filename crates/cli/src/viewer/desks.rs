//! Every desktop the settings list, as the service serves them: a client of
//! each, kept while its settings stay the same, and what its newest status
//! read said of it, for the list the app shows. Each is read by the upkeep,
//! so a desktop not viewed is kept up too. The one viewed is the viewer's
//! own client, whose control, frames and sign-in are the viewer's.
use super::{Viewer, unpoisoned};
use atlas_client::client::{Actor, Client, PersonInput};
use atlas_core::{
    config::desktops::{Selected, Settings},
    desktop::Kind,
    error::{Code, Error, Result},
};
use serde_json::{Map, Value, json};
use std::{collections::BTreeMap, sync::Arc};

/// What a VM's status says of it that the list shows: Atlas VM's, the
/// guest's and the station's own words.
const FACTS: [&str; 8] = [
    "progress",
    "steps",
    "disk",
    "macos",
    "maintenance",
    "atlas",
    "adder",
    "can",
];

/// One desktop the settings list.
#[derive(Clone)]
pub(super) struct Desk {
    /// The settings' name for it.
    pub(super) name: String,
    pub(super) client: Arc<Client>,
    /// Whether it is the settings' default.
    pub(super) default: bool,
}

/// The desktops the service serves, and what each one's newest status read
/// said, by its ID.
#[derive(Default)]
pub(super) struct Desks {
    listed: std::sync::Mutex<Vec<Desk>>,
    seen: std::sync::Mutex<BTreeMap<String, Value>>,
}

impl Viewer {
    /// Every desktop served, in the settings' order, the one viewed as the
    /// viewer serves it. Before any settings are followed, or with none,
    /// the one viewed is the only one.
    pub(super) fn desks(&self) -> Vec<Desk> {
        let viewed = self.client();
        let listed = unpoisoned(&self.desks.listed).clone();
        if listed.is_empty() {
            return vec![Desk {
                name: viewed.selected.name.clone(),
                client: viewed,
                default: true,
            }];
        }
        let id = viewed.desktop().id();
        listed
            .into_iter()
            .map(|desk| {
                if desk.client.desktop().id() == id {
                    Desk {
                        client: viewed.clone(),
                        ..desk
                    }
                } else {
                    desk
                }
            })
            .collect()
    }

    /// Whether more than one desktop is served, so each request names its
    /// own.
    pub(super) fn several(&self) -> bool {
        unpoisoned(&self.desks.listed).len() > 1
    }

    /// The desktop served whose ID is `id`.
    pub(super) fn desk(&self, id: &str) -> Option<Desk> {
        self.desks()
            .into_iter()
            .find(|desk| desk.client.desktop().id().as_str() == id)
    }

    /// Serve every desktop `settings` list: a client kept for each whose
    /// settings are unchanged, a new one for each other, made off the
    /// runtime, as the person's own.
    pub(super) async fn list_desks(self: &Arc<Self>, settings: &Settings) {
        let default = settings.default_desktop().name.clone();
        let wanted: Vec<(Selected, bool)> = settings
            .desktops()
            .iter()
            .map(|listed| (settings.selected(listed), listed.name == default))
            .collect();
        let kept = unpoisoned(&self.desks.listed).clone();
        let viewer = self.clone();
        let made = tokio::task::spawn_blocking(move || {
            wanted
                .into_iter()
                .filter_map(|(selected, default)| {
                    let name = selected.name.clone();
                    let client = match kept.iter().find(|desk| desk.client.selected == selected) {
                        Some(desk) => desk.client.clone(),
                        None => Arc::new(viewer.made(selected).ok()?),
                    };
                    Some(Desk {
                        name,
                        client,
                        default,
                    })
                })
                .collect::<Vec<_>>()
        })
        .await;
        let Ok(desks) = made else {
            return;
        };
        let ids: Vec<String> = desks
            .iter()
            .map(|desk| desk.client.desktop().id().as_str().to_owned())
            .collect();
        unpoisoned(&self.desks.seen).retain(|id, _| ids.contains(id));
        *unpoisoned(&self.desks.listed) = desks;
    }

    /// A client of the desktop `selected` names, acting for the person
    /// through this viewer. It may block on a VM's keeper for a moment.
    ///
    /// # Errors
    ///
    /// As [`Client::new`].
    pub(super) fn made(&self, selected: Selected) -> Result<Client> {
        #[cfg(test)]
        let stand_in = unpoisoned(&self.stand_ins)
            .iter()
            .find(|client| client.desktop() == selected.desktop())
            .cloned();
        #[cfg(not(test))]
        let stand_in: Option<Client> = None;
        let mut client = match stand_in {
            Some(mut stand_in) => {
                stand_in.selected = selected;
                stand_in
            }
            None => Client::new(selected)?,
        };
        client.actor = Actor::Viewer;
        client.person_input = Some(PersonInput::default());
        Ok(client)
    }

    /// Every desktop the settings list now, read afresh, each with its
    /// client or why none could be made, for the person's Stop: a desktop
    /// listed since the last read is made now, and one that cannot be is
    /// named with its failure, never left out. Nothing served changes, the
    /// one viewed included. With no settings followed, the desktops served;
    /// with settings that no longer read, those too, and the failure named.
    pub(super) async fn every_desktop_now(self: &Arc<Self>) -> Vec<(String, Result<Arc<Client>>)> {
        let served = self.desks();
        let Some(path) = self.settings.clone() else {
            return served
                .into_iter()
                .map(|desk| (desk.name, Ok(desk.client)))
                .collect();
        };
        let viewer = self.clone();
        let read = tokio::task::spawn_blocking(move || {
            let settings = Settings::load(&path)?;
            let each = settings
                .desktops()
                .iter()
                .map(|listed| {
                    let desktop = listed.desktop.desktop();
                    let kept = served
                        .iter()
                        .find(|desk| desk.client.desktop() == desktop)
                        .map(|desk| desk.client.clone());
                    let client = kept
                        .map_or_else(|| viewer.made(settings.selected(listed)).map(Arc::new), Ok);
                    (listed.name.clone(), client)
                })
                .collect();
            Ok::<_, Error>(each)
        })
        .await;
        let unread = match read {
            Ok(Ok(each)) => return each,
            Ok(Err(error)) => error,
            Err(_) => Error::new(Code::Interrupted, "The settings could not be read."),
        };
        self.desks()
            .into_iter()
            .map(|desk| (desk.name, Ok(desk.client)))
            .chain([("settings".to_owned(), Err(unread))])
            .collect()
    }

    /// Keep what a status read of `client` said of it, for the list.
    pub(super) fn note_seen(&self, client: &Client, status: &Value) {
        let mut seen = json!({"standing": status["overview"]["state"]});
        if let Kind::Vm(_) = client.desktop().kind() {
            let facts: Map<String, Value> = FACTS
                .iter()
                .filter_map(|fact| Some(((*fact).to_owned(), status.get(*fact)?.clone())))
                .collect();
            seen["facts"] = Value::Object(facts);
        }
        unpoisoned(&self.desks.seen).insert(client.desktop().id().as_str().to_owned(), seen);
    }

    /// Every desktop served, as the app lists them: each one's ID, record
    /// key, name and kind, whether it is the default and the one viewed,
    /// its standing as its newest status read said it, and a VM's facts;
    /// null while none was read.
    pub(super) fn listed(&self) -> Value {
        let viewed = self.client().desktop().id();
        let seen = unpoisoned(&self.desks.seen).clone();
        let desktops: Vec<Value> = self
            .desks()
            .iter()
            .map(|desk| {
                let desktop = desk.client.desktop();
                let id = desktop.id();
                let said = seen.get(id.as_str());
                let mut shown = json!({
                    "id": id.as_str(),
                    "key": desktop.record_key().as_str(),
                    "name": desk.name,
                    "kind": crate::ui::desktop::of(&desk.name, &desktop)["kind"],
                    "default": desk.default,
                    "viewed": id == viewed,
                    "standing": said.map_or(Value::Null, |said| said["standing"].clone()),
                });
                if let Kind::Vm(_) = desktop.kind() {
                    shown["facts"] = said.map_or(Value::Null, |said| said["facts"].clone());
                }
                shown
            })
            .collect();
        json!({"ok": true, "viewed": viewed.as_str(), "desktops": desktops})
    }
}

impl Viewer {
    /// The person's start or stop of the VM `body` names by its ID, as `on`
    /// says. A start is the one a command makes when it needs the VM
    /// running; a stop is kept until Atlas VM confirms the VM stopped, and
    /// work running there ends.
    ///
    /// # Errors
    ///
    /// Refuses a request naming no desktop or no power, a desktop not
    /// served or not a VM, and what the start or stop refuses.
    pub(super) async fn power(self: &Arc<Self>, body: &Value) -> Result<Value> {
        let Some(named) = body["desktop"].as_str() else {
            return Err(Error::new(Code::Input, "Name the VM by its ID"));
        };
        let Some(on) = body["on"].as_bool() else {
            return Err(Error::new(Code::Input, "Power requires a boolean on"));
        };
        self.follow_settings().await;
        let desk = self.desk(named).ok_or_else(super::http::another_desktop)?;
        if !matches!(desk.client.desktop().kind(), Kind::Vm(_)) {
            return Err(Error::new(
                Code::Usage,
                format!(
                    "{} is not a VM; only a VM is started or stopped.",
                    desk.name
                ),
            ));
        }
        let powered = if on {
            atlas_client::vm::start::start(&desk.client).await
        } else {
            let client = desk.client.clone();
            tokio::task::spawn_blocking(move || atlas_client::vm::power_off(&client))
                .await
                .map_err(|_| Error::new(Code::Interrupted, "Stopping the VM stopped."))?
        };
        // The shared look says how it stood before.
        self.forget_look();
        powered
    }
}

#[cfg(test)]
#[path = "desks_tests.rs"]
mod tests;

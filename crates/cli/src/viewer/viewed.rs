//! Which desktop the person views in Atlas.app: one at a time, its control,
//! frames and sign-in the viewer's, while every desktop the settings list is
//! served. The choice is kept in the station's folder by the desktop's
//! record key, so a service started again views the same one; with none
//! kept, or one the settings no longer list, the default is viewed. Viewing
//! another lets go of control on the one viewed before, as following the
//! settings to another helper does.
use super::{Viewer, unpoisoned};
use atlas_core::{
    config::desktops::{Listed, Settings},
    desktop::RecordKey,
    error::{Code, Error, Result},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{path::Path, sync::Arc};

/// The most of the kept choice read.
const LONGEST: u64 = 4096;

/// The choice as it is kept.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Kept {
    desktop: RecordKey,
}

/// The desktop the person chose to view, as kept in the station's folder
/// in `home`; none when none is kept or it cannot be read.
pub(super) fn read(home: &Path) -> Option<RecordKey> {
    let path = atlas_client::station::Station::under(home).viewed().ok()?;
    let bytes = atlas_core::files::read_own(&path, LONGEST)?;
    serde_json::from_slice::<Kept>(&bytes)
        .ok()
        .map(|kept| kept.desktop)
}

/// Keep `desktop` as the one the person views, in the station's folder in
/// `home`, written whole and synced.
///
/// # Errors
///
/// Returns the failure to make the folder or write the file.
fn keep(home: &Path, desktop: &RecordKey) -> Result<()> {
    let path = atlas_client::station::Station::under(home).viewed()?;
    let bytes = serde_json::to_vec(&Kept {
        desktop: desktop.clone(),
    })?;
    atlas_core::files::write_durable(&path, &bytes, 0o600)
}

/// The desktop of `settings` to view, by the person's choice `chosen`: the
/// one it names while they list it, else their default.
pub(super) fn viewed_in<'a>(settings: &'a Settings, chosen: Option<&RecordKey>) -> &'a Listed {
    chosen
        .and_then(|key| {
            settings
                .desktops()
                .iter()
                .find(|listed| listed.record_key() == *key)
        })
        .unwrap_or_else(|| settings.default_desktop())
}

impl Viewer {
    /// The desktop the person chose to view, while one is chosen.
    pub(super) fn chosen(&self) -> Option<RecordKey> {
        unpoisoned(&self.viewed).clone()
    }

    /// Take up the choice kept in the station's folder, when this service
    /// keeps one.
    pub(super) fn adopt_viewed(&self) {
        let home = unpoisoned(&self.station_home).clone();
        if let Some(chosen) = home.as_deref().and_then(read) {
            *unpoisoned(&self.viewed) = Some(chosen);
        }
    }

    /// View the desktop `body` names by its ID: it is kept as the person's
    /// choice, control on the one viewed before is let go, and the service
    /// views it from then on. Viewing the one viewed already changes
    /// nothing.
    ///
    /// # Errors
    ///
    /// Refuses a request naming no desktop, or one the settings do not list,
    /// and one whose control or sign-in could not be let go of yet; fails
    /// when the choice cannot be kept.
    pub(super) async fn view(self: &Arc<Self>, body: &Value) -> Result<Value> {
        let Some(named) = body["desktop"].as_str() else {
            return Err(Error::new(
                Code::Input,
                "Name the desktop to view by its ID",
            ));
        };
        self.follow_settings().await;
        let desk = self.desk(named).ok_or_else(super::http::another_desktop)?;
        let key = desk.client.desktop().record_key();
        let home = unpoisoned(&self.station_home).clone();
        if let Some(home) = home {
            let kept = key.clone();
            tokio::task::spawn_blocking(move || keep(&home, &kept))
                .await
                .map_err(|_| Error::new(Code::Io, "The desktop viewed could not be kept"))??;
        }
        *unpoisoned(&self.viewed) = Some(key);
        self.follow_settings().await;

        if self.client().desktop().id().as_str() != named {
            return Err(Error::new(
                Code::Busy,
                format!(
                    "Atlas is still letting go of control of {}; {} is viewed once it has.",
                    self.client().selected.name,
                    desk.name
                ),
            ));
        }
        Ok(json!({"ok": true, "desktop": named}))
    }
}

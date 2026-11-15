//! What a machine desktop's daemon keeps of its session in the guest. Every
//! record trust depends on is in the guest account's Keychain, where only
//! Atlas's own code reads it: an agent runs as the same account, so any
//! file of the account's could be rewritten under it.
pub mod changes;
pub mod commitment;
pub mod generations;
pub mod input;
#[cfg(target_os = "macos")]
pub mod installer;
pub mod key;
pub mod launch;
pub mod mirror;
pub mod pairing;
pub mod processes;
mod record_file;
#[cfg(target_os = "macos")]
pub mod setup;
pub mod stage;
pub mod stop;
pub mod supervise;
pub mod update;
pub mod vault;

use crate::daemon::serve::machine::Paired;
use atlas_core::{
    desktop::DesktopId,
    error::{Code, Error, Result},
};
use atlas_session::{keys::own_key, pairing::GuestRecords};
use std::{path::PathBuf, sync::Arc};

/// The guest's identity to the station it paired with: opened only by the
/// guest's genuine daemon, as launchd started it, and only once paired.
/// `secret` is where the pairing secret is kept while there is one, and
/// `records` the private folder its changes and its Stop are kept in.
///
/// # Errors
///
/// Refuses any other process, and a guest that is not paired; fails when
/// a record cannot be read.
pub async fn open(desktop: DesktopId, secret: PathBuf, records: PathBuf) -> Result<Paired> {
    launch::prove().await?;
    let vault = vault::Keychain::atlas();
    tokio::task::spawn_blocking(move || {
        let keys = own_key(&key::GuestKey::new(vault.clone()))?;
        let pairing = pairing::Pairing::new(vault.clone(), secret);
        let (station, enrolment) = pairing.committed()?.ok_or_else(|| {
            Error::new(Code::Identity, "This desktop is not paired with a station.")
        })?;
        let vault: Arc<dyn vault::Vault> = Arc::new(vault);
        let changes = records.join("changes");
        atlas_core::files::private_folder(&changes, Code::State)?;
        Ok(Paired {
            keys,
            desktop,
            station,
            enrolment,
            pairing: Arc::new(pairing),
            generations: Arc::new(generations::Kept::new(vault.clone())),
            changes: Arc::new(changes::Changes::new(changes, vault.clone())),
            stop: Arc::new(stop::Stop::new(records.join("stop.json"), vault.clone())),
            updates: Arc::new(update::Updates::new(records.join("updates.json"), vault)),
        })
    })
    .await
    .map_err(|_| Error::new(Code::State, "The guest's records could not be read."))?
}

#[cfg(test)]
#[path = "machine_tests.rs"]
mod tests;

//! What a machine desktop's daemon keeps of its session in the guest. Every
//! record trust depends on is in the guest account's Keychain, where only
//! Atlas's own code reads it: an agent runs as the same account, so any
//! file of the account's could be rewritten under it.
pub mod changes;
pub mod commitment;
pub mod generations;
pub mod key;
pub mod launch;
pub mod pairing;
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
/// `records` the folder its changes are kept in.
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
        let (station, _) = pairing::Pairing::new(vault.clone(), secret)
            .committed()?
            .ok_or_else(|| {
                Error::new(Code::Identity, "This desktop is not paired with a station.")
            })?;
        let vault: Arc<dyn vault::Vault> = Arc::new(vault);
        Ok(Paired {
            keys,
            desktop,
            station,
            generations: Arc::new(generations::Kept::new(vault.clone())),
            changes: Arc::new(changes::Changes::new(records, vault)),
        })
    })
    .await
    .map_err(|_| Error::new(Code::State, "The guest's records could not be read."))?
}

#[cfg(test)]
#[path = "machine_tests.rs"]
mod tests;

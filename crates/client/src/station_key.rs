//! The station's own session key, kept in the account's Keychain, where
//! only Atlas's signature reads it without the Keychain's password. It is
//! made once, under a lock every station process takes to make it, and an
//! existing key is never replaced.
use atlas_core::{
    app_bundle::IDENTIFIER,
    error::{Code, Error, Result},
    files::Lock,
};
use atlas_native::keychain::{self, Item};
use atlas_session::keys::{KEY_LEN, KeptOnce, Keypair, SecretKey, kept_once};
use std::{path::Path, time::Duration};

/// Where the station's key is kept.
const ITEM: Item<'static> = Item {
    service: IDENTIFIER,
    account: "station session key",
};

/// How long a start waits for another making the key.
const PATIENCE: Duration = Duration::from_secs(10);

/// The station's key in the account's Keychain.
#[derive(Debug, Clone, Copy)]
pub struct StationKey {
    item: Item<'static>,
}

impl Default for StationKey {
    fn default() -> Self {
        Self { item: ITEM }
    }
}

impl StationKey {
    /// A key kept as `item`, for a test that must not touch the station's.
    #[cfg(test)]
    const fn at(item: Item<'static>) -> Self {
        Self { item }
    }

    /// The station's key: the one kept, or one made and kept when none is,
    /// under the lock at `lock`.
    ///
    /// # Errors
    ///
    /// Fails when the lock is held past its patience, the Keychain cannot be
    /// read or written, or the item kept is damaged.
    pub fn own(&self, lock: &Path) -> Result<Keypair> {
        made_once(self, lock)
    }
}

impl KeptOnce for StationKey {
    fn load(&self) -> Result<Option<SecretKey>> {
        keychain::read(self.item)?
            .map(|bytes| decoded(&bytes))
            .transpose()
    }

    fn add(&self, secret: &SecretKey) -> Result<bool> {
        keychain::add(self.item, secret.as_bytes())
    }
}

/// The key in `store`, made there under the lock at `lock` when none is:
/// two starts at once end with one key.
fn made_once(store: &impl KeptOnce, lock: &Path) -> Result<Keypair> {
    let file = atlas_core::files::open_lock(lock)?;
    atlas_core::files::lock_within(&file, Lock::Exclusive, PATIENCE).map_err(|_| {
        Error::new(
            Code::Busy,
            "Another Atlas is making the station's session key; try again.",
        )
    })?;
    kept_once(store)
}

/// The key `bytes` hold; anything else is a damaged item, never a reason
/// to make a new key.
fn decoded(bytes: &[u8]) -> Result<SecretKey> {
    let bytes = <[u8; KEY_LEN]>::try_from(bytes).map_err(|_| {
        Error::new(
            Code::Identity,
            "The station's session key in the Keychain is damaged.",
        )
    })?;
    Ok(SecretKey::from_bytes(bytes))
}

#[cfg(test)]
#[path = "station_key_tests.rs"]
mod tests;

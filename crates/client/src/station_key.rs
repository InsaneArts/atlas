//! The station's own session key, kept in the account's Keychain, where
//! only Atlas's signature reads it without the Keychain's password.
use atlas_core::{
    app_bundle::IDENTIFIER,
    error::{Code, Error, Result},
};
use atlas_native::keychain::{self, Item};
use atlas_session::keys::{KEY_LEN, KeyStore, SecretKey};

/// Where the station's key is kept.
const ITEM: Item<'static> = Item {
    service: IDENTIFIER,
    account: "station session key",
};

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
}

impl KeyStore for StationKey {
    fn load(&self) -> Result<Option<SecretKey>> {
        keychain::read(self.item)?
            .map(|bytes| decoded(&bytes))
            .transpose()
    }

    fn save(&self, secret: &SecretKey) -> Result<()> {
        keychain::write(self.item, secret.as_bytes())
    }
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

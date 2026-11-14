//! The guest's own session key.
use super::vault::{Vault, damaged};
use atlas_core::error::Result;
use atlas_session::keys::{KEY_LEN, KeyStore, SecretKey};

const NAME: &str = "guest session key";

/// The guest's key in `vault`.
#[derive(Debug)]
pub struct GuestKey<V> {
    vault: V,
}

impl<V: Vault> GuestKey<V> {
    pub const fn new(vault: V) -> Self {
        Self { vault }
    }
}

impl<V: Vault> KeyStore for GuestKey<V> {
    fn load(&self) -> Result<Option<SecretKey>> {
        let Some(bytes) = self.vault.read(NAME)? else {
            return Ok(None);
        };
        let bytes = <[u8; KEY_LEN]>::try_from(bytes.as_slice()).map_err(|_| damaged("key"))?;
        Ok(Some(SecretKey::from_bytes(bytes)))
    }

    fn save(&self, secret: &SecretKey) -> Result<()> {
        self.vault.write(NAME, secret.as_bytes())
    }
}

#[cfg(test)]
#[path = "key_tests.rs"]
mod tests;

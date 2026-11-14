//! Where the guest's records are kept: named secrets, each read and
//! replaced whole.
use atlas_core::{app_bundle::IDENTIFIER, error::Result};
use atlas_native::keychain::{self, Item};

/// Named records, each read and replaced whole, durably before a write
/// returns.
pub trait Vault: Send + Sync {
    /// The record `name`, if one is kept.
    ///
    /// # Errors
    ///
    /// Fails when it cannot be read; a missing record is `None`.
    fn read(&self, name: &str) -> Result<Option<Vec<u8>>>;

    /// Keep `bytes` as `name` from now on.
    ///
    /// # Errors
    ///
    /// Fails when it cannot be kept.
    fn write(&self, name: &str, bytes: &[u8]) -> Result<()>;
}

impl<V: Vault + ?Sized> Vault for &V {
    fn read(&self, name: &str) -> Result<Option<Vec<u8>>> {
        (**self).read(name)
    }

    fn write(&self, name: &str, bytes: &[u8]) -> Result<()> {
        (**self).write(name, bytes)
    }
}

impl<V: Vault + ?Sized> Vault for std::sync::Arc<V> {
    fn read(&self, name: &str) -> Result<Option<Vec<u8>>> {
        (**self).read(name)
    }

    fn write(&self, name: &str, bytes: &[u8]) -> Result<()> {
        (**self).write(name, bytes)
    }
}

/// The guest account's Keychain, under one service.
#[derive(Debug, Clone)]
pub struct Keychain {
    service: String,
}

impl Keychain {
    /// Atlas's own items.
    pub fn atlas() -> Self {
        Self {
            service: IDENTIFIER.to_owned(),
        }
    }

    /// Items under `service`, for a test that must not touch Atlas's.
    #[cfg(any(test, feature = "test-support"))]
    pub fn under(service: impl Into<String>) -> Self {
        Self {
            service: service.into(),
        }
    }

    fn item<'a>(&'a self, name: &'a str) -> Item<'a> {
        Item {
            service: &self.service,
            account: name,
        }
    }
}

impl Vault for Keychain {
    fn read(&self, name: &str) -> Result<Option<Vec<u8>>> {
        keychain::read(self.item(name))
    }

    fn write(&self, name: &str, bytes: &[u8]) -> Result<()> {
        keychain::write(self.item(name), bytes)
    }
}

/// Records kept in memory, for tests.
#[cfg(any(test, feature = "test-support"))]
#[derive(Debug, Default)]
pub struct Memory(std::sync::Mutex<std::collections::HashMap<String, Vec<u8>>>);

#[cfg(any(test, feature = "test-support"))]
impl Memory {
    fn records(&self) -> std::sync::MutexGuard<'_, std::collections::HashMap<String, Vec<u8>>> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Replace the record `name` behind the store's back, as damage would.
    pub fn damage(&self, name: &str, bytes: &[u8]) {
        self.records().insert(name.to_owned(), bytes.to_vec());
    }

    /// Lose the record `name`, as a rollback would.
    pub fn lose(&self, name: &str) {
        self.records().remove(name);
    }
}

#[cfg(any(test, feature = "test-support"))]
impl Vault for Memory {
    fn read(&self, name: &str) -> Result<Option<Vec<u8>>> {
        Ok(self.records().get(name).cloned())
    }

    fn write(&self, name: &str, bytes: &[u8]) -> Result<()> {
        self.records().insert(name.to_owned(), bytes.to_vec());
        Ok(())
    }
}

/// A record that is kept but cannot be what it says: damaged or from a
/// later version. Never a reason to start again.
pub(super) fn damaged(what: &str) -> atlas_core::error::Error {
    atlas_core::error::Error::new(
        atlas_core::error::Code::Identity,
        format!("The guest's {what} in the Keychain is damaged."),
    )
}

#[cfg(test)]
#[path = "vault_tests.rs"]
mod tests;

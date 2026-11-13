//! The shared Atlas command the helper runs, installed by the desktop's
//! kind, and the build installed there, hashed only when it changes.
use super::Client;
use atlas_core::{desktop::Kind, error::Result};
use serde_json::Value;

impl Client {
    /// Install or refresh the shared helper command from this executable.
    ///
    /// # Errors
    ///
    /// Returns controller, source-file, ownership or filesystem failures. Refuses a shared
    /// destination that another account can modify.
    pub fn install(&self) -> Result<Value> {
        self.install_probe(false)
    }

    /// Repair the probe under recovery exclusion without settling previous work.
    ///
    /// # Errors
    ///
    /// Returns recovery-exclusion, installed-app or filesystem failures. Repairing the
    /// executable never acknowledges earlier pending mutations.
    pub fn bootstrap_probe(&self) -> Result<Value> {
        self.install_probe(true)
    }

    fn install_probe(&self, recovery: bool) -> Result<Value> {
        match self.desktop().kind() {
            Kind::User(login) => crate::user::install::install_on_this_disk(self, login, recovery),
        }
    }

    /// SHA-256 of the installed shared executable, rehashed only when its
    /// device, inode, size or modification time changes.
    pub fn installed_build(&self) -> Option<String> {
        let key = match self.desktop().kind() {
            Kind::User(login) => crate::user::install::build_key_on_this_disk(self, login)?,
        };
        if let Some((cached, build)) = self.installed.lock().ok()?.as_ref()
            && *cached == key
        {
            return Some(build.clone());
        }

        // Hashed without the lock, so other callers are not held up. A
        // failed read is not cached: permissions may be repaired without
        // changing the identity key.
        let build = match self.desktop().kind() {
            Kind::User(login) => crate::user::install::build_on_this_disk(self, login)?,
        };

        if let Ok(mut cache) = self.installed.lock() {
            *cache = Some((key, build.clone()));
        }

        Some(build)
    }
}

#[cfg(test)]
#[path = "install_tests.rs"]
mod tests;

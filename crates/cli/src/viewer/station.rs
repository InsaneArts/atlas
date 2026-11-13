//! The one Atlas service of this account: its lock, held for as long as it
//! serves, and its record, which says where it is and which settings it
//! serves, for the command line and the app to find it. The record holds
//! no secret: what it names is reached only through the service's own
//! checks.
use atlas_core::{
    error::{Code, Error, Result},
    files::LockFile,
};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
};

/// Where the lock and the record are kept, in a home.
const FOLDER: &str = "Library/Application Support/Atlas";
const LOCK: &str = "station.lock";
const RECORD: &str = "station.json";

/// This service's hold on the account's one service, and its record, gone
/// with it.
pub(super) struct Station {
    _lock: LockFile,
    record: PathBuf,
    incarnation: String,
}

impl Station {
    /// Hold the account's one service for this one, serving the settings
    /// at `settings` on `port` as `incarnation`, and record it.
    ///
    /// # Errors
    ///
    /// Refuses while another service holds it, and returns the failure to
    /// make the folder, take the lock or write the record.
    pub(super) fn claim(settings: &Path, port: u16, incarnation: &str) -> Result<Self> {
        Self::claim_in(&atlas_core::config::home()?, settings, port, incarnation)
    }

    fn claim_in(home: &Path, settings: &Path, port: u16, incarnation: &str) -> Result<Self> {
        let folder = home.join(FOLDER);
        atlas_core::files::private_folder(&folder, Code::Menubar)?;
        let lock = atlas_core::files::open_lock(&folder.join(LOCK))?;
        match fs2::FileExt::try_lock_exclusive(&lock.0) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                return Err(Error::new(
                    Code::Menubar,
                    "Atlas already runs in the background",
                ));
            }
            Err(error) => return Err(error.into()),
        }
        let record = folder.join(RECORD);
        let written = json!({
            "settings": settings,
            "port": port,
            "pid": std::process::id(),
            "protocol": super::PROTOCOL,
            "incarnation": incarnation,
        });
        atlas_core::files::write_private(&record, &serde_json::to_vec_pretty(&written)?)?;
        Ok(Self {
            _lock: lock,
            record,
            incarnation: incarnation.to_owned(),
        })
    }
}

impl Drop for Station {
    /// The record goes with the service, unless a later one wrote its own.
    fn drop(&mut self) {
        let ours = fs::read(&self.record)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
            .is_some_and(|record| record["incarnation"] == self.incarnation.as_str());
        if ours {
            let _ = fs::remove_file(&self.record);
        }
    }
}

#[cfg(test)]
#[path = "station_tests.rs"]
mod tests;

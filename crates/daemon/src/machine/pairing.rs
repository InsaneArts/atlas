//! The guest's side of its pairing: which station it committed to, in the
//! Keychain, and the one-time secret it pairs with, in the folder the
//! desktop's host shares with it.
use super::{
    commitment, generations,
    vault::{Vault, damaged},
};
use atlas_core::{
    error::{Code, Error, Result},
    files::Token,
    session::key::PeerKey,
};
use atlas_session::{keys::PairingSecret, pairing::GuestRecords};
use serde::{Deserialize, Serialize};
use std::{
    io::ErrorKind,
    path::{Path, PathBuf},
    sync::{Mutex, PoisonError},
};

const NAME: &str = "guest pairing";

/// One commitment at a time in this process, whatever adapter makes it.
static COMMITTING: Mutex<()> = Mutex::new(());
const VERSION: u32 = 1;

/// The longest secret file read.
const SECRET: u64 = 256;

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Commitment {
    version: u32,
    station: PeerKey,
    change: Token,
}

/// The guest's pairing records in `vault`, and its secret at `secret`.
#[derive(Debug)]
pub struct Pairing<V> {
    vault: V,
    secret: PathBuf,
}

impl<V: Vault> Pairing<V> {
    pub fn new(vault: V, secret: impl Into<PathBuf>) -> Self {
        Self {
            vault,
            secret: secret.into(),
        }
    }

    fn commitment(&self) -> Result<Option<Commitment>> {
        let Some(bytes) = self.vault.read(NAME)? else {
            return Ok(None);
        };
        let kept: Commitment = serde_json::from_slice(&bytes).map_err(|_| damaged("pairing"))?;
        if kept.version != VERSION {
            return Err(damaged("pairing"));
        }
        Ok(Some(kept))
    }
}

impl<V: Vault> GuestRecords for Pairing<V> {
    fn committed(&self) -> Result<Option<(PeerKey, Token)>> {
        Ok(self.commitment()?.map(|kept| (kept.station, kept.change)))
    }

    fn commit(&self, station: PeerKey, change: &Token) -> Result<()> {
        let _one = COMMITTING.lock().unwrap_or_else(PoisonError::into_inner);
        let wanted = Commitment {
            version: VERSION,
            station,
            change: change.clone(),
        };
        match self.commitment()? {
            Some(kept) if kept == wanted => return Ok(()),
            Some(_) => {
                return Err(Error::new(
                    Code::Identity,
                    "This desktop is paired with another station.",
                ));
            }
            None => {}
        }
        // The count and the recovery stores first: a commitment never stands
        // without them.
        generations::begin(&self.vault)?;
        for store in commitment::STORES {
            commitment::create(&self.vault, store)?;
        }
        self.vault.write(NAME, &serde_json::to_vec(&wanted)?)?;
        if self.commitment()?.as_ref() != Some(&wanted) {
            return Err(damaged("pairing"));
        }
        Ok(())
    }

    fn secret(&self) -> Result<Option<PairingSecret>> {
        let Some(bytes) = atlas_core::files::read_own(&self.secret, SECRET) else {
            return match std::fs::symlink_metadata(&self.secret) {
                Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
                _ => Err(Error::new(
                    Code::Identity,
                    "The pairing secret cannot be read.",
                )),
            };
        };
        let text = std::str::from_utf8(&bytes)
            .map(str::trim)
            .unwrap_or_default();
        PeerKey::from_hex(text)
            .map(|bytes| Some(PairingSecret::from_bytes(*bytes.as_bytes())))
            .ok_or_else(|| Error::new(Code::Identity, "The pairing secret is damaged."))
    }

    fn forget_secret(&self) -> Result<()> {
        match std::fs::remove_file(&self.secret) {
            // Already gone: its removal may not have been kept, so it is
            // kept again.
            Ok(()) => {}
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(error) => {
                return Err(
                    Error::new(Code::Io, "The pairing secret cannot be removed.")
                        .with_source(error),
                );
            }
        }
        // Gone for good only once its folder says so.
        let folder = self.secret.parent().unwrap_or(Path::new("."));
        sync(folder).map_err(|error| {
            Error::new(Code::Io, "The pairing secret's removal cannot be kept.").with_source(error)
        })
    }
}

/// Make what changed in `folder` outlive a crash.
#[cfg(not(test))]
fn sync(folder: &Path) -> std::io::Result<()> {
    std::fs::File::open(folder)?.sync_all()
}

/// [`sync`], failing while a test says so.
#[cfg(test)]
fn sync(folder: &Path) -> std::io::Result<()> {
    if tests::SYNC_FAILS.with(std::cell::Cell::get) {
        return Err(std::io::ErrorKind::Other.into());
    }
    std::fs::File::open(folder)?.sync_all()
}

#[cfg(test)]
#[path = "pairing_tests.rs"]
mod tests;

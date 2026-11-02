//! Only held input is journaled, never text or screenshots. A new service can
//! release an interrupted press before accepting another controller.
use atlas_core::{
    actions::Button,
    context::context,
    error::{Code, Error, Result},
    files::{Durability, write_private_with},
    session::InputEpoch,
};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
};

#[derive(Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Held {
    pub(super) epoch: InputEpoch,
    pub(super) button: Option<Button>,
    pub(super) keys: Vec<u16>,
    pub(super) x: f64,
    pub(super) y: f64,
    pub(super) count: u8,
    pub(super) mouse_number: i64,
    pub(super) flags: u64,
}

/// The last successful replacement is cached only to avoid writing an unchanged
/// record twice. A failed replacement preserves the last successful record.
pub(super) struct Journal {
    path: PathBuf,
    saved: Option<Held>,
}

impl Journal {
    pub(super) fn new(path: PathBuf) -> Self {
        Self { path, saved: None }
    }

    pub(super) fn write(&mut self, held: Held) -> Result<()> {
        if self.saved.as_ref() == Some(&held) {
            return Ok(());
        }

        store(&self.path, &held)?;
        self.saved = Some(held);

        Ok(())
    }
}

/// The input a crashed service left held in this login, if any. A journal
/// from an earlier login is set aside, never returned.
pub(super) fn load(path: &Path, epoch: &InputEpoch) -> Result<Option<Held>> {
    let Some(bytes) = read(path, epoch.uid)? else {
        return Ok(None);
    };

    let held: Held =
        serde_json::from_slice(&bytes).map_err(context("Decode held-input journal", path))?;
    if held.epoch.uid != epoch.uid
        || !held.x.is_finite()
        || !held.y.is_finite()
        || held.keys.len() > 16
        || held.keys.iter().any(|key| *key > 127)
        || !(1..=5).contains(&held.count)
        || held.mouse_number < 0
    {
        return Err(Error::new(
            Code::Identity,
            "Held-input journal belongs to a different login or is invalid",
        ));
    }

    if held.epoch.security_session_id == epoch.security_session_id {
        return Ok(Some(held));
    }

    if held.button.is_some() || !held.keys.is_empty() || held.flags != 0 {
        // That login ended (a restart or a forced log-out) and its held
        // input with it; nothing is replayed into this one. The journal is
        // kept beside this one as evidence, and this service starts clean.
        let kept = set_aside(path, &held)?;
        eprintln!(
            "Held input from an earlier login (session {}) was not replayed; its journal is \
             kept at {}",
            held.epoch.security_session_id,
            kept.display()
        );
    }

    Ok(None)
}

/// The journal's bytes, when there is one owned by account `uid`.
fn read(path: &Path, uid: u32) -> Result<Option<Vec<u8>>> {
    let foreign = || {
        Error::new(
            Code::Identity,
            "Held-input journal must belong to the helper",
        )
    };

    // A symlink is never followed (O_NOFOLLOW fails with ELOOP), and the
    // checks are made on the opened descriptor, so the file read is the file
    // checked. Opening never blocks on a FIFO.
    let file = match fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
    {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) if error.raw_os_error() == Some(libc::ELOOP) => return Err(foreign()),
        Err(error) => return Err(context("Open held-input journal", path)(error)),
    };

    let metadata = file
        .metadata()
        .map_err(context("Inspect held-input journal", path))?;
    if !metadata.is_file() || metadata.uid() != uid {
        return Err(foreign());
    }

    let bytes = atlas_core::files::read_up_to(file, 4097)
        .map_err(context("Read held-input journal", path))?;

    if bytes.len() > 4096 {
        return Err(Error::new(Code::Native, "Held-input journal is invalid"));
    }

    Ok(Some(bytes))
}

/// Move an earlier login's journal aside, never over an earlier one, and
/// say where it went.
fn set_aside(path: &Path, held: &Held) -> Result<PathBuf> {
    let refused = |why: String| {
        Error::new(
            Code::Identity,
            format!(
                "Held input from an earlier login could not be set aside{why}; it will not be \
                 replayed into this login"
            ),
        )
    };
    let session = held.epoch.security_session_id;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let kept = (0..100u32)
        .map(|attempt| path.with_extension(format!("{session}.{stamp}-{attempt}.json")))
        .find(|kept| fs::symlink_metadata(kept).is_err())
        .ok_or_else(|| refused(String::new()))?;

    fs::rename(path, &kept).map_err(|error| refused(format!(" ({error})")))?;

    Ok(kept)
}

/// Replace the journal in one step. It is written as far as the kernel: it
/// is for a service that crashed. A power cut ends the login whose input
/// was held, and a journal from an earlier login is set aside, not
/// replayed.
fn store(path: &Path, held: &Held) -> Result<()> {
    let temporary = path.with_extension("pending");
    let bytes = serde_json::to_vec(held).map_err(context("Encode held-input journal", path))?;
    write_private_with(&temporary, &bytes, Durability::Kernel)
        .map_err(context("Write held-input journal", path))?;
    fs::rename(temporary, path).map_err(context("Replace held-input journal", path))?;
    Ok(())
}

#[cfg(test)]
mod tests;

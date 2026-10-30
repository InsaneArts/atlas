//! Only held input is journaled, never text or screenshots. A new service can
//! release an interrupted press before accepting another controller.
use crate::{
    actions::Button,
    error::{Code, Error, Result},
    files::{Durability, write_private_with},
    session::InputEpoch,
};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Read,
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
};

#[derive(Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Held {
    pub epoch: InputEpoch,
    pub button: Option<Button>,
    pub keys: Vec<u16>,
    pub x: f64,
    pub y: f64,
    pub count: u8,
    #[serde(default)]
    pub mouse_number: i64,
    pub flags: u64,
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
    let foreign = || {
        Error::new(
            Code::Identity,
            "Held-input journal must belong to the helper",
        )
    };

    // A symlink is never followed (O_NOFOLLOW fails with ELOOP), and the
    // checks are made on the opened descriptor, so the file read is the file
    // checked. Opening never blocks on a FIFO.
    let mut file = match fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
    {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) if error.raw_os_error() == Some(libc::ELOOP) => return Err(foreign()),
        Err(error) => return Err(error.into()),
    };

    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.uid() != epoch.uid {
        return Err(foreign());
    }

    let mut bytes = Vec::new();
    (&mut file).take(4097).read_to_end(&mut bytes)?;

    if bytes.len() > 4096 {
        return Err(Error::new(Code::Native, "Held-input journal is invalid"));
    }

    let held: Held = serde_json::from_slice(&bytes)?;
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

    if held.epoch.security_session_id != epoch.security_session_id {
        if held.button.is_none() && held.keys.is_empty() && held.flags == 0 {
            return Ok(None);
        }

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

        return Ok(None);
    }

    Ok(Some(held))
}

/// Move an earlier login's journal aside, never over an earlier one, and
/// say where it went.
fn set_aside(path: &Path, held: &Held) -> Result<PathBuf> {
    let session = held.epoch.security_session_id;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let Some(kept) = (0..100u32)
        .map(|attempt| path.with_extension(format!("{session}.{stamp}-{attempt}.json")))
        .find(|kept| fs::symlink_metadata(kept).is_err())
    else {
        return Err(Error::new(
            Code::Identity,
            "Held input from an earlier login could not be set aside; it will not be replayed \
             into this login",
        ));
    };

    fs::rename(path, &kept).map_err(|error| {
        Error::new(
            Code::Identity,
            format!(
                "Held input from an earlier login could not be set aside ({error}); it \
                 will not be replayed into this login"
            ),
        )
    })?;

    Ok(kept)
}

/// Replace the journal in one step. It is written as far as the kernel: it
/// is for a service that crashed. A power cut ends the login whose input
/// was held, and a journal from an earlier login is set aside, not
/// replayed.
pub(super) fn store(path: &Path, held: &Held) -> Result<()> {
    let temporary = path.with_extension("pending");
    write_private_with(&temporary, &serde_json::to_vec(held)?, Durability::Kernel)?;
    fs::rename(temporary, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_epoch() -> InputEpoch {
        InputEpoch {
            uid: nix::unistd::getuid().as_raw(),
            security_session_id: 10,
            display_id: 1,
            width: 1600.0,
            height: 900.0,
            process_id: 1,
        }
    }

    #[test]
    fn legacy_held_journal_recovers_with_zero_mouse_number() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("held.json");
        let mut epoch = test_epoch();
        // This is the previous on-disk schema, not a newly serialized Held.
        let legacy = serde_json::json!({
            "epoch": epoch,
            "button": "left",
            "keys": [55],
            "x": 10.5,
            "y": 20.5,
            "count": 2,
            "flags": 1 << 20,
        });

        write_private_with(
            &path,
            &serde_json::to_vec(&legacy).unwrap(),
            Durability::Disk,
        )
        .unwrap();

        epoch.process_id = 2;
        let recovered = load(&path, &epoch).unwrap().unwrap();

        assert_eq!(recovered.mouse_number, 0);
        assert_eq!(recovered.button, Some(Button::Left));
        assert_eq!(recovered.keys, vec![55]);
        assert_eq!(recovered.flags, 1 << 20);
        assert_eq!((recovered.x, recovered.y, recovered.count), (10.5, 20.5, 2));
    }

    #[test]
    fn nonzero_press_number_survives_restart_and_cleanup_commit() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("held.json");
        let mut epoch = test_epoch();
        let held = Held {
            epoch: epoch.clone(),
            button: Some(Button::Right),
            keys: vec![55],
            x: 10.5,
            y: 20.5,
            count: 2,
            mouse_number: 37,
            flags: 1 << 20,
        };
        Journal::new(path.clone()).write(held.clone()).unwrap();

        // A replacement input process must retain the interrupted press's
        // number so its release can match the already-posted mouse-down.
        epoch.process_id = 2;
        let mut recovered = load(&path, &epoch).unwrap().unwrap();
        assert!(recovered == held);
        assert_eq!(recovered.mouse_number, 37);

        // This validates the recovery record, not desktop event delivery.
        recovered.epoch = epoch.clone();
        recovered.button = None;
        recovered.keys.clear();
        recovered.flags = 0;

        Journal::new(path.clone()).write(recovered.clone()).unwrap();
        let clean = load(&path, &epoch).unwrap().unwrap();
        assert!(clean == recovered);
        assert_eq!(clean.mouse_number, 37);
    }

    #[test]
    fn negative_press_number_is_not_replayed() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("held.json");
        let epoch = test_epoch();
        let mut held = Held {
            epoch: epoch.clone(),
            button: Some(Button::Left),
            keys: Vec::new(),
            x: 10.0,
            y: 10.0,
            count: 1,
            mouse_number: 1,
            flags: 0,
        };
        store(&path, &held).unwrap();
        assert!(load(&path, &epoch).unwrap().is_some());

        held.mouse_number = -1;
        store(&path, &held).unwrap();

        let Err(error) = load(&path, &epoch) else {
            panic!("A negative mouse event number was accepted for recovery");
        };

        assert_eq!(error.code, "identity");
    }

    #[test]
    fn clean_journals_survive_login_changes_but_held_input_does_not_cross() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("held.json");
        let mut epoch = test_epoch();
        let mut held = Held {
            epoch: epoch.clone(),
            button: None,
            keys: Vec::new(),
            x: 10.0,
            y: 10.0,
            count: 1,
            mouse_number: 0,
            flags: 0,
        };
        store(&path, &held).unwrap();

        epoch.security_session_id = 20;
        assert!(load(&path, &epoch).unwrap().is_none());

        held.button = Some(Button::Left);
        store(&path, &held).unwrap();
        assert!(
            load(&path, &epoch).unwrap().is_none(),
            "nothing is replayed"
        );
        assert!(!path.exists(), "the next login starts clean");

        let kept: Vec<_> = fs::read_dir(directory.path())
            .unwrap()
            .flatten()
            .map(|entry| entry.file_name().into_string().unwrap())
            .filter(|name| name.starts_with("held.10.") && name.ends_with(".json"))
            .collect();
        assert_eq!(kept.len(), 1, "the earlier login's journal is kept");

        let evidence: Held =
            serde_json::from_slice(&fs::read(directory.path().join(&kept[0])).unwrap()).unwrap();
        assert!(evidence == held);

        store(&path, &held).unwrap();
        assert!(load(&path, &epoch).unwrap().is_none());
        assert!(!path.exists(), "a second one never replaces the first");
    }

    /// Only the journal file itself is read: never a link to one, even a
    /// valid one, nor anything that is not a regular file.
    #[test]
    fn a_linked_or_irregular_journal_is_refused() {
        let directory = tempfile::tempdir().unwrap();
        let epoch = test_epoch();
        let real = directory.path().join("real.json");
        store(
            &real,
            &Held {
                epoch: epoch.clone(),
                button: None,
                keys: Vec::new(),
                x: 1.0,
                y: 1.0,
                count: 1,
                mouse_number: 0,
                flags: 0,
            },
        )
        .unwrap();

        assert!(load(&real, &epoch).unwrap().is_some());

        let link = directory.path().join("held.json");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        assert_eq!(load(&link, &epoch).err().unwrap().code, "identity");
        assert_eq!(
            load(directory.path(), &epoch).err().unwrap().code,
            "identity"
        );

        let fifo = directory.path().join("fifo.json");
        nix::unistd::mkfifo(&fifo, nix::sys::stat::Mode::S_IRWXU).unwrap();
        assert_eq!(load(&fifo, &epoch).err().unwrap().code, "identity");
        assert!(
            load(&directory.path().join("absent.json"), &epoch)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn a_failed_pointer_move_write_still_allows_the_release_to_be_posted() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("held.json");
        let epoch = test_epoch();
        let held = Held {
            epoch: epoch.clone(),
            button: Some(Button::Left),
            keys: Vec::new(),
            x: 10.0,
            y: 10.0,
            count: 1,
            mouse_number: 1,
            flags: 0,
        };
        let mut journal = Journal::new(path.clone());
        let mut posted = Vec::new();
        {
            // Posting is recorded only after the conservative journal write succeeds.
            let mut post = |record, phase| -> Result<()> {
                journal.write(record)?;
                posted.push(phase);
                Ok(())
            };
            post(held.clone(), "mouse-down").unwrap();

            let pending = path.with_extension("pending");
            fs::create_dir(&pending).unwrap();
            let mut moved = held.clone();
            moved.x += 1.0;
            assert!(post(moved, "mouse-dragged").is_err());

            post(held.clone(), "mouse-up").unwrap();
        }
        assert_eq!(posted, ["mouse-down", "mouse-up"]);
        assert!(load(&path, &epoch).unwrap().unwrap() == held);

        let mut clean = held;
        clean.button = None;
        assert!(journal.write(clean.clone()).is_err());
        assert!(journal.write(clean.clone()).is_err());
        assert_eq!(
            load(&path, &epoch).unwrap().unwrap().button,
            Some(Button::Left)
        );

        fs::remove_dir(path.with_extension("pending")).unwrap();
        journal.write(clean.clone()).unwrap();
        assert!(load(&path, &epoch).unwrap().unwrap() == clean);
    }

    #[test]
    fn failed_clean_commit_is_retried_without_losing_the_held_record() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("held.json");
        let epoch = test_epoch();
        let mut held = Held {
            epoch: epoch.clone(),
            button: None,
            keys: vec![0],
            x: 10.0,
            y: 10.0,
            count: 1,
            mouse_number: 0,
            flags: 1 << 20,
        };
        let mut journal = Journal::new(path.clone());
        journal.write(held.clone()).unwrap();

        held.keys.clear();
        held.flags = 0;

        // A failed replacement must preserve the old recoverable record and
        // must not convince the cache that cleanup was durably committed.
        let pending = path.with_extension("pending");
        fs::create_dir(&pending).unwrap();
        assert!(journal.write(held.clone()).is_err());
        let interrupted = load(&path, &epoch).unwrap().unwrap();
        assert_eq!(interrupted.keys, vec![0]);
        assert_eq!(interrupted.flags, 1 << 20);

        fs::remove_dir(pending).unwrap();
        journal.write(held).unwrap();
        let clean = load(&path, &epoch).unwrap().unwrap();
        assert!(clean.keys.is_empty());
        assert_eq!(clean.flags, 0);
    }
}

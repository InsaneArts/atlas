//! The person's Stop as their side keeps it: a marker that is there while
//! agents are stopped and holds the revision of the Stop that made it, each
//! Stop writing one never written before. A Resume reads the revision it means
//! to lift and, once the desktop has answered, lifts only that one: a Stop
//! that came meanwhile stays, whether or not its caller lived to tell the
//! desktop. An empty marker, as older builds write, is a Stop all the same.
//!
//! A Stop is never refused and never lost. When another Stop or Resume holds
//! the marker's lock past the patience, the Stop does not overwrite the
//! marker without the lock; it drops a witness in a folder beside it that a
//! Resume cannot clear unless it saw it. The marker and the witnesses it saw
//! are what a Resume lifts, together and only when both still stand.
use super::Controller;
use atlas_core::{
    context::context,
    error::{Error, Result},
    files::{Lock, LockFile, lock_within, open_lock, token},
};
use std::{
    collections::BTreeSet,
    fs,
    io::ErrorKind,
    os::unix::fs::{DirBuilderExt, OpenOptionsExt},
    time::Duration,
};

/// How long a Stop or Resume waits for another to finish with the marker. A
/// Stop that waited this long keeps a witness instead: it is never refused.
const PATIENCE: Duration = Duration::from_secs(2);

/// What a Resume read of the Stop noted now: the marker, and the witnesses
/// of Stops that could not take the lock. A Resume lifts exactly this.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StopRevision {
    /// The marker's contents, when it is there.
    marker: Option<String>,
    /// The witnesses present, by name.
    pending: BTreeSet<String>,
}

impl StopRevision {
    fn is_stopped(&self) -> bool {
        self.marker.is_some() || !self.pending.is_empty()
    }
}

/// What lifting a Stop came to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Resumed {
    /// The Stop the Resume read was lifted; agents may run.
    Cleared,
    /// Nothing was noted to lift; agents may run.
    AlreadyClear,
    /// A Stop given since the Resume read still holds agents; nothing was
    /// lifted.
    StopHeld,
}

impl Controller {
    /// Whether the person stopped agents, as noted on their side: the
    /// marker, or a witness of a Stop that could not take the lock.
    pub fn stop_noted(&self) -> bool {
        self.stopped.exists() || self.pending_names().next().is_some()
    }

    /// Note that the person stopped agents (`on`), with a revision of its
    /// own, or lift whatever Stop is noted.
    ///
    /// # Errors
    ///
    /// Returns failures reading or updating this controller identity's private stop marker.
    pub fn note_stop(&self, on: bool) -> Result<()> {
        self.refuse_linked_directory()?;
        if !on {
            // With no folder yet there is no Stop to lift, and none is made.
            if fs::symlink_metadata(&self.directory).is_err() {
                return Ok(());
            }
            let _held = self.lock_stop()?;
            return self.clear_all();
        }
        self.make_directory()?;
        // A Stop that cannot take the lock keeps a witness, never losing it
        // and never overwriting the marker a Resume may be clearing.
        match self.lock_stop() {
            Ok(_held) => {
                // The witnesses this Stop is folding in, captured before its
                // own marker goes down: its marker is published first and the
                // captured witnesses removed only after, so a Stop that dies
                // between leaves them, never an unstopped moment.
                let folded: Vec<_> = self.pending_names().collect();
                fs::write(&self.stopped, token())
                    .map_err(|error| Error::writing(&self.directory, error))?;
                for name in folded {
                    let _ = fs::remove_file(self.stop_pending.join(name));
                }
                Ok(())
            }
            Err(_busy) => self.keep_witness(),
        }
    }

    /// The Stop noted now, which a Resume lifts once the desktop has
    /// answered; none while agents are not stopped here.
    ///
    /// # Errors
    ///
    /// Returns failures reading the marker: a Stop that cannot be read is
    /// never taken as none.
    pub fn stop_revision(&self) -> Result<Option<StopRevision>> {
        self.refuse_linked_directory()?;
        if fs::symlink_metadata(&self.directory).is_err() {
            return Ok(None);
        }
        let _held = self.lock_stop()?;
        let revision = self.read_stop()?;
        Ok(revision.is_stopped().then_some(revision))
    }

    /// Lift the Stop `revision` the Resume read before it asked the desktop,
    /// when the marker and every witness it saw still stand and no other has
    /// come: a Stop given since holds agents and is never lifted.
    ///
    /// # Errors
    ///
    /// Returns failures reading or removing the marker.
    pub fn lift_stop(&self, revision: Option<&StopRevision>) -> Result<Resumed> {
        self.refuse_linked_directory()?;
        if fs::symlink_metadata(&self.directory).is_err() {
            return Ok(Resumed::AlreadyClear);
        }
        let _held = self.lock_stop()?;
        let now = self.read_stop()?;
        let empty = StopRevision {
            marker: None,
            pending: BTreeSet::new(),
        };
        let observed = revision.unwrap_or(&empty);
        if &now != observed {
            // Something changed: a newer Stop holds agents, or another
            // Resume already cleared what this one read.
            return Ok(if now.is_stopped() {
                Resumed::StopHeld
            } else {
                Resumed::AlreadyClear
            });
        }
        if !now.is_stopped() {
            return Ok(Resumed::AlreadyClear);
        }
        self.remove_stop()?;
        for name in &observed.pending {
            let _ = fs::remove_file(self.stop_pending.join(name));
        }
        Ok(Resumed::Cleared)
    }

    /// The marker's lock, beside it, with bounded patience.
    fn lock_stop(&self) -> Result<LockFile> {
        self.make_directory()?;
        let file = open_lock(&self.stop_lock)?;
        lock_within(&file, Lock::Exclusive, PATIENCE).map_err(
            |atlas_core::files::Held(cause)| {
                Error::new(
                    atlas_core::error::Code::Busy,
                    "Another Stop or Resume is still being kept; try again",
                )
                .with_source(cause)
            },
        )?;
        Ok(file)
    }

    /// The Stop noted, read while the caller holds the marker's lock: the
    /// marker and every witness beside it.
    fn read_stop(&self) -> Result<StopRevision> {
        let marker = match fs::read_to_string(&self.stopped) {
            Ok(revision) => Some(revision),
            Err(error) if error.kind() == ErrorKind::NotFound => None,
            Err(error) => return Err(context("Read the Stop marker", &self.stopped)(error)),
        };
        Ok(StopRevision {
            marker,
            pending: self.pending_names().collect(),
        })
    }

    /// Drop a witness of a Stop that could not take the lock, named for
    /// itself, which no Resume clears unless it saw it.
    fn keep_witness(&self) -> Result<()> {
        fs::DirBuilder::new()
            .mode(0o700)
            .recursive(true)
            .create(&self.stop_pending)
            .map_err(|error| Error::writing(&self.stop_pending, error))?;
        let witness = self.stop_pending.join(token());
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&witness)
            .map(|_| ())
            .map_err(|error| Error::writing(&self.stop_pending, error))
    }

    /// The witnesses beside the marker, by name.
    fn pending_names(&self) -> impl Iterator<Item = String> + '_ {
        fs::read_dir(&self.stop_pending)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|entry| entry.file_name().into_string().ok())
    }

    fn remove_stop(&self) -> Result<()> {
        match fs::remove_file(&self.stopped) {
            Err(error) if error.kind() != ErrorKind::NotFound => {
                Err(Error::writing(&self.directory, error))
            }
            _ => Ok(()),
        }
    }

    /// Fold every witness into the marker: a Stop with the lock takes them
    /// all, so they never pile up.
    fn drain_pending(&self) {
        for name in self.pending_names().collect::<Vec<_>>() {
            let _ = fs::remove_file(self.stop_pending.join(name));
        }
    }

    /// Clear the marker and every witness, under the lock.
    fn clear_all(&self) -> Result<()> {
        self.drain_pending();
        self.remove_stop()
    }
}

#[cfg(test)]
#[path = "stop_tests.rs"]
mod tests;

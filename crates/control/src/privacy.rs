//! The private hold: while the person signs the helper in to an account,
//! the helper's screen is hidden from agents. A marker beside the person's
//! note names the lease it was set under (its holder's process and the
//! lease's generation) and is honoured only while that process lives and
//! the note still names that generation, so a viewer killed mid-sign-in, or
//! a later lease, leaves nothing hidden. A generation counter beside it goes
//! up each time a hold begins or ends and is never removed: a read of the
//! screen admitted before either is never kept.
use super::{Controller, busy::busy_private};
use atlas_core::{
    error::{Code, Error, Result},
    files::{read_own, write_atomic},
    process::Instance,
    time::now_ms,
};
use serde::{Deserialize, Serialize};
use std::{fs, future::Future, io::ErrorKind, path::Path};

/// The most the marker, the person's note or the generation may hold
/// before it is treated as unreadable.
const LIMIT: u64 = 4096;

/// What a private hold is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HoldKind {
    /// Signing the helper in to a site.
    SignIn,
    /// Deleting a site's data in the browser's settings.
    Forget,
    /// Clearing the browser's data in its settings.
    Reset,
}

/// A private hold: the site it is for, and why.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrivateHold {
    pub site: String,
    pub kind: HoldKind,
}

/// The marker as it is kept: the hold, and the lease it was set under.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Marker {
    #[serde(flatten)]
    hold: PrivateHold,
    /// The process holding the lease.
    holder: Instance,
    /// The lease's generation, as the person's note holds it.
    lease: String,
    /// When it began, in milliseconds since 1970.
    since: u64,
}

/// The screen's privacy generation when a read of it was admitted: the read
/// is kept only while it is unchanged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Visible(u64);

/// What the marker says now.
enum Found {
    Nothing,
    Live(PrivateHold),
    /// A marker that can't be read while the person has control.
    Unreadable,
}

impl Controller {
    /// The private hold in force now, when there is one: its marker names a
    /// process that still runs and the lease generation the person's note
    /// still holds. A marker that can't be read names no hold here, but
    /// still hides the screen while the person has control.
    pub fn private_hold(&self) -> Option<PrivateHold> {
        match self.found() {
            Found::Live(hold) => Some(hold),
            Found::Nothing | Found::Unreadable => None,
        }
    }

    /// Whether the screen may be read now, and its generation then: what
    /// [`Controller::still_visible`] checks the read against before it is
    /// kept or shown.
    ///
    /// # Errors
    ///
    /// Returns `busy`, held by the person, while a private hold is in force,
    /// and `state` when the generation can't be read.
    pub fn screen_visible(&self) -> Result<Visible> {
        match self.found() {
            Found::Live(_) | Found::Unreadable => Err(busy_private()),
            Found::Nothing => self.generation().map(Visible),
        }
    }

    /// Whether a read of the screen admitted at `admitted` may be kept or
    /// shown: no hold is in force and none began or ended since.
    ///
    /// # Errors
    ///
    /// As [`Controller::screen_visible`], and `busy`, held by the person,
    /// when the generation changed.
    pub fn still_visible(&self, admitted: Visible) -> Result<()> {
        if self.screen_visible()? == admitted {
            Ok(())
        } else {
            Err(busy_private())
        }
    }

    /// `read` of the screen, refused while a private hold is in force, and
    /// its result dropped when a hold began or ended while it ran.
    ///
    /// # Errors
    ///
    /// As [`Controller::still_visible`], and whatever `read` returns.
    pub async fn shown<T>(&self, read: impl Future<Output = Result<T>>) -> Result<T> {
        let admitted = self.screen_visible()?;
        let value = read.await?;
        self.still_visible(admitted)?;
        Ok(value)
    }

    /// Begin `hold` under the person's lease of `generation`: the
    /// generation goes up first, so every read admitted before it is
    /// dropped, then the marker names this process and the lease.
    pub(crate) fn begin_private(&self, lease: &str, hold: &PrivateHold) -> Result<()> {
        let holder = Instance::current().ok_or_else(|| {
            Error::new(Code::State, "macOS did not describe this process to Atlas")
        })?;
        self.bump_generation()?;
        let marker = Marker {
            hold: hold.clone(),
            holder,
            lease: lease.to_owned(),
            since: now_ms(),
        };
        // Whole or not at all: a holder killed while writing leaves no torn
        // marker for a later lease to meet.
        write_atomic(&self.private, &serde_json::to_vec(&marker)?, 0o600)
    }

    /// Clear a marker no lease can hold any longer, under the controller's
    /// exclusion and before the person's next lease is noted: one left by
    /// a holder that ended or was killed, readable or not. The generation
    /// goes up when one was there, so a read admitted under it is dropped.
    pub(crate) fn clear_stale_private(&self) -> Result<()> {
        match fs::symlink_metadata(&self.private) {
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(Error::writing(&self.private, error)),
            Ok(_) => {}
        }
        self.end_private()
    }

    /// End the private hold: the marker goes, then the generation goes up,
    /// so a read admitted during the hold is never kept.
    ///
    /// # Errors
    ///
    /// Returns the failure to remove the marker or to write the generation.
    /// A marker left behind is honoured no longer than the lease it names.
    pub fn end_private(&self) -> Result<()> {
        match fs::remove_file(&self.private) {
            Err(error) if error.kind() != ErrorKind::NotFound => {
                return Err(Error::writing(&self.private, error));
            }
            _ => {}
        }
        self.bump_generation()
    }

    /// The marker, as far as it is in force.
    fn found(&self) -> Found {
        if fs::symlink_metadata(&self.private).is_err() {
            return Found::Nothing;
        }
        let Some(person) = read_own(&self.person, LIMIT) else {
            // No person holds control: nothing they began is in force.
            return Found::Nothing;
        };
        let marker = read_own(&self.private, LIMIT)
            .and_then(|bytes| serde_json::from_slice::<Marker>(&bytes).ok());
        match marker {
            Some(marker) if marker.lease.as_bytes() == person && marker.holder.running() => {
                Found::Live(marker.hold)
            }
            Some(_) => Found::Nothing,
            // Unreadable while the person has control: hidden until it ends.
            None => Found::Unreadable,
        }
    }

    /// The generation now: none written yet is 0.
    fn generation(&self) -> Result<u64> {
        read_generation(&self.private_generation)?.ok_or_else(|| {
            Error::new(
                Code::State,
                "Atlas couldn't read whether the helper's screen is private. Try again.",
            )
        })
    }

    /// Count one more hold begun or ended. One that can't be read starts
    /// again: no read was admitted under it.
    fn bump_generation(&self) -> Result<()> {
        self.make_directory()?;
        let next = read_generation(&self.private_generation)
            .ok()
            .flatten()
            .unwrap_or(0)
            .wrapping_add(1);
        write_atomic(&self.private_generation, next.to_string().as_bytes(), 0o600)
    }
}

/// The generation kept at `path`: 0 when there is none, None when it can't
/// be read.
fn read_generation(path: &Path) -> Result<Option<u64>> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Some(0)),
        Err(error) => return Err(Error::writing(path, error)),
        Ok(_) => {}
    }
    Ok(read_own(path, LIMIT)
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .and_then(|text| text.trim().parse().ok()))
}

#[cfg(test)]
mod tests;

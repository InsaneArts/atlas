//! The private hold: while the person signs the helper in to an account,
//! the helper's screen is hidden from agents. A marker beside the person's
//! note names the lease it was set under (its holder's process and the
//! lease's generation). While that process lives, the hold is honoured as
//! long as the note still names that generation, so a later lease of the
//! same viewer leaves nothing hidden. When its holder died, or the marker
//! cannot be read, the hold was *interrupted*: the screen stays hidden from
//! agents and their changes are refused until the person finishes or
//! cancels it in Atlas, and taking control again never clears it. A
//! generation counter beside it goes up each time a hold begins or ends and
//! is never removed: a read of the screen admitted before either is never
//! kept.
use super::{
    Controller, Ownership,
    busy::{busy_interrupted, busy_private},
};
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
    /// The bundle identifier of the browser it opened, so an interrupted
    /// sign-in can still record its account.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub browser: Option<String>,
}

/// A hold whose holder died, or whose marker cannot be read: what is known
/// of it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Interrupted {
    /// The hold, when its marker can be read.
    pub hold: Option<PrivateHold>,
}

/// An interrupted hold taken over by this process: the desktop's exclusion,
/// with the person's note naming the hold's lease again, so the hold is
/// honoured as a live one by every client, and nobody can take control and
/// clear it, until [`Adopted::end`].
pub struct Adopted {
    /// The desktop the hold is on, whatever the settings later name.
    controller: Controller,
    /// Held for as long as the hold is: dropped, it lets the exclusion go.
    ownership: Option<Ownership>,
    /// Whether the hold was ended; one dropped unended is interrupted again.
    ended: bool,
    /// What is known of the hold.
    pub hold: Option<PrivateHold>,
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
    /// Its holder died, or it can't be read (no hold).
    Interrupted(Option<PrivateHold>),
}

impl Controller {
    /// The private hold in force now, when there is one: its marker names a
    /// process that still runs and the lease generation the person's note
    /// still holds. A marker that can't be read names no hold here, but
    /// still hides the screen while the person has control.
    pub fn private_hold(&self) -> Option<PrivateHold> {
        match self.found() {
            Found::Live(hold) => Some(hold),
            Found::Nothing | Found::Interrupted(_) => None,
        }
    }

    /// The interrupted hold, when there is one no process holds.
    pub fn interrupted_hold(&self) -> Option<Interrupted> {
        match self.found() {
            Found::Interrupted(hold) => Some(Interrupted { hold }),
            Found::Nothing | Found::Live(_) => None,
        }
    }

    /// Refuse an agent's change while a hold is interrupted.
    ///
    /// # Errors
    ///
    /// Returns `busy`, held by the person, while one is.
    pub(crate) fn require_not_interrupted(&self) -> Result<()> {
        match self.found() {
            Found::Interrupted(_) => Err(busy_interrupted()),
            Found::Nothing | Found::Live(_) => Ok(()),
        }
    }

    /// Take over an interrupted hold, before serving anything: this
    /// process takes the desktop's exclusion, notes the person's lease of
    /// the hold again, and becomes the marker's holder, so the hold stays in
    /// force for every client while it is resolved. None when no hold is
    /// interrupted.
    ///
    /// # Errors
    ///
    /// Returns the refusal to take the exclusion, and failures writing the
    /// note or the marker; the hold then stays interrupted.
    pub fn adopt_interrupted(&self) -> Result<Option<Adopted>> {
        if !matches!(self.found(), Found::Interrupted(_)) {
            return Ok(None);
        }
        let mut ownership = self.owning(self.take_lock()?);
        // Read again under the exclusion: the hold may have been resolved.
        let Found::Interrupted(hold) = self.found() else {
            return Ok(None);
        };
        let marker = read_own(&self.private, LIMIT)
            .and_then(|bytes| serde_json::from_slice::<Marker>(&bytes).ok());
        let lease = match &marker {
            Some(marker) => marker.lease.clone(),
            None => atlas_core::files::token(),
        };
        fs::write(&self.person, lease.as_bytes())
            .map_err(|error| Error::writing(&self.person, error))?;
        ownership.person = Some(self.person.clone());
        ownership.generation = Some(lease.clone());
        if let Some(marker) = marker {
            let holder = Instance::current().ok_or_else(|| {
                Error::new(Code::State, "macOS did not describe this process to Atlas")
            })?;
            let adopted = Marker { holder, ..marker };
            write_atomic(&self.private, &serde_json::to_vec(&adopted)?, 0o600)?;
        }
        Ok(Some(Adopted {
            controller: self.clone(),
            ownership: Some(ownership),
            ended: false,
            hold,
        }))
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
            Found::Live(_) => Err(busy_private()),
            Found::Interrupted(_) => Err(busy_interrupted()),
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
    /// exclusion and before the person's next lease is noted: one an
    /// earlier lease of a holder still running left. An interrupted hold is
    /// never cleared here; only the person ends it. The generation goes up
    /// when one was cleared, so a read admitted under it is dropped.
    pub(crate) fn clear_stale_private(&self) -> Result<()> {
        match fs::symlink_metadata(&self.private) {
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(Error::writing(&self.private, error)),
            Ok(_) => {}
        }
        match self.found() {
            Found::Interrupted(_) => Ok(()),
            Found::Nothing | Found::Live(_) => self.end_private(),
        }
    }

    /// End the private hold: the generation goes up, then the marker goes,
    /// so no step that can fail comes after the screen is shown again.
    ///
    /// # Errors
    ///
    /// Returns the failure to write the generation or remove the marker. A
    /// marker left behind is honoured as long as the lease it names, or,
    /// once its holder is gone, until the person ends it.
    pub fn end_private(&self) -> Result<()> {
        // The generation first: should it fail, the marker still hides the
        // screen, so no step that can fail comes after the screen is shown.
        self.bump_generation()?;
        match fs::remove_file(&self.private) {
            Err(error) if error.kind() != ErrorKind::NotFound => {
                Err(Error::writing(&self.private, error))
            }
            _ => Ok(()),
        }
    }

    /// Where the marker is kept, for a test to damage it.
    #[cfg(any(test, feature = "test-support"))]
    pub fn private_path_for_test(&self) -> &Path {
        &self.private
    }

    /// Leave `hold` interrupted, as a service killed while holding it
    /// leaves it: its marker names a process that is gone.
    ///
    /// # Panics
    ///
    /// When the marker cannot be written.
    #[cfg(any(test, feature = "test-support"))]
    pub fn interrupt_for_test(&self, hold: &PrivateHold) {
        self.make_directory().unwrap();
        let marker = Marker {
            hold: hold.clone(),
            holder: gone(),
            lease: atlas_core::files::token(),
            since: now_ms(),
        };
        write_atomic(&self.private, &serde_json::to_vec(&marker).unwrap(), 0o600).unwrap();
    }

    /// The marker, as far as it is in force.
    fn found(&self) -> Found {
        match fs::symlink_metadata(&self.private) {
            Err(error) if error.kind() == ErrorKind::NotFound => return Found::Nothing,
            // A marker that can't even be looked at may hold a hold.
            Err(_) => return Found::Interrupted(None),
            Ok(_) => {}
        }
        let marker = read_own(&self.private, LIMIT)
            .and_then(|bytes| serde_json::from_slice::<Marker>(&bytes).ok());
        let Some(marker) = marker else {
            return Found::Interrupted(None);
        };
        if !marker.holder.running() {
            return Found::Interrupted(Some(marker.hold));
        }
        match read_own(&self.person, LIMIT) {
            Some(person) if marker.lease.as_bytes() == person => Found::Live(marker.hold),
            // Its holder runs under another lease, or none: a hold that
            // lease ended.
            _ => Found::Nothing,
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

impl Adopted {
    /// End the hold on its own desktop, so the screen is shown again;
    /// dropping this afterwards lets the desktop's exclusion go.
    ///
    /// # Errors
    ///
    /// Returns the failure to write the generation or remove the marker.
    /// Keep this held then: letting the exclusion go would show the screen
    /// while the hold was never ended.
    pub fn end(&mut self) -> Result<()> {
        self.controller.end_private()?;
        self.ended = true;
        Ok(())
    }
}

impl Drop for Adopted {
    /// A hold dropped before it ended, as a service shutting down drops it,
    /// is interrupted again before the exclusion goes: its marker names a
    /// process that is gone, so it stays hidden with no person's note. When
    /// that cannot be written, the exclusion and the person's note are kept
    /// until this process exits, which interrupts the hold all the same.
    fn drop(&mut self) {
        let Some(ownership) = self.ownership.take() else {
            return;
        };
        if self.ended || self.controller.interrupt_again() {
            drop(ownership);
        } else {
            std::mem::forget(ownership);
        }
    }
}

impl Controller {
    /// Name a process that is gone as the holder of the marker this process
    /// holds, so its hold is interrupted once its exclusion goes. Whether
    /// the hold is interrupted, or unreadable and so interrupted anyway.
    fn interrupt_again(&self) -> bool {
        // A marker that could not be read now may still name this process:
        // only one that reads, as a hold or as nothing, can be let go.
        let Some(bytes) = read_own(&self.private, LIMIT) else {
            return false;
        };
        let Ok(marker) = serde_json::from_slice::<Marker>(&bytes) else {
            // Unreadable as a hold, it is interrupted whoever holds it.
            return true;
        };
        let interrupted = Marker {
            holder: gone(),
            ..marker
        };
        serde_json::to_vec(&interrupted)
            .ok()
            .is_some_and(|bytes| write_atomic(&self.private, &bytes, 0o600).is_ok())
    }
}

/// A process that is not running: no process has this PID and start.
fn gone() -> Instance {
    Instance::new(i32::MAX, [1, 1])
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

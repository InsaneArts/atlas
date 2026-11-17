//! Each VM's name, reserved in `names/<name>` before anything of it is
//! made, naming its identifier: made only where none is, checked against
//! the names the settings already list and every other reservation, and
//! gone only when the VM is. An add asked again with a name it reserved,
//! or unnamed while an unnamed add is still under way, goes on with that VM.
use super::{Lifecycle, create, read, record::State, remove};
use atlas_core::{
    desktop::DesktopId,
    error::{Code, Error, Result},
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Where the reservations are, in the lifecycle folder.
const NAMES: &str = "names";

/// The name an unnamed add takes first; then `vm-2`, `vm-3` and on.
const UNNAMED: &str = "vm";

/// The most names an unnamed add tries.
const TRIES: usize = 1000;

/// A reservation as kept.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Held {
    id: DesktopId,
    named: bool,
}

/// A VM's name, reserved for it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reservation {
    pub name: String,
    pub id: DesktopId,
    /// Whether the person named it.
    pub named: bool,
    /// Whether it was reserved before, by an add that is still under way.
    pub resumed: bool,
}

impl Lifecycle {
    fn names(&self) -> Result<PathBuf> {
        self.inner(NAMES)
    }

    /// Reserve `name` for a new VM, or the first free name when none is
    /// given, where the settings list `listed` already; or the reservation
    /// of the add of that name, or of the unnamed add, still under way.
    ///
    /// # Errors
    ///
    /// Refuses a name that is not a desktop name, one the settings list and
    /// one whose VM is added or being removed; fails when a reservation or
    /// a record cannot be read or made.
    pub fn reserve(&self, name: Option<&str>, listed: &[&str]) -> Result<Reservation> {
        // A name another add took meanwhile is looked at again.
        loop {
            if let Some(reservation) = self.claimed(&self.prospect(name, listed)?)? {
                return Ok(reservation);
            }
        }
    }

    /// What [`Self::reserve`] would give, with nothing made: the reservation
    /// of the add of `name`, or of the unnamed add, still under way; or a
    /// free name with a new VM's identifier, for the person to be asked
    /// about before it is [claimed](Self::claim).
    ///
    /// # Errors
    ///
    /// As [`Self::reserve`].
    pub(in crate::vm) fn prospect(
        &self,
        name: Option<&str>,
        listed: &[&str],
    ) -> Result<Reservation> {
        let fresh = |name: String, named: bool| -> Result<Reservation> {
            Ok(Reservation {
                name,
                id: fresh_id()?,
                named,
                resumed: false,
            })
        };
        let Some(name) = name else {
            for (name, held) in self.reservations()? {
                if !held.named && self.under_way(&held.id)? {
                    return Ok(Reservation {
                        name,
                        id: held.id,
                        named: false,
                        resumed: true,
                    });
                }
            }
            let names = self.names()?;
            let free = (1..=TRIES)
                .map(unnamed)
                .find(|name| !listed.contains(&name.as_str()) && !names.join(name).exists());
            return match free {
                Some(name) => fresh(name, false),
                None => Err(Error::new(
                    Code::Config,
                    "Every name an unnamed VM takes is in use; name it with --name.",
                )),
            };
        };
        plain(name)?;
        match self.held(name)? {
            Some(held) if self.under_way(&held.id)? => Ok(Reservation {
                name: name.to_owned(),
                id: held.id,
                named: held.named,
                resumed: true,
            }),
            Some(_) => Err(Error::new(
                Code::Config,
                format!("There is already a VM named \"{name}\"."),
            )
            .with(
                "next",
                serde_json::json!(format!("Use another name, or remove {name} first.")),
            )),
            None if listed.contains(&name) => Err(Error::new(
                Code::Config,
                format!("The settings already list a desktop named \"{name}\"."),
            )
            .with("next", serde_json::json!("Use another name."))),
            None => fresh(name.to_owned(), true),
        }
    }

    /// Make the reservation `prospect` ([`Self::prospect`]) names, for the
    /// add the person allowed: one resumed must still be held for its VM,
    /// still being added; a new one is made only where none is.
    ///
    /// # Errors
    ///
    /// Refuses a name taken, or given up, since it was looked at; fails when
    /// a reservation or a record cannot be read or made.
    pub(in crate::vm) fn claim(&self, prospect: &Reservation) -> Result<Reservation> {
        self.claimed(prospect)?.ok_or_else(|| {
            Error::new(
                Code::Config,
                format!(
                    "The name \"{}\" changed hands while you were asked; nothing was done. Ask \
                     again.",
                    prospect.name
                ),
            )
        })
    }

    /// [`Self::claim`]: none when the name changed hands since `prospect`.
    fn claimed(&self, prospect: &Reservation) -> Result<Option<Reservation>> {
        if prospect.resumed {
            let held = self.held(&prospect.name)?;
            let still = match held {
                Some(held) if held.id == prospect.id => self.under_way(&held.id)?,
                _ => false,
            };
            return Ok(still.then(|| prospect.clone()));
        }
        let held = Held {
            id: prospect.id.clone(),
            named: prospect.named,
        };
        let made = create(&self.names()?.join(&prospect.name), &held)?;
        Ok(made.then(|| prospect.clone()))
    }

    /// Whether the VM `id` is still being added: its record says so, or it
    /// has none yet, as an add cut short just after its reservation leaves
    /// it.
    fn under_way(&self, id: &DesktopId) -> Result<bool> {
        Ok(self
            .record(id)?
            .is_none_or(|record| record.state == State::Adding))
    }

    /// The reservation of `name`; none when there is none.
    fn held(&self, name: &str) -> Result<Option<Held>> {
        read(&self.names()?.join(name), "A VM's name reservation")
    }

    /// Every reservation, by name in order.
    fn reservations(&self) -> Result<Vec<(String, Held)>> {
        let names = self.names()?;
        let entries = std::fs::read_dir(&names).map_err(|error| Error::writing(&names, error))?;
        let mut held = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|error| Error::writing(&names, error))?;
            let Some(name) = entry
                .file_name()
                .to_str()
                .filter(|name| atlas_core::config::desktops::plain_name(name))
                .map(str::to_owned)
            else {
                continue;
            };
            if let Some(reservation) = self.held(&name)? {
                held.push((name, reservation));
            }
        }
        held.sort_by(|one, other| one.0.cmp(&other.0));
        Ok(held)
    }

    /// The name the VM `id` reserved, when it reserved one.
    ///
    /// # Errors
    ///
    /// Fails when a reservation cannot be read.
    pub fn reserved_by(&self, id: &DesktopId) -> Result<Option<String>> {
        Ok(self
            .reservations()?
            .into_iter()
            .find(|(_, held)| held.id == *id)
            .map(|(name, _)| name))
    }

    /// Give `name` up, while the VM `id` holds it: once that VM is gone.
    ///
    /// # Errors
    ///
    /// Fails when the reservation cannot be read or removed.
    pub fn release(&self, name: &str, id: &DesktopId) -> Result<()> {
        match self.held(name)? {
            Some(held) if held.id == *id => remove(&self.names()?.join(name)),
            _ => Ok(()),
        }
    }
}

/// Refuse `name` unless it is a desktop name.
fn plain(name: &str) -> Result<()> {
    if atlas_core::config::desktops::plain_name(name) {
        return Ok(());
    }
    Err(Error::new(
        Code::BadRequest,
        format!(
            "\"{name}\" is not a desktop name: use 1 to 32 lowercase letters, digits and dashes"
        ),
    ))
}

/// The name an unnamed add tries `at`th: `vm`, then `vm-2` and on.
fn unnamed(at: usize) -> String {
    if at == 1 {
        UNNAMED.to_owned()
    } else {
        format!("{UNNAMED}-{at}")
    }
}

/// A new VM's identifier, as Atlas VM's are: 24 lowercase hexadecimal
/// digits.
fn fresh_id() -> Result<DesktopId> {
    let token = atlas_core::files::token();
    token
        .get(..24)
        .and_then(DesktopId::parse)
        .ok_or_else(|| Error::new(Code::State, "A VM's identifier could not be made."))
}

#[cfg(test)]
#[path = "names_tests.rs"]
mod tests;

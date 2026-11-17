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
        match name {
            Some(name) => self.reserve_named(name, listed),
            None => self.reserve_unnamed(listed),
        }
    }

    fn reserve_named(&self, name: &str, listed: &[&str]) -> Result<Reservation> {
        if !atlas_core::config::desktops::plain_name(name) {
            return Err(Error::new(
                Code::BadRequest,
                format!(
                    "\"{name}\" is not a desktop name: use 1 to 32 lowercase letters, digits and \
                     dashes"
                ),
            ));
        }
        loop {
            if let Some(held) = self.held(name)? {
                if !self.under_way(&held.id)? {
                    return Err(Error::new(
                        Code::Config,
                        format!("There is already a VM named \"{name}\"."),
                    ));
                }
                return Ok(Reservation {
                    name: name.to_owned(),
                    id: held.id,
                    named: held.named,
                    resumed: true,
                });
            }
            if listed.contains(&name) {
                return Err(Error::new(
                    Code::Config,
                    format!("The settings already list a desktop named \"{name}\"."),
                ));
            }
            if let Some(reservation) = self.reserve_new(name, true)? {
                return Ok(reservation);
            }
        }
    }

    fn reserve_unnamed(&self, listed: &[&str]) -> Result<Reservation> {
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
        for at in 1..=TRIES {
            let name = if at == 1 {
                UNNAMED.to_owned()
            } else {
                format!("{UNNAMED}-{at}")
            };
            if listed.contains(&name.as_str()) || names.join(&name).exists() {
                continue;
            }
            if let Some(reservation) = self.reserve_new(&name, false)? {
                return Ok(reservation);
            }
        }
        Err(Error::new(
            Code::Config,
            "Every name an unnamed VM takes is in use; name it with --name.",
        ))
    }

    /// `name` reserved for a new VM, made only where no reservation is;
    /// none when another was made meanwhile.
    fn reserve_new(&self, name: &str, named: bool) -> Result<Option<Reservation>> {
        let id = fresh_id()?;
        let held = Held {
            id: id.clone(),
            named,
        };
        if !create(&self.names()?.join(name), &held)? {
            return Ok(None);
        }
        Ok(Some(Reservation {
            name: name.to_owned(),
            id,
            named,
            resumed: false,
        }))
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

//! Which Atlas made a VM's Keychain items. An item is Atlas's own only for
//! the program at the very path, with the very requirement, that made it
//! (`atlas_native::keychain`), so a VM's lifecycle pins both, and the
//! station's shared key is checked readable by the program its keeper will
//! run before anything of a VM is made.
use atlas_core::error::{Code, Error, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// The Atlas that made a VM's Keychain items: its program's path and its
/// designated requirement.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Provenance {
    pub program: PathBuf,
    pub requirement: String,
}

impl Provenance {
    /// This Atlas's.
    ///
    /// # Errors
    ///
    /// Fails when this program's path or signing cannot be read.
    pub fn own() -> Result<Self> {
        Ok(Self {
            program: this_program()?,
            requirement: atlas_native::keychain::own_requirement()?,
        })
    }

    /// Go on only as the Atlas that made the VM's items: `own` is this
    /// one's.
    ///
    /// # Errors
    ///
    /// Refuses another path or requirement, with what to do instead: a new
    /// secret is never made for a guest already given one.
    pub fn check(&self, own: &Self) -> Result<()> {
        if self == own {
            return Ok(());
        }
        Err(Error::new(
            Code::Identity,
            format!(
                "This VM was begun by the Atlas at {}, and its Keychain items are that Atlas's \
                 alone. Run that Atlas again to go on, or remove the VM.",
                self.program.display()
            ),
        ))
    }
}

/// Check, before anything of a VM is made, that the station's key is one
/// the keeper can read: the keeper runs the Atlas.app this program is in,
/// so this program must be that one, and must read the key (making it when
/// there is none).
///
/// # Errors
///
/// Refuses this program run from elsewhere than its app's own, and a key
/// another program made, with what to do instead; fails when the Keychain
/// cannot be read.
pub fn preflight_station_key() -> Result<()> {
    let keeper = std::fs::canonicalize(
        atlas_core::app_bundle::installed_app()?.join(atlas_core::app_bundle::BUNDLE_EXECUTABLE),
    )?;
    preflight_with(&this_program()?, &keeper, || {
        crate::station::Station::own()?.key().map(|_| ())
    })
}

fn preflight_with(this: &Path, keeper: &Path, key: impl FnOnce() -> Result<()>) -> Result<()> {
    if this != keeper {
        return Err(Error::new(
            Code::Identity,
            format!(
                "A VM's keeper runs {}, not this atlas ({}), and the Keychain keeps Atlas's items \
                 for one program alone. Run the atlas in Atlas.app, then try again.",
                keeper.display(),
                this.display()
            ),
        ));
    }
    key().map_err(|error| {
        if error.code != Code::Identity {
            return error;
        }
        Error::new(
            Code::Identity,
            "The station's key was made by Atlas at another path, so this Atlas cannot use it \
             and no VM is made. Run Atlas from where it was when its first VM was added, or \
             remove every VM first.",
        )
        .with(
            "next",
            serde_json::json!("Open Atlas from where it was when its first VM was added."),
        )
        .with_source(error)
    })
}

/// This program, by its real path.
fn this_program() -> Result<PathBuf> {
    Ok(std::fs::canonicalize(std::env::current_exe()?)?)
}

#[cfg(test)]
#[path = "provenance_tests.rs"]
mod tests;

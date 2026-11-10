//! The person's choice to let agents copy sign-ins and secrets into the
//! helper. It is off unless the person turned it on: while the record is
//! missing, or can't be looked at, Atlas refuses such copies. Only the person
//! turns it on; anyone may turn it off. Like Stop, it holds against agents
//! using Atlas's commands, not against a shell that writes the record itself.
use atlas_core::error::{Code, Error, Result};
use std::{fs, io::ErrorKind, path::Path};

/// Where the choice is kept, under the person's home folder.
const RECORD: &str = "Library/Application Support/Atlas/sign-in-copies";

/// Whether the person lets agents copy sign-ins and secrets into the helper.
#[must_use]
pub fn allowed() -> bool {
    atlas_core::config::home().is_ok_and(|home| allowed_in(&home))
}

/// [`allowed`] for the home folder `home`.
pub(crate) fn allowed_in(home: &Path) -> bool {
    matches!(home.join(RECORD).try_exists(), Ok(true))
}

/// Let agents copy sign-ins in (`allow`, the person only) or stop them,
/// and say whether they may now.
///
/// # Errors
///
/// Returns `stopped` when an agent asks to allow copies, and file errors
/// when the choice can't be written.
pub fn set(allow: bool, by_person: bool) -> Result<bool> {
    set_in(&atlas_core::config::home()?, allow, by_person)
}

/// [`set`] for the home folder `home`.
pub(crate) fn set_in(home: &Path, allow: bool, by_person: bool) -> Result<bool> {
    let record = home.join(RECORD);
    if allow {
        if !by_person {
            return Err(Error::new(
                Code::Stopped,
                "Only the person lets agents copy sign-ins into the helper, from Atlas › Settings › \
                 Privacy",
            ));
        }
        if let Some(parent) = record.parent() {
            fs::create_dir_all(parent).map_err(|error| Error::writing(parent, error))?;
        }
        fs::write(&record, b"allowed\n").map_err(|error| Error::writing(&record, error))?;
    } else {
        match fs::remove_file(&record) {
            Ok(()) => {}
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(error) => return Err(Error::writing(&record, error)),
        }
    }
    Ok(allowed_in(home))
}

#[cfg(test)]
mod tests;

//! The installer's own copy of Atlas, which its job runs: a whole bundle in
//! the installer's folder, apart from the one it replaces, since Atlas's
//! signature seals the program inside its bundle and a program copied out
//! alone is not Atlas's code. A next copy is moved beside it, swapped in,
//! and the one it replaced removed; done again, any of that cut short
//! finishes.
use atlas_core::{
    error::{Code, Error, Result},
    fs_move::{FileName, Moved, exchange_between, move_between},
};
use std::{fs::File, path::Path};

/// The installer's own copy, in its folder.
pub(in crate::machine) const OWN: &str = "Atlas.app";
/// A next copy beside it, about to take its place; or, once swapped, the
/// one it replaced, about to go.
const INCOMING: &str = "Atlas.app.incoming";

fn name(text: &str) -> FileName {
    FileName::new(text).expect("a plain name")
}

fn io(error: impl std::error::Error + Send + Sync + 'static) -> Error {
    Error::new(
        Code::Io,
        "The installer's own copy could not be put in place.",
    )
    .with_source(error)
}

fn synced(moved: Moved) -> Result<()> {
    match moved {
        Moved::Synced => Ok(()),
        Moved::Unsynced(error) => Err(io(error)),
    }
}

/// Make the copy `next` in `from`, when given, the installer's own in
/// `home`; `wanted` says whether a bundle is the code it must be.
///
/// # Errors
///
/// Refuses when neither the own copy nor a next one is the wanted code;
/// fails when a move cannot be done.
pub(in crate::machine) fn take(
    home: &Path,
    next: Option<(&File, &FileName)>,
    wanted: &dyn Fn(&Path) -> bool,
) -> Result<()> {
    let folder = File::open(home).map_err(io)?;
    let incoming = home.join(INCOMING);
    if let Some((from, next)) = next {
        remove(&incoming)?;
        synced(move_between(from, next, &folder, &name(INCOMING)).map_err(io)?)?;
    }
    let own = home.join(OWN);
    if !wanted(&own) {
        if !wanted(&incoming) {
            return Err(Error::new(
                Code::BadRequest,
                "The installer's own copy of Atlas is not the one it must be.",
            ));
        }
        if std::fs::symlink_metadata(&own).is_ok() {
            synced(exchange_between(&folder, &name(OWN), &folder, &name(INCOMING)).map_err(io)?)?;
        } else {
            synced(move_between(&folder, &name(INCOMING), &folder, &name(OWN)).map_err(io)?)?;
        }
    }
    remove(&incoming)?;
    folder.sync_all().map_err(io)
}

/// Remove the folder `path`, unless it is already gone.
fn remove(path: &Path) -> Result<()> {
    match std::fs::remove_dir_all(path) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(io(error)),
        _ => Ok(()),
    }
}

#[cfg(test)]
#[path = "own_tests.rs"]
mod tests;

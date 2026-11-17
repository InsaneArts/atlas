//! A desktop gone with its work forgotten: everything its controller keeps
//! in Atlas's state folder, its claims, marker, Stop, notes and locks, and
//! nothing of any other desktop's.
use super::Controller;
use atlas_core::{
    context::context,
    error::{Code, Error, Result},
    files::open_lock,
};
use fs2::FileExt;
use std::{fs, time::Duration};

/// How long the controller's own locks are waited for.
const LOCKS: Duration = Duration::from_secs(2);

impl Controller {
    /// Forget everything this controller keeps of its desktop, for a desktop
    /// that is gone and its work with it: every file and folder named for
    /// it in the state folder, removed under its control, marker and Stop
    /// locks, links never followed, and the folder synced.
    ///
    /// # Errors
    ///
    /// Busy while another process holds control or the Stop's lock, or the
    /// marker's past a short wait: nothing is removed. Fails when something
    /// named for it cannot be removed; what was removed stays removed.
    pub fn forget(&self) -> Result<()> {
        let busy = || {
            Error::new(
                Code::Busy,
                "Another Atlas command still holds this desktop's control.",
            )
        };
        if !self
            .directory
            .try_exists()
            .map_err(context("Inspect state folder", &self.directory))?
        {
            return Ok(());
        }
        let control = open_lock(&self.lock)?;
        FileExt::try_lock_exclusive(&control.0).map_err(|_| busy())?;
        let stop = open_lock(&self.stop_lock)?;
        FileExt::try_lock_exclusive(&stop.0).map_err(|_| busy())?;
        let _marker = self.lock_marker_within(LOCKS)?;
        let named = format!("{}.", self.name);
        let entries =
            fs::read_dir(&self.directory).map_err(context("List state folder", &self.directory))?;
        for entry in entries {
            let entry = entry.map_err(context("List state folder", &self.directory))?;
            let path = entry.path();
            if !entry
                .file_name()
                .to_str()
                .is_some_and(|name| name.starts_with(&named))
            {
                continue;
            }
            let kind = entry
                .file_type()
                .map_err(context("Inspect state record", &path))?;
            let removed = if kind.is_dir() {
                fs::remove_dir_all(&path)
            } else {
                fs::remove_file(&path)
            };
            match removed {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                other => other.map_err(context("Remove state record", &path))?,
            }
        }
        fs::File::open(&self.directory)
            .and_then(|folder| folder.sync_all())
            .map_err(context("Sync state folder", &self.directory))?;
        Ok(())
    }
}

#[cfg(test)]
#[path = "forget_tests.rs"]
mod tests;

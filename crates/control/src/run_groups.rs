//! The process groups of commands agents ran that finished while something
//! they started still ran: `atlas run` lets its command's group go on once
//! the command itself exits. Each is kept until nothing in it runs as the
//! helper, so the person never signs in beside an agent's work.
use super::{Controller, Purpose, busy::busy_command};
use atlas_core::{
    error::{Error, Result},
    files::{read_own, write_atomic},
    protocol::left_may_run,
};
use std::{fs, io::ErrorKind};

/// The most groups kept: one past it lets the oldest that ended go, never
/// one that still runs.
const KEPT: usize = 256;
/// The most the record may hold before it is treated as unreadable.
const LIMIT: u64 = 64 * 1024;

impl Controller {
    /// Keep `group`, the process group of a command an agent ran that just
    /// finished, when anything in it still runs as `helper`. Groups whose
    /// processes all ended go meanwhile.
    ///
    /// # Errors
    ///
    /// Returns the failure to read or write the record.
    pub fn note_run_group(&self, group: i32, helper: u32) -> Result<()> {
        if group <= 0 || !left_may_run(&[], Some(group), helper) {
            return Ok(());
        }
        let _lock = self.lock_marker()?;
        let mut groups = self.read_run_groups()?;
        groups.retain(|kept| *kept != group && left_may_run(&[], Some(*kept), helper));
        groups.push(group);
        if groups.len() > KEPT {
            let over = groups.len() - KEPT;
            groups.drain(..over);
        }
        self.write_run_groups(&groups)
    }

    /// Refuse while a group [`Controller::note_run_group`] kept still runs
    /// anything as `helper`, as the person's sign-in must; those that ended
    /// go.
    ///
    /// # Errors
    ///
    /// Returns `busy`, held by an agent with `running` `command`, while one
    /// runs, and the failure to read the record: a group it can't account
    /// for may run.
    pub fn require_no_run_left(&self, helper: u32) -> Result<()> {
        let _lock = self.lock_marker()?;
        let groups = self.read_run_groups()?;
        let running: Vec<i32> = groups
            .iter()
            .copied()
            .filter(|group| left_may_run(&[], Some(*group), helper))
            .collect();
        if running.len() != groups.len() {
            // Forgetting what ended is tidying: a failure keeps it for later.
            let _ = self.write_run_groups(&running);
        }
        if running.is_empty() {
            Ok(())
        } else {
            Err(busy_command(Purpose::Change(super::Beside::Nothing)))
        }
    }

    fn read_run_groups(&self) -> Result<Vec<i32>> {
        match fs::symlink_metadata(&self.run_groups) {
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(Error::writing(&self.run_groups, error)),
            Ok(_) => {}
        }
        read_own(&self.run_groups, LIMIT)
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .ok_or_else(|| {
                Error::new(
                    atlas_core::error::Code::State,
                    "Atlas couldn't read which agents' commands left work running. Try again.",
                )
            })
    }

    fn write_run_groups(&self, groups: &[i32]) -> Result<()> {
        if groups.is_empty() {
            return match fs::remove_file(&self.run_groups) {
                Err(error) if error.kind() != ErrorKind::NotFound => {
                    Err(Error::writing(&self.run_groups, error))
                }
                _ => Ok(()),
            };
        }
        write_atomic(&self.run_groups, &serde_json::to_vec(groups)?, 0o600)
    }
}

#[cfg(test)]
mod tests;

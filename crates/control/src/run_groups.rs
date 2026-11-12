//! The process groups of commands agents ran that finished while something
//! they started still ran: `atlas run` lets its command's group go on once
//! the command itself exits. Each is kept until nothing in it runs as the
//! helper, so the person never signs in beside an agent's work. A group is
//! never let go while it runs: past [`KEPT`], the record is marked
//! overflowing instead, which refuses until a full look at the helper's
//! processes finds none left over. A command whose group could not be
//! recorded keeps its claim as an unknown outcome, with its group noted
//! beside the marker when it can be, until a later look records it or finds
//! it ended.
use super::{Controller, Pending, Purpose, busy::busy_command};
use atlas_core::{
    error::{Code, Error, Result},
    files::{read_own, write_atomic},
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs, io::ErrorKind, path::Path};

/// The most groups kept. One more marks the record overflowing; none that
/// runs is ever let go.
#[cfg(not(test))]
pub(crate) const KEPT: usize = 256;
/// Few enough for a test to overflow.
#[cfg(test)]
pub(crate) const KEPT: usize = 2;
/// The most a record may hold before it is treated as unreadable.
const LIMIT: u64 = 64 * 1024;

/// The record: the groups kept, and whether one past [`KEPT`] could not be.
#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    groups: Vec<i32>,
    #[serde(default)]
    overflow: bool,
}

impl Controller {
    /// Keep `group`, the process group of a command an agent ran that just
    /// finished, when anything in it still runs in this desktop. Groups whose
    /// processes all ended go meanwhile; past [`KEPT`] running ones, the
    /// record is marked overflowing instead.
    ///
    /// # Errors
    ///
    /// Returns the failure to read or write the record.
    pub fn note_run_group(&self, group: i32) -> Result<()> {
        if group <= 0 || !self.desktop.left_may_run(&[], Some(group)) {
            return Ok(());
        }
        let _lock = self.lock_marker()?;
        self.keep_run_group(group)
    }

    /// [`Controller::note_run_group`] while the caller holds the marker lock.
    fn keep_run_group(&self, group: i32) -> Result<()> {
        let mut record = read::<Record>(&self.run_groups)?.unwrap_or_default();
        record
            .groups
            .retain(|kept| *kept != group && self.desktop.left_may_run(&[], Some(*kept)));
        if record.groups.len() < KEPT {
            record.groups.push(group);
        } else {
            record.overflow = true;
        }
        self.write_run_groups(&record)
    }

    /// Keep `claim`, of a command whose `group` could not be recorded, as an
    /// unknown outcome: its group is noted beside the marker, when it can
    /// be, so a later look records it or finds it ended and settles it.
    pub fn keep_unrecorded_run(&self, claim: Pending, group: i32) {
        if let Ok(_lock) = self.lock_marker() {
            let mut unrecorded = read::<BTreeMap<String, i32>>(&self.unrecorded_runs)
                .ok()
                .flatten()
                .unwrap_or_default();
            unrecorded.insert(claim.generation().to_owned(), group);
            let _ = write_record(&self.unrecorded_runs, &unrecorded);
        }
        self.abandon_command(claim);
    }

    /// Record the groups of commands earlier settles could not record, and
    /// settle their claims: one whose group still runs in the desktop joins
    /// the record, one whose group ended is settled as it is. What fails
    /// stays as it was, for the next look.
    pub(crate) fn settle_unrecorded_runs(&self) {
        let settled = {
            let Ok(_lock) = self.lock_marker() else {
                return;
            };
            let Ok(Some(unrecorded)) = read::<BTreeMap<String, i32>>(&self.unrecorded_runs) else {
                return;
            };
            let mut settled = Vec::new();
            let mut left = BTreeMap::new();
            for (generation, group) in unrecorded {
                let kept = !self.desktop.left_may_run(&[], Some(group))
                    || self.keep_run_group(group).is_ok();
                if kept {
                    settled.push(generation);
                } else {
                    left.insert(generation, group);
                }
            }
            if write_record(&self.unrecorded_runs, &left).is_err() {
                return;
            }
            settled
        };
        for generation in settled {
            let _ = self.clear_if(&generation);
        }
    }

    /// Refuse while a group [`Controller::note_run_group`] kept still runs
    /// anything in this desktop, as the person's sign-in must; those that ended
    /// go. An overflowing record refuses until it keeps fewer than
    /// [`KEPT`] running groups and a full look finds no process of the
    /// desktop's left in a group whose leader is gone, as a command's
    /// leftover work is.
    ///
    /// # Errors
    ///
    /// Returns `busy`, held by an agent with `running` `command`, while one
    /// runs, and the failure to read the record or the desktop's processes:
    /// a group it can't account for may run.
    pub fn require_no_run_left(&self) -> Result<()> {
        self.settle_unrecorded_runs();
        let _lock = self.lock_marker()?;
        let record = read::<Record>(&self.run_groups)?.unwrap_or_default();
        let running: Vec<i32> = record
            .groups
            .iter()
            .copied()
            .filter(|group| self.desktop.left_may_run(&[], Some(*group)))
            .collect();
        let overflow = record.overflow && (running.len() >= KEPT || self.desktop.orphans_run()?);
        if running.len() != record.groups.len() || overflow != record.overflow {
            // Forgetting what ended is tidying: a failure keeps it for later.
            let _ = self.write_run_groups(&Record {
                groups: running.clone(),
                overflow,
            });
        }
        if running.is_empty() && !overflow {
            Ok(())
        } else {
            Err(busy_command(Purpose::Change(super::Beside::Nothing)))
        }
    }

    fn write_run_groups(&self, record: &Record) -> Result<()> {
        if record.groups.is_empty() && !record.overflow {
            return remove(&self.run_groups);
        }
        write_record(&self.run_groups, record)
    }
}

/// The record at `path`: None when there is none.
fn read<T: serde::de::DeserializeOwned>(path: &Path) -> Result<Option<T>> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(Error::writing(path, error)),
        Ok(_) => {}
    }
    read_own(path, LIMIT)
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .map(Some)
        .ok_or_else(|| {
            Error::new(
                Code::State,
                "Atlas couldn't read which agents' commands left work running. Try again.",
            )
        })
}

/// Write `value` whole at `path`, or remove it when it is empty.
fn write_record<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let bytes = serde_json::to_vec(value)?;
    if bytes == b"{}" || bytes == b"[]" {
        return remove(path);
    }
    write_atomic(path, &bytes, 0o600)
}

fn remove(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Err(error) if error.kind() != ErrorKind::NotFound => Err(Error::writing(path, error)),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests;

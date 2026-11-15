//! A VM desktop removed. Under the station's exclusion, with its record
//! kept outside the folder it deletes, before each step and after each
//! outcome, so a removal cut short goes on from it; while the record is
//! there no runner starts and no agent is installed. The folder is deleted
//! only once no runner of it lives and every VM its runs started is gone.
use super::{
    System, Vms,
    evidence::{self, Run},
    sync, take_lock, try_lock,
};
use atlas_core::{
    desktop::DesktopId,
    error::{Error, Result},
    files::Token,
    fs_move::{FileName, Moved, move_between},
    launchd::Job,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs::File,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
};

/// How far a removal got.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Step {
    /// Recorded: from here no runner starts and no agent is installed.
    Marked,
    /// Its runner asked to stop the VM, delete its passwords and exit.
    Forgetting,
    /// Its agent going.
    Retiring,
    /// No runner, and no VM it started, shown to live.
    Proving,
    /// Its folder going.
    Deleting,
    /// Its passwords going.
    Passwords,
    /// Done.
    Done,
}

/// Which file a path named when it was seen.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Identity {
    device: u64,
    inode: u64,
}

impl Identity {
    /// What is at `path` now, never through a link; none when nothing is.
    ///
    /// # Errors
    ///
    /// Fails when what is there cannot be read, which is never absent.
    fn of(path: &Path) -> Result<Option<Self>> {
        match std::fs::symlink_metadata(path) {
            Ok(seen) => Ok(Some(Self {
                device: seen.dev(),
                inode: seen.ino(),
            })),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(Error::writing(path, error)),
        }
    }
}

/// A removal as kept.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Record {
    change: Token,
    step: Step,
    /// The folder when the removal began; none when there was none.
    folder: Option<Identity>,
    /// What the runner said to `forget`, or that it did not answer.
    forgot: Option<Value>,
    /// The runs proved gone, kept before anything is deleted.
    proved: Option<Vec<Run>>,
    /// What became of each password.
    passwords: BTreeMap<String, String>,
    /// What was found in the folder's places and left, being another.
    kept: Vec<String>,
}

/// Remove the VM desktop `desktop`, as the removal `change`, or go on with
/// the one under way.
///
/// # Errors
///
/// Fails when its record cannot be kept or read, or a step fails in a way
/// that says nothing of how far it got; the removal then goes on when
/// asked again.
pub fn remove(desktop: &DesktopId, change: &Token) -> Result<Value> {
    remove_with(&Vms::own()?, &super::Real, desktop, change)
}

fn pending(step: Step, why: &str) -> Value {
    json!({"ok": true, "removal": "pending", "step": step, "why": why})
}

fn answer(record: &Record) -> Value {
    json!({
        "ok": true,
        "removal": "done",
        "change": record.change,
        "passwords": record.passwords,
        "kept": record.kept,
    })
}

fn remove_with(
    vms: &Vms,
    system: &impl System,
    desktop: &DesktopId,
    change: &Token,
) -> Result<Value> {
    let Some(_station) = try_lock(&vms.station_lock(desktop))? else {
        return Ok(pending(
            Step::Marked,
            "Another station operation holds this VM.",
        ));
    };
    let marker = vms.removal(desktop);
    let mut record = match read(&marker)? {
        // Done: written through again first, so an answer given is one
        // a power cut cannot take back.
        Some(record) if record.step == Step::Done => {
            sync(&vms.removals())?;
            return Ok(answer(&record));
        }
        Some(record) if &record.change != change => {
            return Ok(json!({"ok": false, "error": "Another removal of this VM is under way."}));
        }
        Some(record) => record,
        None => {
            atlas_core::files::private_folder(&vms.removals(), atlas_core::error::Code::State)?;
            sync(&vms.parent)?;
            let record = Record {
                change: change.clone(),
                step: Step::Marked,
                folder: Identity::of(&vms.folder(desktop))?,
                forgot: None,
                proved: None,
                passwords: BTreeMap::new(),
                kept: Vec::new(),
            };
            keep(vms, &marker, &record)?;
            record
        }
    };
    let folder = vms.folder(desktop);
    let (domain, label) = (vms.domain(), Vms::label(desktop));

    if record.step == Step::Marked {
        record.step = Step::Forgetting;
        keep(vms, &marker, &record)?;
    }
    if record.step == Step::Forgetting {
        // Answered or not, it goes on: what follows proves the rest.
        let asked = json!({"op": "forget", "change": change.as_str()});
        record.forgot = Some(
            system
                .ask(&folder.join("control"), &asked)
                .unwrap_or_else(|_| json!({"answered": false})),
        );
        record.step = Step::Retiring;
        keep(vms, &marker, &record)?;
    }
    if record.step == Step::Retiring {
        retire(vms, system, desktop)?;
        if system.job(&domain, &label)? != Job::Absent {
            return Ok(pending(
                Step::Retiring,
                "launchd has not let the VM's job go yet.",
            ));
        }
        record.step = Step::Proving;
        keep(vms, &marker, &record)?;
    }
    let still = "A runner of this VM still runs; one that does not quit by itself needs quitting.";
    // Held to the end: no runner lives, or can start, while they are.
    let Some(_lifecycle) = try_lock(&vms.lifecycle(desktop))? else {
        // One that was busy with another change is asked again, the same
        // change, which it answers from its record once done.
        let asked = json!({"op": "forget", "change": change.as_str()});
        if let Ok(reply) = system.ask(&folder.join("control"), &asked) {
            record.forgot = Some(reply);
            keep(vms, &marker, &record)?;
        }
        return Ok(pending(record.step, still));
    };
    // The folder's own lock too, wherever the folder the removal began
    // with is now: older builds take it and no other, and made when
    // missing, so none takes it after.
    let aside = aside_of(vms, desktop, &record.change);
    let place = if ours(&record, &folder)? {
        Some(folder.clone())
    } else if ours(&record, &aside)? {
        Some(aside.clone())
    } else {
        None
    };
    let _runner = match &place {
        Some(place) => match take_lock(&place.join("Runner.lock"))? {
            Some(lock) => Some(lock),
            None => return Ok(pending(record.step, still)),
        },
        None => None,
    };
    if system.job(&domain, &label)? != Job::Absent {
        retire(vms, system, desktop)?;
        return Ok(pending(
            record.step,
            "launchd has not let the VM's job go yet.",
        ));
    }
    if record.step == Step::Proving {
        let runs = match &place {
            Some(place) => evidence::runs(&place.join("Incarnation.json")),
            None => Some(Vec::new()),
        };
        let Some(runs) = runs else {
            return Ok(pending(
                Step::Proving,
                "What this VM's runs left cannot be read.",
            ));
        };
        if let Some(why) = evidence::not_gone(&runs, system) {
            return Ok(pending(Step::Proving, why));
        }
        record.proved = Some(runs);
        record.step = Step::Deleting;
        keep(vms, &marker, &record)?;
    }
    if record.step == Step::Deleting {
        delete(vms, desktop, &mut record)?;
        record.step = Step::Passwords;
        keep(vms, &marker, &record)?;
    }
    if record.step == Step::Passwords {
        let forgotten = record
            .forgot
            .as_ref()
            .is_some_and(|forgot| forgot["outcome"]["state"] == "done");
        for account in [
            desktop.as_str().to_owned(),
            format!("{}.admin", desktop.as_str()),
        ] {
            if record.passwords.contains_key(&account) {
                continue;
            }
            let became = if forgotten {
                "deleted"
            } else {
                match system.delete_password(&account) {
                    Ok(true) => "deleted",
                    Ok(false) => "absent",
                    Err(_) => "left",
                }
            };
            record.passwords.insert(account, became.to_owned());
        }
        // A bootstrap still on its way from a crashed install may have
        // come since: not done while the job is there.
        if system.job(&domain, &label)? != Job::Absent {
            retire(vms, system, desktop)?;
            keep(vms, &marker, &record)?;
            return Ok(pending(
                Step::Passwords,
                "launchd has not let the VM's job go yet.",
            ));
        }
        record.step = Step::Done;
        keep(vms, &marker, &record)?;
    }
    Ok(answer(&record))
}

/// The agent's plist removed first, so a bootstrap still on its way has
/// nothing to load, then its job booted out; whether it went is read back
/// by the caller, never taken from launchctl's word.
fn retire(vms: &Vms, system: &impl System, desktop: &DesktopId) -> Result<()> {
    let plist = vms.agent_plist(desktop);
    match std::fs::remove_file(&plist) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
            return Err(Error::writing(&plist, error));
        }
        _ => {}
    }
    if vms.agents().exists() {
        sync(&vms.agents())?;
    }
    let job = format!("{}/{}", vms.domain(), Vms::label(desktop));
    let _ = system.launchctl(&["bootout", &job])?;
    Ok(())
}

/// Where the folder of `desktop` is moved aside by the removal `change`.
fn aside_of(vms: &Vms, desktop: &DesktopId, change: &Token) -> PathBuf {
    vms.parent.join(format!(
        ".removing-{}-{}",
        desktop.as_str(),
        change.as_str()
    ))
}

/// Whether `path` is the folder the removal `record` began with.
fn ours(record: &Record, path: &Path) -> Result<bool> {
    Ok(record.folder.is_some() && Identity::of(path)? == record.folder)
}

/// The folder recorded at the removal's start moved aside, then deleted,
/// each judged by its identity wherever it is: another found in either
/// place is never touched, and named. `VMs` is written through before it
/// returns, however far an earlier try got.
fn delete(vms: &Vms, desktop: &DesktopId, record: &mut Record) -> Result<()> {
    let source = vms.folder(desktop);
    let aside = aside_of(vms, desktop, &record.change);
    if ours(record, &source)? {
        let parent = File::open(&vms.parent).map_err(|error| Error::writing(&vms.parent, error))?;
        let (from, to) = (name(&source)?, name(&aside)?);
        match move_between(&parent, &from, &parent, &to)
            .map_err(|error| Error::writing(&source, error))?
        {
            Moved::Synced => {}
            Moved::Unsynced(error) => return Err(Error::writing(&vms.parent, error)),
        }
    }
    if ours(record, &aside)? {
        std::fs::remove_dir_all(&aside).map_err(|error| Error::writing(&aside, error))?;
    }
    sync(&vms.parent)?;
    for place in [&source, &aside] {
        let shown = place.to_string_lossy().into_owned();
        if Identity::of(place)?.is_some() && !ours(record, place)? && !record.kept.contains(&shown)
        {
            record.kept.push(shown);
        }
    }
    Ok(())
}

fn name(path: &Path) -> Result<FileName> {
    path.file_name()
        .and_then(|name| name.to_str())
        .and_then(FileName::new)
        .ok_or_else(|| {
            Error::new(
                atlas_core::error::Code::State,
                "A VM's folder has no plain name.",
            )
        })
}

/// The record at `marker`; none when there is none.
fn read(marker: &Path) -> Result<Option<Record>> {
    match std::fs::symlink_metadata(marker) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(Error::writing(marker, error)),
        Ok(_) => {}
    }
    let bytes = atlas_core::files::read_own(marker, 256 * 1024).ok_or_else(|| {
        Error::new(
            atlas_core::error::Code::State,
            "This VM's removal record cannot be read.",
        )
    })?;
    Ok(Some(serde_json::from_slice(&bytes)?))
}

/// Keep `record` at `marker`: written whole beside it, renamed, and its
/// folder written through.
fn keep(vms: &Vms, marker: &Path, record: &Record) -> Result<()> {
    atlas_core::files::save_private(marker, &serde_json::to_vec(record)?)?;
    sync(&vms.removals())
}

#[cfg(test)]
#[path = "removal_tests.rs"]
mod tests;

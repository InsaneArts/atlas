//! Removing a VM desktop, `atlas desktop remove <name>` (or `--id <id>`):
//! the person's grant through Atlas.app first, and the settings the
//! removal leaves checked before anything is touched. Then, under the VM's
//! station-operation lock and one removal at a time per settings file, it
//! is counted with every removal already under way, so the settings always
//! keep a desktop; its lifecycle record says `removing` with the removal's
//! change (an earlier removal's, when one is recorded), kept before the
//! removal runs. Once the removal answered done, the settings entry goes,
//! then the name's reservation, and the record last. A VM half made, its
//! add cut short and maybe never in the settings, is removed the same way.
//! A removal that kept paths, being another's, or left passwords it could
//! not delete without asking keeps the record `removing` and its name
//! taken, and says what and why; nothing later deletes a path it kept.
//! Uninstall removes every VM the same way ([`uninstall`]), leaving the
//! settings as they are.
mod real;
pub mod uninstall;

use super::{
    StationHold,
    add::settings_at,
    lifecycle::{
        Lifecycle, Provenance,
        barrier::Passed,
        grant,
        record::{Record, State},
    },
};
use atlas_core::{
    config::{
        desktops::{Chosen, Settings},
        write::{Change, Disposition},
    },
    desktop::DesktopId,
    error::{Code, Error, Result},
    files::Token,
};
use serde_json::{Value, json};
use std::{path::Path, time::Duration};

/// How often the removal is asked again while it is under way.
const AGAIN: Duration = Duration::from_secs(2);
/// How many times it is asked before the command stops and says to ask
/// again: its runner stopping the VM and launchd letting its jobs go.
const TRIES: u32 = 90;

/// What the removal asks of the system, given apart so tests stand in.
trait Means {
    /// Ask the person to allow `operation` through Atlas.app, and use the
    /// grant they gave: the change it is made as.
    fn grant(&self, operation: &grant::Operation) -> Result<Token>;
    /// This Atlas, for a record made by the removal itself.
    fn provenance(&self) -> Result<Provenance>;
    /// The VM `vm`'s station-operation lock, past the barrier `passed`;
    /// none while another holds it.
    fn hold<'a>(&self, vm: &DesktopId, passed: &'a impl Passed) -> Result<Option<StationHold<'a>>>;
    /// Remove the VM `hold` holds, as the removal `change`, or go on with
    /// it: what it came to.
    fn remove(&self, hold: &StationHold<'_>, change: &Token) -> Result<Value>;
    /// Let `how_long` go by.
    fn wait(&self, how_long: Duration);
}

/// Which VM the person asked to remove.
#[derive(Clone, Copy, Debug)]
pub enum Which<'a> {
    /// The desktop of this name.
    Name(&'a str),
    /// The VM of this identifier, whatever its name now.
    Id(&'a DesktopId),
}

/// What the person asked to remove: a VM of the settings file `settings`.
#[derive(Clone, Copy, Debug)]
pub struct Asked<'a> {
    pub which: Which<'a>,
    pub settings: &'a Path,
}

/// The VM a removal is of, as the settings and the lifecycle name it.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Target {
    id: DesktopId,
    /// Its name: as listed, recorded or reserved, else its identifier.
    name: String,
    /// Whether the person named it.
    named: bool,
    /// The name the settings list it as; none when they do not list it.
    listed: Option<String>,
}

/// Remove the VM `asked` names, or go on with its removal, once the person
/// allows it in Atlas.app, saying each step to `say` as a plain line.
///
/// # Errors
///
/// Refuses without the person's grant, a VM that is not there, a removal
/// the settings cannot take (the last desktop, counting every removal under
/// way), another settings file than the VM's, another removal recorded for
/// it and an uninstall under way, each before anything is touched; fails,
/// the reason kept in the VM's record, when the removal fails, and stops,
/// to be asked again, while it is still under way.
pub fn remove(asked: Asked<'_>, say: &dyn Fn(&str)) -> Result<Value> {
    remove_with(&Lifecycle::own()?, &real::Real, asked, say)
}

fn remove_with(
    lifecycle: &Lifecycle,
    means: &impl Means,
    asked: Asked<'_>,
    say: &dyn Fn(&str),
) -> Result<Value> {
    let settings = atlas_core::config::resolve_path(asked.settings)?;
    let target = found(lifecycle, &settings, asked.which)?;
    if let Some(record) = lifecycle.record(&target.id)? {
        same_settings(&record, &settings)?;
    }
    // Refused before the person is asked, and judged again once admitted.
    checked(lifecycle, &settings, &target)?;
    let operation = grant::Operation {
        kind: grant::Kind::Remove,
        vm: target.id.clone(),
        name: target.name.clone(),
        settings: settings.clone(),
        cancel: false,
    };
    say(&format!(
        "Asking you in Atlas.app to allow removing the VM \u{201c}{}\u{201d}.",
        target.name
    ));
    let granted = means.grant(&operation)?;
    let admitted = lifecycle.admit()?;
    let hold = means
        .hold(&target.id, &admitted)?
        .ok_or_else(|| Error::new(Code::Busy, "Another station operation holds this VM."))?;
    let (record, change) = {
        let _admitting = lifecycle.admitting_removal(&settings)?;
        // Looked at again: the settings or the VM may have changed while
        // the person was asked, and other removals may have begun.
        if found(lifecycle, &settings, asked.which)? != target {
            return Err(Error::new(
                Code::Config,
                format!(
                    "\u{201c}{}\u{201d} changed while you were asked; nothing was done. Ask again.",
                    target.name
                ),
            ));
        }
        checked(lifecycle, &settings, &target)?;
        begun(
            lifecycle,
            means,
            &target,
            &settings,
            granted,
            Disposition::Desktop,
        )?
    };
    Removing {
        lifecycle,
        means,
        hold: &hold,
        target: &target,
        record,
        disposition: Disposition::Desktop,
        say,
    }
    .run(&change)
}

/// The VM's record, made `removing` and kept with the removal's change
/// before anything is removed: the change kept already, else the one an
/// earlier removal of the VM is recorded under, else `granted`. A desktop
/// removed on its own must be removed through the settings it was added
/// to; uninstall removes it whichever they are.
fn begun(
    lifecycle: &Lifecycle,
    means: &impl Means,
    target: &Target,
    settings: &Path,
    granted: Token,
    disposition: Disposition,
) -> Result<(Record, Token)> {
    let mut record = match lifecycle.record(&target.id)? {
        Some(record) => {
            if disposition == Disposition::Desktop {
                same_settings(&record, settings)?;
            }
            record
        }
        None => Record {
            id: target.id.clone(),
            name: target.name.clone(),
            named: target.named,
            settings: settings.to_owned(),
            state: State::Removing,
            steps: Vec::new(),
            operations: Vec::new(),
            removal: None,
            kept: Vec::new(),
            provenance: means.provenance()?,
            failure: None,
            starts: Vec::new(),
            refused_start: None,
        },
    };
    let recorded = lifecycle
        .recorded_removal(&target.id)?
        .map(|(change, _)| change);
    let change = match (record.removal.clone(), recorded) {
        (Some(kept), Some(recorded)) if kept != recorded => {
            return Err(Error::new(
                Code::State,
                format!(
                    "Another removal of the VM \u{201c}{}\u{201d} is recorded than the one Atlas \
                     kept for it; nothing was done.",
                    target.name
                ),
            ));
        }
        (Some(kept), _) => kept,
        // One begun by the hidden repair command is gone on with.
        (None, Some(recorded)) => recorded,
        (None, None) => granted,
    };
    record.state = State::Removing;
    record.removal = Some(change.clone());
    lifecycle.keep_record(&record)?;
    Ok((record, change))
}

/// The VM `which` names in the settings at `path`: by name, its settings
/// entry, else its name's reservation, else a lifecycle record of that
/// name; by identifier, whichever of those holds it.
fn found(lifecycle: &Lifecycle, path: &Path, which: Which<'_>) -> Result<Target> {
    let current = settings_at(path)?;
    let listed_of = |id: &DesktopId| {
        current
            .iter()
            .flat_map(Settings::desktops)
            .find(|listed| matches!(&listed.desktop, Chosen::Vm(config) if config.id == *id))
            .map(|listed| listed.name.clone())
    };
    let kept = lifecycle.kept_any()?;
    match which {
        Which::Name(name) => {
            if let Some(listed) = current
                .iter()
                .flat_map(Settings::desktops)
                .find(|listed| listed.name == name)
            {
                return match &listed.desktop {
                    Chosen::Vm(config) => Ok(Target {
                        id: config.id.clone(),
                        name: name.to_owned(),
                        named: true,
                        listed: Some(name.to_owned()),
                    }),
                    Chosen::User(_) => Err(Error::new(
                        Code::Config,
                        format!("\u{201c}{name}\u{201d} is the Second Login, not a VM."),
                    )
                    .with(
                        "next",
                        json!(
                            "atlas desktop remove removes VMs; atlas uninstall removes the Second \
                             Login."
                        ),
                    )),
                };
            }
            let found = if kept {
                match lifecycle.reserved(name)? {
                    Some(reserved) => Some(reserved),
                    None => lifecycle
                        .records()?
                        .into_iter()
                        .find(|record| record.name == name)
                        .map(|record| (record.id, record.named)),
                }
            } else {
                None
            };
            let Some((id, named)) = found else {
                return Err(missing(format!(
                    "There is no VM named \u{201c}{name}\u{201d}."
                )));
            };
            Ok(Target {
                listed: listed_of(&id),
                id,
                name: name.to_owned(),
                named,
            })
        }
        Which::Id(id) => {
            let record = if kept { lifecycle.record(id)? } else { None };
            let reserved = if kept {
                lifecycle.reserved_by(id)?
            } else {
                None
            };
            let listed = listed_of(id);
            let name = listed
                .clone()
                .or_else(|| record.as_ref().map(|record| record.name.clone()))
                .or(reserved);
            let recorded = lifecycle.recorded_removal(id)?.is_some();
            let Some(name) = name.or_else(|| recorded.then(|| id.as_str().to_owned())) else {
                return Err(missing(format!(
                    "There is no VM with the id {}.",
                    id.as_str()
                )));
            };
            Ok(Target {
                id: id.clone(),
                named: record.as_ref().is_none_or(|record| record.named),
                name,
                listed,
            })
        }
    }
}

fn missing(error: String) -> Error {
    Error::new(Code::Config, error).with("next", json!("See the desktops with atlas desktop list."))
}

/// Refuse a removal from another settings file than the one `record`'s VM
/// is added to.
fn same_settings(record: &Record, settings: &Path) -> Result<()> {
    if record.settings == settings {
        return Ok(());
    }
    Err(Error::new(
        Code::Config,
        format!(
            "The VM \u{201c}{}\u{201d} is added to the settings at {}; run it again with --config \
             {} to remove it.",
            record.name,
            record.settings.display(),
            record.settings.display()
        ),
    ))
}

/// The settings at `path` as the removal of `target` would leave them,
/// nothing written. Refuses the last desktop, counting every other VM whose
/// removal is under way as gone.
fn checked(lifecycle: &Lifecycle, path: &Path, target: &Target) -> Result<()> {
    if target.listed.is_none() || settings_at(path)?.is_none() {
        return Ok(());
    }
    removal(&target.id, going(lifecycle, path, &target.id)?).check(path)?;
    Ok(())
}

/// Every other VM the settings at `path` list whose removal is under way:
/// its record says `removing`, or a removal of it is recorded (cut short,
/// begun by the hidden repair command, or done with its entry still
/// listed). None of them is left by a removal, nor made the default.
fn going(lifecycle: &Lifecycle, path: &Path, id: &DesktopId) -> Result<Vec<DesktopId>> {
    let mut going = Vec::new();
    for listed in settings_at(path)?.iter().flat_map(Settings::desktops) {
        let Chosen::Vm(config) = &listed.desktop else {
            continue;
        };
        if config.id == *id {
            continue;
        }
        let removing = lifecycle
            .record(&config.id)?
            .is_some_and(|record| record.state == State::Removing);
        if removing || lifecycle.recorded_removal(&config.id)?.is_some() {
            going.push(config.id.clone());
        }
    }
    Ok(going)
}

/// The settings change that takes the VM `id` off the list, found by its
/// identifier, as a desktop removed on its own, never handing the default
/// to one of `going`.
fn removal(id: &DesktopId, going: Vec<DesktopId>) -> Change {
    Change::RemoveVm {
        id: id.clone(),
        disposition: Disposition::Desktop,
        going,
    }
}

/// A removal under way, under the VM's station-operation lock.
struct Removing<'a, M> {
    lifecycle: &'a Lifecycle,
    means: &'a M,
    hold: &'a StationHold<'a>,
    target: &'a Target,
    record: Record,
    /// What becomes of the settings.
    disposition: Disposition,
    say: &'a dyn Fn(&str),
}

impl<M: Means> Removing<'_, M> {
    fn say(&self, line: &str) {
        (self.say)(line);
    }

    /// The steps, a failure kept in the VM's record when it can be.
    fn run(mut self, change: &Token) -> Result<Value> {
        match self.steps(change) {
            Ok(done) => Ok(done),
            Err(error) => {
                self.record.failed(&error);
                // Its reason is kept when it can be; the failure is said either way.
                let _ = self.lifecycle.keep_record(&self.record);
                Err(error)
            }
        }
    }

    /// The command that goes on with this removal.
    fn again(&self) -> String {
        match self.disposition {
            Disposition::Desktop => format!(
                "atlas desktop remove {} again (or atlas desktop remove --id {})",
                self.target.name,
                self.target.id.as_str()
            ),
            Disposition::Uninstall => "atlas uninstall again".to_owned(),
        }
    }

    fn steps(&mut self, change: &Token) -> Result<Value> {
        let name = self.target.name.clone();
        self.say(&format!("Removing the VM \u{201c}{name}\u{201d}."));
        let done = self.removed(change)?;
        let path = self.record.settings.clone();
        // Uninstall leaves the settings as they are until its shared
        // cleanup decides what becomes of the whole file.
        let default = match self.disposition {
            Disposition::Desktop => self.unlisted(&path)?,
            Disposition::Uninstall => None,
        };
        // A path kept stays named while it may be there; only one the
        // person moved or deleted since no longer holds the record.
        let mut kept = Vec::new();
        for path in done["kept"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            match std::fs::symlink_metadata(path) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                _ => kept.push(path.to_owned()),
            }
        }
        let left = done["left"].as_array().cloned().unwrap_or_default();
        if kept.is_empty() && left.is_empty() {
            if let Some(reserved) = self.lifecycle.reserved_by(&self.target.id)? {
                self.lifecycle.release(&reserved, &self.target.id)?;
            }
            self.lifecycle.forget_record(&self.target.id)?;
        } else {
            // Its name stays taken, so this VM is found by it again.
            self.record.kept.clone_from(&kept);
            self.record.failure = None;
            self.lifecycle.keep_record(&self.record)?;
        }
        if let Some(default) = &default {
            self.say(&format!(
                "The default desktop is now \u{201c}{default}\u{201d}."
            ));
        }
        let again = self.again();
        let until = match self.disposition {
            Disposition::Desktop => format!("the name {name} stays taken"),
            Disposition::Uninstall => "nothing more of Atlas is removed".to_owned(),
        };
        let mut result = json!({
            "ok": true,
            "removed": {"name": name, "id": self.target.id, "kind": "vm"},
            "settings": atlas_core::config::tilde(&path),
            "default": default,
            "kept": kept,
            "left": left,
        });
        if !kept.is_empty() {
            result["keptWhy"] = json!(format!(
                "These were not the VM's own when its removal began, so Atlas left them as they \
                 are and never deletes them. Move them aside or delete them yourself, then run \
                 {again}; until then {until}."
            ));
        }
        if !left.is_empty() {
            result["leftNext"] = json!(format!(
                "Atlas could not delete these Keychain items without asking. In Keychain Access, \
                 search for each by its name, delete it, then run {again}."
            ));
        }
        Ok(result)
    }

    /// The VM's settings entry taken off the list at `path` by its
    /// identifier, whatever it is named now, counted with every removal
    /// under way, which is admitted meanwhile; then the VM confirmed no
    /// longer listed. The default it committed, when it moved it.
    fn unlisted(&self, path: &Path) -> Result<Option<String>> {
        if settings_at(path)?.is_none() {
            return Ok(None);
        }
        let default = {
            let _admitting = self.lifecycle.admitting_removal(path)?;
            let going = going(self.lifecycle, path, &self.target.id)?;
            removal(&self.target.id, going).applied(path)?.default
        };
        let listed = settings_at(path)?.iter().flat_map(Settings::desktops).any(
            |listed| matches!(&listed.desktop, Chosen::Vm(config) if config.id == self.target.id),
        );
        if listed {
            return Err(Error::new(
                Code::State,
                format!(
                    "The settings at {} still list the VM \u{201c}{}\u{201d}; its record is kept.",
                    path.display(),
                    self.target.name
                ),
            ));
        }
        Ok(default)
    }

    /// The removal asked, as the very same change, until it answers done,
    /// each new reason it is still under way said once.
    fn removed(&self, change: &Token) -> Result<Value> {
        let mut why = String::new();
        for at in 0..TRIES {
            if at > 0 {
                self.means.wait(AGAIN);
            }
            let reply = self.means.remove(self.hold, change)?;
            if reply["ok"] != true {
                return Err(Error::new(
                    Code::State,
                    reply["error"]
                        .as_str()
                        .unwrap_or("The VM's removal was refused.")
                        .to_owned(),
                ));
            }
            if reply["removal"] == "done" {
                return Ok(reply);
            }
            let now = reply["why"].as_str().unwrap_or("It is still under way.");
            if now != why {
                now.clone_into(&mut why);
                self.say(&why);
            }
        }
        Err(Error::new(
            Code::NotReady,
            format!(
                "The VM's removal is not done yet: {why} Run {} to go on.",
                self.again()
            ),
        ))
    }
}

#[cfg(test)]
#[path = "removing_tests.rs"]
mod tests;

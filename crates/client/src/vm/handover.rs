//! Moving each VM's session keeper onto the build an update installed.
//!
//! Every keeper launchd runs for this account is found by the settings its
//! own agent pins, whatever settings the update was given; one that cannot
//! be accounted for refuses the update ([`discover`]). Before the bundle is
//! replaced, each keeper's VM is held under its exclusion and its keeper
//! asked whether work runs there, as its guest reports it (jobs, changes in
//! flight, unresolved supervisors; what cannot be told counts as running):
//! with work running the update is refused unless the person allowed
//! disrupting it. Once the new build is in place, each keeper is handed over
//! under the desktop's exclusion, its work asked about again first: a
//! handover is recorded in the VM's station folder with a change ID of its
//! own, the keeper asked to hand over, and the record kept until the old
//! keeper is seen gone and the new one serving, so a handover cut short is
//! finished by the next keeper and by the next run of this step. Agents on
//! the VM stay stopped until the person resumes them: nothing here resumes
//! them.
//!
//! A previous bundle a keeper still runs from is never removed
//! ([`running_from`]).
use super::keeper;
use crate::{link::Keeper, station::Station};
use atlas_control::{Controller, Ownership, StopWhy};
use atlas_core::{
    config::desktops::{Chosen, Selected, Settings},
    desktop::{DesktopId, RecordKey},
    error::{Code, Error, Holder, Result},
    files::Token,
    launchd::Job,
    process::Instance,
};
use atlas_session::pairing::{Enrolment, StationRecords};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

/// The record of a handover under way, in the VM's station folder.
const RECORD: &str = "handover.json";
/// The longest record read.
const LONGEST: u64 = 4096;
/// How long taking a desktop's exclusion waits for a pending-state writer.
const MARKER_CHECK: Duration = Duration::from_millis(250);
/// How long the guest's report of its work is waited for.
const REPORT: Duration = Duration::from_secs(10);
/// How long a keeper has to answer its hello.
const HELLO: Duration = Duration::from_secs(2);
/// How long the keeper has to keep the Stop, say it and answer: the guest
/// waits up to ten seconds for work to end before it answers.
const HANDING: Duration = Duration::from_secs(30);
/// How long the old keeper has to be gone and the new one serving: launchd
/// may wait ten seconds before it starts a job again.
const SERVING: Duration = Duration::from_secs(60);
/// How often that is looked at.
const LOOK: Duration = Duration::from_millis(250);
/// The first part of every keeper's label.
const LABELS: &str = "com.insanearts.atlas.station.";
const NEXT: &str = "Wait for the work to finish, or add --allow-disrupt to stop agents there for \
                    the update; they stay stopped until you resume them.";

/// A handover under way: its change, and the keeper handing over.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Record {
    change: Token,
    from: Instance,
}

impl Record {
    /// The handover kept in the station folder `folder`, if any.
    ///
    /// # Errors
    ///
    /// Fails when one is there but cannot be read: never taken for none.
    pub(crate) fn kept(folder: &Path) -> Result<Option<Self>> {
        let path = folder.join(RECORD);
        if std::fs::symlink_metadata(&path)
            .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound)
        {
            return Ok(None);
        }
        let damaged = || {
            Error::new(
                Code::State,
                "The record of a VM's session keeper being handed over cannot be read.",
            )
        };
        let bytes = atlas_core::files::read_own(&path, LONGEST).ok_or_else(damaged)?;
        serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|_| damaged())
    }

    /// A handover of `from` for the change `change`.
    #[cfg(test)]
    pub(super) const fn new(change: Token, from: Instance) -> Self {
        Self { change, from }
    }

    pub(super) fn keep(&self, folder: &Path) -> Result<()> {
        atlas_core::files::write_atomic(&folder.join(RECORD), &serde_json::to_vec(self)?, 0o600)
    }

    fn forget(folder: &Path) -> Result<()> {
        let path = folder.join(RECORD);
        match std::fs::remove_file(&path) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
                Err(Error::writing(&path, error))
            }
            _ => Ok(()),
        }
    }
}

/// What the person is told while an update is still handing `name`'s VM
/// `vm` over to the new build: a handover kept in its station folder, or one
/// that cannot be read. None when none is kept. Nothing finishes it on its
/// own but the keeper's start and the next update.
#[must_use]
pub fn unfinished(name: &str, vm: &DesktopId) -> Option<String> {
    unfinished_in(&Station::own().ok()?.path_of(vm), name)
}

pub(super) fn unfinished_in(folder: &Path, name: &str) -> Option<String> {
    (!matches!(Record::kept(folder), Ok(None))).then(|| {
        format!(
            "An Atlas update is still handing {name}'s session to the new build; run atlas \
             update again."
        )
    })
}

/// What the station asks of a VM's keeper and the system around it, given
/// apart so tests stand in. Each may wait, so none runs on a runtime.
pub(crate) trait Keepers: Send + Sync {
    /// Whether launchd has `vm`'s keeper's job.
    fn loaded(&self, vm: &DesktopId) -> Result<bool>;
    /// Whether that job runs a process now; one that cannot be told runs.
    fn runs(&self, vm: &DesktopId) -> bool;
    /// Every line `vm`'s keeper answers `request` with, within `deadline`,
    /// once it is trusted.
    fn ask(&self, vm: &DesktopId, request: &Value, deadline: Duration) -> Result<Vec<Value>>;
    /// Whether `vm` is paired.
    fn paired(&self, vm: &DesktopId) -> Result<bool>;
    /// Whether the process `instance` has ended.
    fn ended(&self, instance: &Instance) -> bool;
    /// Whether Atlas VM confirms that `vm`'s power is stopped now: nothing
    /// runs in it then. No answer, or one not confirmed, is no.
    fn stopped(&self, vm: &DesktopId) -> bool;
}

/// The keepers as launchd runs them for this account, each VM by the
/// settings that name it.
struct Launchd {
    selected: Vec<(DesktopId, Selected)>,
}

impl Launchd {
    fn of(found: &[Found]) -> Self {
        Self {
            selected: found
                .iter()
                .map(|found| (found.vm.clone(), found.selected.clone()))
                .collect(),
        }
    }

    fn job(vm: &DesktopId) -> Result<atlas_core::launchd::Job> {
        let domain = format!("gui/{}", nix::unistd::getuid().as_raw());
        atlas_core::launchd::job(&domain, &keeper::label(vm))
    }
}

impl Keepers for Launchd {
    fn loaded(&self, vm: &DesktopId) -> Result<bool> {
        Ok(matches!(
            Self::job(vm)?,
            atlas_core::launchd::Job::Loaded(_)
        ))
    }

    fn runs(&self, vm: &DesktopId) -> bool {
        !matches!(
            Self::job(vm),
            Ok(atlas_core::launchd::Job::Loaded(
                atlas_core::launchd::Loaded { pid: None, .. }
            ))
        )
    }

    fn ask(&self, vm: &DesktopId, request: &Value, deadline: Duration) -> Result<Vec<Value>> {
        let (_, selected) = self
            .selected
            .iter()
            .find(|(id, _)| id == vm)
            .ok_or_else(|| Error::new(Code::ConfigInvalid, "No settings name that VM."))?;
        let keeper = Keeper::at(
            Arc::new(selected.clone()),
            vm.clone(),
            keeper::socket_of(vm)?,
            deadline,
            Arc::new(crate::link::Launchd),
        );
        keeper.exchange(request, deadline)
    }

    fn paired(&self, vm: &DesktopId) -> Result<bool> {
        let record = atlas_session::pairing_file::PairingFile::at(Station::own()?.path_of(vm));
        Ok(matches!(record.read()?, Some(Enrolment::Paired(_))))
    }

    fn ended(&self, instance: &Instance) -> bool {
        instance.ended()
    }

    fn stopped(&self, vm: &DesktopId) -> bool {
        let Ok(vms) = super::Vms::own() else {
            return false;
        };
        // Asked under the station-operation lock, as every other ask of it.
        let Ok(Some(_operation)) = super::try_lock(&vms.station_lock(vm)) else {
            return false;
        };
        super::control::ask(&vms.folder(vm).join("control"), &json!({"op": "state"}))
            .is_ok_and(|state| powered_off(&state))
    }
}

/// One VM desktop with a keeper, as the station knows it.
pub(crate) struct Desk {
    name: String,
    vm: DesktopId,
    /// The desktop's exclusion.
    controller: Controller,
    /// Its private station folder.
    folder: PathBuf,
}

impl Desk {
    fn own(name: String, vm: DesktopId) -> Result<Self> {
        let controller =
            Controller::new(&atlas_core::desktop::Desktop::from(atlas_core::vm::Vm {
                id: vm.clone(),
            }))?;
        let folder = Station::own()?.folder(&vm)?;
        Ok(Self {
            name,
            vm,
            controller,
            folder,
        })
    }

    #[cfg(test)]
    pub(crate) fn at(name: &str, vm: DesktopId, controller: Controller, folder: PathBuf) -> Self {
        Self {
            name: name.to_owned(),
            vm,
            controller,
            folder,
        }
    }

    /// This desktop's exclusion, for a service change.
    fn exclusion(&self) -> Result<Ownership> {
        self.controller
            .acquire_for_service_change(false, MARKER_CHECK)
    }
}

/// A keeper launchd has for this account, accounted for by the settings
/// its agent pins: whatever settings an update was given, every keeper is
/// one it hands over.
#[derive(Clone, Debug)]
pub struct Found {
    vm: DesktopId,
    /// The settings file its agent pins.
    settings: PathBuf,
    /// The VM as those settings name it.
    selected: Selected,
}

impl Found {
    /// The VM's record key, `vm-<id>`.
    #[must_use]
    pub fn key(&self) -> String {
        RecordKey::vm(&self.vm).as_str().to_owned()
    }

    /// The settings file its agent pins.
    #[must_use]
    pub fn settings(&self) -> &Path {
        &self.settings
    }
}

/// What launchd has of this account's jobs, given apart so tests stand in.
trait Jobs {
    /// The label of every keeper's job.
    fn labels(&self) -> Result<Vec<String>>;
    /// The job `label`.
    fn job(&self, label: &str) -> Result<Job>;
}

/// launchd as it is.
struct Listed;

impl Jobs for Listed {
    fn labels(&self) -> Result<Vec<String>> {
        atlas_core::launchd::labels(&domain(), LABELS)
    }

    fn job(&self, label: &str) -> Result<Job> {
        atlas_core::launchd::job(&domain(), label)
    }
}

fn domain() -> String {
    format!("gui/{}", nix::unistd::getuid().as_raw())
}

/// Every keeper launchd has for this account, each by the settings its
/// agent pins.
///
/// # Errors
///
/// Refuses, naming them, keepers that cannot be accounted for: launchd's
/// jobs that cannot be listed, a keeper whose job does not say what it
/// runs, or whose settings cannot be read or list no such VM. An update
/// could neither check nor hand over such a keeper.
pub fn discover() -> Result<Vec<Found>> {
    discover_with(&Listed)
}

fn discover_with(jobs: &dyn Jobs) -> Result<Vec<Found>> {
    let labels = jobs.labels().map_err(|error| {
        unaccounted(&["which session keepers launchd runs cannot be told".to_owned()])
            .with_source(error)
    })?;
    let mut found = Vec::new();
    let mut unknown = Vec::new();
    for label in labels {
        match account(jobs, &label) {
            Ok(Some(keeper)) => found.push(keeper),
            Ok(None) => {}
            Err(why) => unknown.push(format!("{label} ({why})")),
        }
    }
    if unknown.is_empty() {
        Ok(found)
    } else {
        Err(unaccounted(&unknown))
    }
}

/// The keeper `label` names, by the settings its agent pins; none when
/// launchd no longer has it, and why it cannot be accounted for otherwise.
fn account(jobs: &dyn Jobs, label: &str) -> std::result::Result<Option<Found>, String> {
    let vm = label
        .strip_prefix(LABELS)
        .and_then(DesktopId::parse)
        .ok_or("its label names no VM")?;
    let loaded = match jobs.job(label) {
        Ok(Job::Loaded(loaded)) => loaded,
        Ok(Job::Absent) => return Ok(None),
        Err(error) => return Err(format!("launchd cannot say what it runs: {}", error.error)),
    };
    let (key, settings) = loaded
        .arguments
        .as_deref()
        .and_then(pinned)
        .ok_or("its agent runs no keeper")?;
    if key != RecordKey::vm(&vm).as_str() {
        return Err("its agent keeps another VM".to_owned());
    }
    let read = Settings::load(&settings).map_err(|error| {
        format!(
            "its settings, {}, cannot be read: {}",
            settings.display(),
            error.error
        )
    })?;
    let listed = read
        .desktops()
        .iter()
        .find(|listed| matches!(&listed.desktop, Chosen::Vm(config) if config.id == vm))
        .ok_or_else(|| format!("its settings, {}, list no such VM", settings.display()))?;
    Ok(Some(Found {
        selected: read.selected(listed),
        vm,
        settings,
    }))
}

/// The record key and settings a keeper's `arguments` pin, as its agent
/// writes them: the program, then `station keep --key <key> --settings
/// <path>`, nothing else.
fn pinned(arguments: &[String]) -> Option<(&str, PathBuf)> {
    match arguments {
        [_, station, keep, key_flag, key, settings_flag, settings]
            if station == "station"
                && keep == "keep"
                && key_flag == "--key"
                && settings_flag == "--settings" =>
        {
            Some((key.as_str(), PathBuf::from(settings)))
        }
        _ => None,
    }
}

/// The refusal of an update while keepers it cannot account for run, each
/// named with why.
fn unaccounted(keepers: &[String]) -> Error {
    Error::new(
        Code::Update,
        format!(
            "A VM's session keeper runs that Atlas cannot account for: {}. Atlas was not \
             updated.",
            keepers.join(", ")
        ),
    )
    .with(
        "next",
        json!(
            "Restore the settings that keeper's agent names, or remove that VM, then run atlas \
             update again."
        ),
    )
}

/// What runs on `desk`'s VM, as its guest reports it through its keeper;
/// none when nothing does. What cannot be told counts as running. A VM never
/// paired has had no session for work to start through, and one whose guest
/// cannot report, its keeper not live, runs nothing once Atlas VM confirms
/// its power is stopped.
fn running(keepers: &dyn Keepers, desk: &Desk) -> Option<String> {
    match keepers.paired(&desk.vm) {
        Ok(false) => return None,
        Ok(true) => {}
        Err(_) => return Some("whether it is paired cannot be told".to_owned()),
    }
    let read = json!({"op": "read", "request": {"op": "hello"},
        "deadline_ms": u64::try_from(REPORT.as_millis()).unwrap_or(u64::MAX)});
    let hello = keepers
        .ask(&desk.vm, &read, REPORT + HELLO)
        .ok()
        .and_then(|mut lines| lines.pop())
        .filter(|hello| hello["ok"] == true);
    if hello.is_none() && keepers.stopped(&desk.vm) {
        return None;
    }
    work_of(hello.as_ref())
}

/// Whether Atlas VM's `state` reply confirms the VM's power is stopped.
fn powered_off(state: &Value) -> bool {
    state["ok"] == true && state["confirmed"] == true && state["power"] == "stopped"
}

/// What the guest's `hello` says runs there; none only when it says nothing
/// does.
fn work_of(hello: Option<&Value>) -> Option<String> {
    let Some(hello) = hello.filter(|hello| hello["ok"] == true) else {
        return Some("what runs there cannot be told".to_owned());
    };
    let work = &hello["work"];
    if work["state"] == "none" {
        return None;
    }
    let parts: Vec<String> = [
        ("jobs", "background job", "background jobs"),
        ("changes", "change in flight", "changes in flight"),
        (
            "supervisors",
            "unresolved supervisor",
            "unresolved supervisors",
        ),
    ]
    .into_iter()
    .filter_map(|(key, one, many)| match work[key].as_u64() {
        Some(0) => None,
        Some(1) => Some(format!("1 {one}")),
        Some(count) => Some(format!("{count} {many}")),
        None => Some(format!("{many} unknown")),
    })
    .collect();
    Some(if parts.is_empty() {
        "work it cannot tell".to_owned()
    } else {
        parts.join(", ")
    })
}

/// The refusal of a change while work runs on the VMs `running` names, each
/// with what runs there, and what was not done, `undone`.
fn busy(running: &[(String, String)], undone: &str) -> Error {
    let named: Vec<String> = running
        .iter()
        .map(|(name, what)| format!("{name} ({what})"))
        .collect();
    let desktops = if running.len() == 1 {
        "VM desktop"
    } else {
        "VM desktops"
    };
    Error::new(
        Code::Busy,
        format!("Work runs on {desktops} {}. {undone}", named.join(", ")),
    )
    .held_by(Holder::Agent)
    .with("next", json!(NEXT))
    .with(
        "running",
        json!(
            running
                .iter()
                .map(|(name, what)| json!({"desktop": name, "running": what}))
                .collect::<Vec<_>>()
        ),
    )
}

/// The VM desktops with a keeper held under their exclusion for an update,
/// from before its bundle is replaced.
pub struct Held {
    keepers: Box<dyn Keepers>,
    desks: Vec<Desk>,
    _exclusions: Vec<Ownership>,
    /// The uninstall barrier, passed before any desktop's exclusion and
    /// held as long as they are; none when there is no VM to hold.
    _admitted: Option<super::lifecycle::barrier::Admitted>,
}

impl std::fmt::Debug for Held {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Held")
            .field("desks", &self.desks.len())
            .finish_non_exhaustive()
    }
}

/// Hold the VM of every keeper launchd has for this account under its
/// exclusion for an update, whatever settings the update was given,
/// refusing it, unless `allow_disrupt`, while work runs on any of them.
///
/// # Errors
///
/// Refuses keepers that cannot be accounted for ([`discover`]), an
/// uninstall's VM phase under way, a desktop's exclusion held elsewhere,
/// and work that runs or cannot be told, naming the desktops and what runs.
pub fn hold_for_update(allow_disrupt: bool) -> Result<Held> {
    let found = discover()?;
    let desks = found
        .iter()
        .map(|found| Desk::own(found.selected.name.clone(), found.vm.clone()))
        .collect::<Result<Vec<_>>>()?;
    hold(
        &super::lifecycle::Lifecycle::own()?,
        Box::new(Launchd::of(&found)),
        desks,
        allow_disrupt,
    )
}

fn hold(
    lifecycle: &super::lifecycle::Lifecycle,
    keepers: Box<dyn Keepers>,
    desks: Vec<Desk>,
    allow_disrupt: bool,
) -> Result<Held> {
    // First in the station's order: no keeper is handed over while an
    // uninstall's VM phase is under way. An update with no VM to hold
    // passes no barrier, as it touches none.
    let admitted = if desks.is_empty() {
        None
    } else {
        Some(lifecycle.admit()?)
    };
    let exclusions = desks
        .iter()
        .map(Desk::exclusion)
        .collect::<Result<Vec<_>>>()?;
    let held = Held {
        keepers,
        desks,
        _exclusions: exclusions,
        _admitted: admitted,
    };
    held.idle(allow_disrupt)?;
    Ok(held)
}

impl Held {
    /// Refuse, unless `allow_disrupt`, while work runs on any desktop held.
    ///
    /// # Errors
    ///
    /// Refuses while work runs or cannot be told, naming the desktops and
    /// what runs.
    pub fn idle(&self, allow_disrupt: bool) -> Result<()> {
        if allow_disrupt {
            return Ok(());
        }
        let running: Vec<(String, String)> = self
            .desks
            .iter()
            .filter_map(|desk| {
                running(self.keepers.as_ref(), desk).map(|what| (desk.name.clone(), what))
            })
            .collect();
        if running.is_empty() {
            Ok(())
        } else {
            Err(busy(&running, "Atlas was not updated."))
        }
    }
}

/// Hand the keeper of the VM keyed `key` in the settings at `settings` over
/// to the keeper launchd starts from the build now installed, as an update
/// does, refusing unless `allow_disrupt` while work runs there.
///
/// # Errors
///
/// Refuses a VM the settings do not list, an uninstall's VM phase under
/// way, its exclusion held elsewhere, and work running; fails when its keeper does not hand over, or the new one
/// does not serve in time. The handover stays recorded then, for the next
/// keeper and the next run.
pub fn hand_over(key: &str, settings: &Path, allow_disrupt: bool) -> Result<Value> {
    let vm = keeper::listed(key, settings)?;
    let read = Settings::load(settings)?;
    let listed = read
        .desktops()
        .iter()
        .find(|listed| matches!(&listed.desktop, Chosen::Vm(config) if config.id == vm))
        .ok_or_else(|| Error::new(Code::ConfigInvalid, "The settings list no such VM."))?;
    let found = Found {
        selected: read.selected(listed),
        vm: vm.clone(),
        settings: settings.to_owned(),
    };
    let desk = Desk::own(listed.name.clone(), vm)?;
    let admitted = super::lifecycle::Lifecycle::own()?.admit()?;
    handed(
        &Launchd::of(&[found]),
        &desk,
        &admitted,
        allow_disrupt,
        SERVING,
    )
}

/// [`hand_over`] of `desk` through `keepers`, past the uninstall barrier
/// `_passed` holds, the new keeper given `serving` to be seen.
pub(crate) fn handed(
    keepers: &dyn Keepers,
    desk: &Desk,
    _passed: &impl super::lifecycle::barrier::Passed,
    allow_disrupt: bool,
    serving: Duration,
) -> Result<Value> {
    let _exclusion = desk.exclusion()?;
    if !keepers.loaded(&desk.vm)? {
        return Ok(json!({"ok": true, "desktop": desk.name, "handed": false,
            "why": "No session keeper runs for it."}));
    }
    let kept = Record::kept(&desk.folder)?;
    // Cut short after the old keeper ended: finished once the new one serves.
    if let Some(record) = &kept
        && keepers.ended(&record.from)
    {
        let to = serves_after(keepers, desk, &record.from, serving)?;
        Record::forget(&desk.folder)?;
        return Ok(finished(desk, record, to, None));
    }
    // Work is asked about before anything is done to the VM, a handover
    // recorded earlier included: work may have begun since. What cannot be
    // told, a keeper not running included, counts as running unless Atlas VM
    // confirms the VM is powered off.
    if !allow_disrupt && let Some(what) = running(keepers, desk) {
        return Err(busy(
            &[(desk.name.clone(), what)],
            "Its session keeper was not handed over.",
        ));
    }
    // No keeper runs to hand over: launchd starts the new build's own in its
    // place. The update's Stop is kept here, as the keeper would keep it.
    if kept.is_none() && !keepers.runs(&desk.vm) {
        let noted = desk.controller.note_stop_unless_noted(StopWhy::Update)?;
        return Ok(
            json!({"ok": true, "desktop": desk.name, "handed": false, "noted": noted,
            "why": "Its session keeper was not running; launchd starts the new build's in its \
                    place."}),
        );
    }
    let from = serving_instance(keepers, desk)?;
    let record = Record {
        change: kept.map_or_else(Token::new, |record| record.change),
        from,
    };
    record.keep(&desk.folder)?;
    let asked = json!({"op": "handover", "change": record.change,
        "deadline_ms": u64::try_from(HANDING.as_millis()).unwrap_or(u64::MAX)});
    // An answer lost with a keeper ended midway is no refusal: the next
    // keeper finishes the handover.
    let answer = keepers
        .ask(&desk.vm, &asked, HANDING + HELLO)
        .ok()
        .and_then(|mut lines| lines.pop());
    if let Some(answer) = &answer
        && answer["ok"] != true
    {
        return Err(Error::refused(
            Code::Update,
            answer,
            "The VM's session keeper did not hand over.",
        ));
    }
    let to = serves_after(keepers, desk, &record.from, serving)?;
    Record::forget(&desk.folder)?;
    Ok(finished(desk, &record, to, answer.as_ref()))
}

/// The keeper serving `desk`'s VM now, by its hello.
fn serving_instance(keepers: &dyn Keepers, desk: &Desk) -> Result<Instance> {
    let hello = keepers
        .ask(&desk.vm, &json!({"op": "hello"}), HELLO)?
        .pop()
        .unwrap_or_default();
    serde_json::from_value(hello["instance"].clone()).map_err(|_| {
        Error::new(
            Code::Update,
            format!(
                "{}'s session keeper does not say which keeper it is, so it cannot be handed \
                 over.",
                desk.name
            ),
        )
    })
}

/// The keeper that serves `desk`'s VM once `from` is gone, waited for within
/// `serving`.
fn serves_after(
    keepers: &dyn Keepers,
    desk: &Desk,
    from: &Instance,
    serving: Duration,
) -> Result<Instance> {
    let until = Instant::now() + serving;
    loop {
        if keepers.ended(from)
            && let Ok(now) = serving_instance(keepers, desk)
            && now != *from
        {
            return Ok(now);
        }
        if Instant::now() >= until {
            return Err(Error::new(
                Code::Update,
                format!(
                    "{}'s new session keeper did not start in time. Run the update again to \
                     finish handing it over; agents there stay stopped.",
                    desk.name
                ),
            ));
        }
        std::thread::sleep(LOOK);
    }
}

fn finished(desk: &Desk, record: &Record, to: Instance, answer: Option<&Value>) -> Value {
    let mut finished = json!({
        "ok": true,
        "desktop": desk.name,
        "handed": true,
        "change": record.change,
        "from": record.from.pid,
        "to": to.pid,
    });
    if let Some(answer) = answer {
        finished["said"] = answer["said"].clone();
        finished["outcome"] = answer["outcome"].clone();
    }
    finished
}

/// What the station asks of the system to tell which keepers run from a
/// bundle, given apart so tests stand in.
trait Processes {
    /// The PIDs of every keeper launchd runs for this account now.
    fn keepers(&self) -> Result<Vec<u32>>;
    /// The file the process `pid` runs from, as the kernel names its image
    /// now; none when it cannot be told.
    fn image(&self, pid: u32) -> Option<PathBuf>;
    /// Whether the process `pid` still runs.
    fn alive(&self, pid: u32) -> bool;
}

/// The system as it is.
struct System;

impl Processes for System {
    fn keepers(&self) -> Result<Vec<u32>> {
        let domain = format!("gui/{}", nix::unistd::getuid().as_raw());
        let mut pids = Vec::new();
        for label in atlas_core::launchd::labels(&domain, LABELS)? {
            if let atlas_core::launchd::Job::Loaded(loaded) =
                atlas_core::launchd::job(&domain, &label)?
            {
                pids.extend(loaded.pid);
            }
        }
        Ok(pids)
    }

    fn image(&self, pid: u32) -> Option<PathBuf> {
        atlas_core::app_bundle::signature::executable_of(pid)
    }

    fn alive(&self, pid: u32) -> bool {
        i32::try_from(pid).is_ok_and(|pid| {
            !matches!(
                nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid), None),
                Err(nix::errno::Errno::ESRCH)
            )
        })
    }
}

/// The processes of every session keeper launchd runs for this account now;
/// none known when that cannot be told.
#[must_use]
pub fn keeper_pids() -> Vec<u32> {
    System.keepers().unwrap_or_default()
}

/// Why the bundle `bundle` is kept: a VM's session keeper still runs from
/// it, or which keepers run cannot be told. None when none does.
#[must_use]
pub fn running_from(bundle: &Path) -> Option<String> {
    running_from_with(&System, bundle)
}

fn running_from_with(processes: &dyn Processes, bundle: &Path) -> Option<String> {
    let Ok(pids) = processes.keepers() else {
        return Some("Which VM session keepers run cannot be told.".to_owned());
    };
    pids.into_iter().find_map(|pid| match processes.image(pid) {
        Some(image) if !within(&image, bundle) => None,
        Some(_) => Some(format!(
            "A VM's session keeper, process {pid}, still runs from it."
        )),
        None if processes.alive(pid) => Some(format!(
            "What VM session keeper {pid} runs from cannot be told."
        )),
        None => None,
    })
}

/// Whether `path` is inside `bundle`, both resolved; one that cannot be
/// resolved is judged as it is written.
fn within(path: &Path, bundle: &Path) -> bool {
    let resolved = |path: &Path| std::fs::canonicalize(path).unwrap_or_else(|_| path.to_owned());
    resolved(path).starts_with(resolved(bundle))
}

#[cfg(test)]
#[path = "handover_tests.rs"]
mod tests;

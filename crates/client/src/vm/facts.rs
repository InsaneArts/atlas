//! What the app needs to know of a VM desktop, built in this one place for
//! `atlas desktop list` and a VM's status: how far its making got, the
//! steps of its add done, its disk, its macOS and whether a newer one is
//! out, its maintenance, its Atlas, whether its add still goes on, and what
//! the person can do with it now. Each is Atlas VM's, the guest's or the
//! station's own word: nothing is guessed, and what cannot be told is null,
//! never read as done or up to date.
use super::{
    lifecycle::{
        Lifecycle,
        record::{Record, State},
    },
    maintenance::Kept,
    runner::{Hello, Operation, Power, Release, Reply, Runner},
};
use crate::client::Client;
use atlas_core::{config::desktops::Chosen, desktop::DesktopId, error::Result, process};
use serde_json::{Value, json};
use std::{cmp::Ordering, time::Duration};

/// What Atlas VM says of a VM, as read for its facts: its declaration and
/// the newest macOS, each none when it was not answered, and its state or
/// why it could not be read.
#[derive(Debug)]
pub(crate) struct Word {
    pub(crate) hello: Option<Hello>,
    pub(crate) state: Result<Reply>,
    pub(crate) latest: Option<Release>,
}

impl Word {
    /// As `ask` answers each read: the newest macOS is asked only of an
    /// Atlas VM that declares it.
    pub(crate) fn asked(ask: impl Fn(&Operation) -> Result<Reply>) -> Self {
        let hello = ask(&Operation::Hello)
            .ok()
            .filter(|reply| reply.ok)
            .and_then(|reply| reply.hello);
        let state = ask(&Operation::State);
        let latest = hello
            .as_ref()
            .filter(|hello| hello.can(LATEST))
            .and_then(|_| ask(&Operation::Latest).ok())
            .filter(|reply| reply.ok)
            .and_then(|reply| reply.latest);
        Self {
            hello,
            state,
            latest,
        }
    }

    /// Whether Atlas VM declared `capability`.
    fn declares(&self, capability: &str) -> bool {
        self.hello
            .as_ref()
            .is_some_and(|hello| hello.can(capability))
    }

    /// Its state, served, once Atlas VM declared `capability` with it.
    fn state_with(&self, capability: &str) -> Option<&Reply> {
        self.served().filter(|_| self.declares(capability))
    }

    /// Its state, when it was served.
    fn served(&self) -> Option<&Reply> {
        self.state.as_ref().ok().filter(|state| state.ok)
    }
}

/// Atlas VM's word on how far making a VM got.
const PROGRESS: &str = "progress";
/// Atlas VM's word on how much of a VM's disk is used.
const DISK: &str = "disk";
/// Atlas VM's word on the newest macOS this Mac's framework supports.
const LATEST: &str = "latest-macos";

/// Everything a VM's facts are built from.
pub(crate) struct Seen<'a> {
    pub(crate) lifecycle: &'a Lifecycle,
    pub(crate) vm: &'a DesktopId,
    pub(crate) name: &'a str,
    /// Its lifecycle record, when it has one.
    pub(crate) record: Option<&'a Record>,
    pub(crate) word: &'a Word,
    /// The guest's macOS as it reported it, `{version, build}`; null when it
    /// did not.
    pub(crate) system: &'a Value,
    /// What its Atlas's update stands at, as a status says it.
    pub(crate) atlas: Value,
}

/// How long Atlas VM is given to answer each read of a status's facts.
const STATUS_READ: Duration = Duration::from_secs(2);

/// The facts of the VM `client` acts on, beside its status `status`: none
/// for Second Login, or while its lifecycle record cannot be read.
pub(crate) async fn of_status(client: &Client, status: &Value) -> Option<Value> {
    let Chosen::Vm(config) = &client.selected.desktop else {
        return None;
    };
    let lifecycle = client.lifecycle.clone()?;
    let (vm, name) = (config.id.clone(), client.selected.name.clone());
    let system = reported_system(status);
    let atlas = crate::client::guest_update(&name, status);
    tokio::task::spawn_blocking(move || {
        let record = lifecycle.record(&vm).ok()?;
        let runner = Runner::in_vms(&lifecycle.vms(), &vm);
        let word = Word::asked(|operation| runner.read_within(operation, STATUS_READ));
        Some(facts(&Seen {
            lifecycle: &lifecycle,
            vm: &vm,
            name: &name,
            record: record.as_ref(),
            word: &word,
            system: &system,
            atlas,
        }))
    })
    .await
    .ok()?
}

/// The guest's macOS as `status` says it: from the guest's own hello, else
/// from the report its keeper last read of it; null when neither says.
pub(crate) fn reported_system(status: &Value) -> Value {
    [
        &status["guest"]["system"],
        &status["keeper"]["report"]["system"],
    ]
    .into_iter()
    .find(|system| system.is_object())
    .cloned()
    .unwrap_or(Value::Null)
}

/// The facts of the VM `seen` names, as one object.
pub(crate) fn facts(seen: &Seen<'_>) -> Value {
    json!({
        "progress": progress(seen),
        "steps": seen.record.map_or(Value::Null, |record| json!(record.steps)),
        "disk": disk(seen.word),
        "macos": macos(seen.system, seen.word),
        "maintenance": Kept::of(seen.record.and_then(|record| record.maintenance.clone()))
            .said(seen.name),
        "atlas": seen.atlas,
        "adder": adder(seen.record),
        "can": can(seen),
    })
}

/// How far making the VM of an add under way got, as Atlas VM keeps it for
/// the make this add asked: none from an Atlas VM that does not declare it,
/// for another make's, or once the add is done.
fn progress(seen: &Seen<'_>) -> Value {
    let Some(record) = seen.record.filter(|record| record.state == State::Adding) else {
        return Value::Null;
    };
    let asked = record.last_sent("create").map(|sent| &sent.change);
    seen.word
        .state_with(PROGRESS)
        .and_then(|state| state.making.as_ref())
        .filter(|making| Some(&making.change) == asked)
        .map_or(Value::Null, |making| {
            json!({
                "phase": making.phase,
                "received": making.received,
                "total": making.total,
                "fraction": making.fraction,
            })
        })
}

/// How much of the VM's disk is used, and its size, each as Atlas VM says
/// it apart from the other.
fn disk(word: &Word) -> Value {
    word.state_with(DISK).and_then(|state| state.disk).map_or(
        Value::Null,
        |disk| json!({"used": disk.used, "size": disk.size}),
    )
}

/// The guest's macOS as it reported it, the newest Atlas VM says this Mac
/// supports, and whether that one is newer: only when it is strictly
/// greater, and null while either cannot be told.
fn macos(system: &Value, word: &Word) -> Value {
    let version = system["version"].as_str();
    let latest = word.latest.as_ref().filter(|_| word.declares(LATEST));
    let newer = version
        .zip(latest)
        .and_then(|(version, latest)| compare(&latest.version, version))
        .map(|order| order == Ordering::Greater);
    json!({
        "version": version,
        "build": system["build"].as_str(),
        "latest": latest,
        "newer": newer,
    })
}

/// How the macOS version `one` compares with `other`, by each number in
/// turn, a missing one read as zero; none when either is not a version.
fn compare(one: &str, other: &str) -> Option<Ordering> {
    let numbers = |version: &str| {
        version
            .split('.')
            .map(|part| {
                (!part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
                    .then(|| part.parse::<u64>().ok())
                    .flatten()
            })
            .collect::<Option<Vec<u64>>>()
            .filter(|numbers| (1..=4).contains(&numbers.len()))
    };
    let (mut one, mut other) = (numbers(one)?, numbers(other)?);
    let length = one.len().max(other.len());
    one.resize(length, 0);
    other.resize(length, 0);
    Some(one.cmp(&other))
}

/// Whether the process making the add under way still runs, as the
/// record keeps it: null when the add is done, or its process cannot be
/// told.
fn adder(record: Option<&Record>) -> Value {
    let Some(record) = record.filter(|record| record.state == State::Adding) else {
        return Value::Null;
    };
    let running = record.adder.and_then(|adder| match adder.state() {
        process::State::Running | process::State::Stopped => Some(true),
        process::State::Gone => Some(false),
        process::State::Unknown => None,
    });
    json!({"running": running})
}

/// What the person can do with the VM now: start it when it is seen
/// stopped, stop it when it is seen running or starting, open it for
/// maintenance, each only once it is added and while no removal, update
/// handover or maintenance holds it; end a maintenance kept; and remove it,
/// which goes on with one cut short.
fn can(seen: &Seen<'_>) -> Value {
    let mut can = Vec::new();
    let created = seen
        .record
        .is_some_and(|record| record.state == State::Created && record.removal.is_none());
    let maintained = seen
        .record
        .is_some_and(|record| record.maintenance.is_some());
    if created && !maintained && !held(seen) {
        let power = seen
            .word
            .served()
            .filter(|state| state.confirmed == Some(true))
            .and_then(|state| state.power);
        match power {
            Some(Power::Stopped) => can.push("start"),
            Some(Power::Running | Power::Starting) => can.push("stop"),
            _ => {}
        }
        if !super::keeper::update_unsettled(&station(seen)) {
            can.push("maintain");
        }
    }
    if created && maintained {
        can.push("end");
    }
    can.push("remove");
    json!(can)
}

/// Whether a removal recorded apart from its lifecycle record, or an
/// update's handover, holds the VM: one that cannot be read holds it.
fn held(seen: &Seen<'_>) -> bool {
    !matches!(seen.lifecycle.recorded_removal(seen.vm), Ok(None))
        || super::handover::unfinished_in(&station(seen), seen.name).is_some()
}

/// The station's folder for the VM.
fn station(seen: &Seen<'_>) -> std::path::PathBuf {
    crate::station::Station::under(seen.lifecycle.home()).path_of(seen.vm)
}

#[cfg(test)]
#[path = "facts_tests.rs"]
mod tests;

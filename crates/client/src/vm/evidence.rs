//! Whether every runner of a VM's folder, and every VM they started, is
//! gone: by Atlas VM's own rule (`RunRecord.machineGone`), over the runs it
//! kept in `Incarnation.json`, the current one and each earlier one not yet
//! seen gone. What cannot be told is never gone.
use super::System;
use atlas_core::process::Instance;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// The framework's program that runs a VM.
const MACHINE: &str = "/com.apple.Virtualization.VirtualMachine";
/// The longest record read.
const LONGEST: u64 = 64 * 1024;

/// One run of a VM's runner as it kept itself: its own process and, once
/// it told one, the framework's process that ran its VM.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct Run {
    pub(super) runner: Instance,
    pub(super) machine: Option<Instance>,
}

/// A process as Atlas VM writes it.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Process {
    pid: i32,
    start_seconds: u64,
    start_microseconds: u64,
}

impl Process {
    fn instance(&self) -> Instance {
        Instance::new(self.pid, [self.start_seconds, self.start_microseconds])
    }
}

/// `Incarnation.json` as Atlas VM writes it.
#[derive(Deserialize)]
struct Record {
    run: Process,
    machine: Option<Process>,
    #[serde(default)]
    earlier: Vec<Record>,
}

impl Record {
    fn runs(&self, into: &mut Vec<Run>) {
        for earlier in &self.earlier {
            earlier.runs(into);
        }
        into.push(Run {
            runner: self.run.instance(),
            machine: self.machine.as_ref().map(Process::instance),
        });
    }
}

/// The runs kept at `file`; none when it is there but cannot be read
/// whole, which proves nothing; no runs when it was never written.
pub(super) fn runs(file: &Path) -> Option<Vec<Run>> {
    let bytes = match atlas_core::files::read_own(file, LONGEST) {
        Some(bytes) => bytes,
        None => {
            return std::fs::symlink_metadata(file)
                .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound)
                .then(Vec::new);
        }
    };
    let record: Record = serde_json::from_slice(&bytes).ok()?;
    let mut runs = Vec::new();
    record.runs(&mut runs);
    Some(runs)
}

/// Why one of `runs` is not seen gone; none once every runner has ended
/// and every VM they started is gone.
pub(super) fn not_gone(runs: &[Run], system: &impl System) -> Option<&'static str> {
    for run in runs {
        // A run kept as unreadable names no process of its own.
        if run.runner.pid > 0 && system.ended(&run.runner) != Some(true) {
            return Some("a runner of this VM still runs, or whether it does cannot be told");
        }
        let gone = match &run.machine {
            Some(machine) => system.ended(machine) == Some(true),
            // None kept: no VM process of the framework's started since
            // that run did, so none can be one it started.
            None => system.machines().is_some_and(|machines| {
                !machines
                    .iter()
                    .any(|machine| machine.start >= run.runner.start)
            }),
        };
        if !gone {
            return Some(
                "a VM this folder's runs started still runs, or whether it does cannot be told",
            );
        }
    }
    None
}

/// Every process of the framework's that runs a VM now, of this account;
/// none when one of its processes' programs cannot be read and its short
/// name could be that program's. As Atlas VM's own scan: a process whose
/// program was deleted since it started has no path to read, and is no VM
/// unless its name says it may be.
pub(super) fn machines() -> Option<Vec<Instance>> {
    let uid = nix::unistd::getuid().as_raw();
    let listed = atlas_core::process::of_user(uid).ok()?;
    let mut found = Vec::new();
    for info in listed {
        if info.zombie {
            continue;
        }
        let pid = u32::try_from(info.instance.pid).ok()?;
        let Some(program) = atlas_core::app_bundle::executable_of(pid) else {
            // Gone meanwhile is not one; nor is one named otherwise.
            if info.instance.ended() || !MACHINE.trim_start_matches('/').starts_with(&info.name) {
                continue;
            }
            return None;
        };
        if program.to_string_lossy().ends_with(MACHINE) {
            found.push(info.instance);
        }
    }
    Some(found)
}

//! Whether this Mac can make and run a new VM desktop now, as the setup
//! window's choice says it: macOS 27 or later, Apple silicon whose kernel
//! runs virtual machines, 70 GB free on the volume of this account's home,
//! and fewer than two of this account's VMs running. Each is this Mac's own
//! word, read now. A fact that cannot be read counts against the VM, except
//! the VMs running, which Atlas VM counts again before it makes one.
use super::runner::{Operation, Power, Runner};
use atlas_core::process;
use serde::Serialize;
use serde_json::{Value, json};
use std::{path::Path, time::Duration};

/// What a new VM needs that this Mac lacks: the first of them, in this
/// order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Lack {
    /// macOS 27 or later.
    Macos,
    /// Apple silicon whose kernel runs virtual machines.
    Chip,
    /// 70 GB free.
    Disk,
    /// Fewer than two VMs running.
    Running,
}

/// The oldest macOS release a VM is made on.
const MACOS: u64 = 27;
/// What making a VM takes free: about 28 GB once installed, and the
/// restore image's 26 GB to download, as Atlas VM checks it.
const FREE: u64 = 70_000_000_000;
/// The most macOS VMs Apple's framework runs at once.
const MOST_RUNNING: usize = 2;
/// How long each VM's Atlas VM is given to say its power.
const ASKING: Duration = Duration::from_secs(2);

/// What this Mac was seen to have; each none when it did not say.
#[derive(Clone, Debug, Default)]
struct Seen {
    /// Its macOS version, as 27.2.1.
    version: Option<String>,
    /// Whether it has Apple silicon.
    silicon: Option<bool>,
    /// Whether its kernel runs virtual machines.
    hypervisor: Option<bool>,
    /// The bytes free on the volume of this account's home.
    free: Option<u64>,
    /// How many of this account's VMs Atlas VM says start or run.
    running: usize,
}

/// Whether this Mac can make a new VM now, `home` being this account's
/// home: `{"available": true}`, else `{"available": false, "why": …}`
/// with what it lacks.
#[must_use]
pub fn now(home: &Path) -> Value {
    said(judged(&Seen {
        version: process::System::now().version,
        silicon: process::apple_silicon(),
        hypervisor: process::hypervisor(),
        free: free(home),
        running: running(),
    }))
}

/// `lack` as the choice reads it.
fn said(lack: Option<Lack>) -> Value {
    match lack {
        None => json!({"available": true}),
        Some(lack) => json!({"available": false, "why": lack}),
    }
}

/// What `seen` lacks for a new VM, the first in [`Lack`]'s order; none
/// when it has all.
fn judged(seen: &Seen) -> Option<Lack> {
    let major = seen
        .version
        .as_deref()
        .and_then(|version| version.split('.').next())
        .and_then(|major| major.parse::<u64>().ok());
    if major.is_none_or(|major| major < MACOS) {
        return Some(Lack::Macos);
    }
    if seen.silicon != Some(true) || seen.hypervisor != Some(true) {
        return Some(Lack::Chip);
    }
    if seen.free.is_none_or(|free| free < FREE) {
        return Some(Lack::Disk);
    }
    (seen.running >= MOST_RUNNING).then_some(Lack::Running)
}

/// The bytes free on the volume of `folder`; none when it cannot be read.
fn free(folder: &Path) -> Option<u64> {
    nix::sys::statvfs::statvfs(folder)
        .ok()
        .map(|stats| u64::from(stats.blocks_available()).saturating_mul(stats.fragment_size()))
}

/// How many of this account's VMs Atlas VM says start or run now. A VM
/// whose Atlas VM does not answer is not counted: Atlas VM counts its own
/// again before it makes or starts one, and refuses past the limit.
fn running() -> usize {
    super::kept()
        .unwrap_or_default()
        .iter()
        .filter(|vm| {
            Runner::of(vm)
                .and_then(|runner| runner.read_within(&Operation::State, ASKING))
                .is_ok_and(|reply| {
                    reply.ok && matches!(reply.power, Some(Power::Running | Power::Starting))
                })
        })
        .count()
}

#[cfg(test)]
#[path = "room_tests.rs"]
mod tests;

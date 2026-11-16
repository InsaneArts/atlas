//! A VM desktop as the station keeps it on this Mac: its folder, served by
//! Atlas VM under a launchd agent of this account's own, and its removal.
//! The station's own code, which outlives Atlas VM, and its trust in the
//! socket Atlas VM serves the VM's guest on.
mod agent;
mod control;
mod evidence;
pub mod keeper;
mod removal;
mod station;

pub use agent::install_agent;
pub use removal::remove;
pub use station::{Facts, Refused, StationSocket, Untrusted, facts, trusted};

use atlas_core::{
    desktop::DesktopId,
    error::{Code, Error, Result},
    files::{Lock, LockFile},
    launchd::Job,
    process::Instance,
};
use serde_json::Value;
use std::{
    fs::File,
    path::{Path, PathBuf},
    time::Duration,
};

/// The Keychain service Atlas VM keeps a VM's passwords under.
const PASSWORDS: &str = "com.insanearts.atlas.vm";
/// How long a lock that looks held is tried for before the work waits.
const PATIENCE: Duration = Duration::from_secs(2);

/// Where this account keeps its VMs, and whose they are.
#[derive(Clone, Debug)]
struct Vms {
    /// The folder every VM's folder is in.
    parent: PathBuf,
    /// The account's home.
    home: PathBuf,
    /// The account's user ID.
    uid: u32,
}

/// The VMs this account keeps on this disk, listed in settings or not, with
/// any whose removal is still under way.
///
/// # Errors
///
/// Fails when the account or its VMs folder cannot be read: VMs may be kept
/// there.
pub fn kept() -> Result<Vec<DesktopId>> {
    Vms::own()?.kept()
}

impl Vms {
    /// Every VM folder and pending removal here, by identifier.
    fn kept(&self) -> Result<Vec<DesktopId>> {
        let mut kept = Vec::new();
        for (folder, suffix) in [
            (self.parent.clone(), ""),
            (self.parent.join(".removals"), ".json"),
        ] {
            let entries = match std::fs::read_dir(&folder) {
                Ok(entries) => entries,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => {
                    return Err(Error::new(Code::State, "This Mac's VMs cannot be read.")
                        .with_source(error));
                }
            };
            for entry in entries {
                let entry = entry.map_err(|error| {
                    Error::new(Code::State, "This Mac's VMs cannot be read.").with_source(error)
                })?;
                let name = entry.file_name();
                let id = name
                    .to_str()
                    .and_then(|name| name.strip_suffix(suffix))
                    .and_then(DesktopId::parse);
                let Some(id) = id else { continue };
                // A finished removal's record is kept to answer it again.
                if !suffix.is_empty() && !removal::under_way(&entry.path())? {
                    continue;
                }
                if !kept.contains(&id) {
                    kept.push(id);
                }
            }
        }
        Ok(kept)
    }

    /// This account's.
    fn own() -> Result<Self> {
        let uid = nix::unistd::getuid();
        let user = nix::unistd::User::from_uid(uid)
            .ok()
            .flatten()
            .ok_or_else(|| Error::new(Code::State, "This account cannot be read."))?;
        Ok(Self::under(&user.dir, uid.as_raw()))
    }

    /// The VMs of the account `uid`, whose home is `home`.
    fn under(home: &Path, uid: u32) -> Self {
        Self {
            parent: home.join("Library/Application Support/Atlas/VMs"),
            home: home.to_owned(),
            uid,
        }
    }

    fn folder(&self, desktop: &DesktopId) -> PathBuf {
        self.parent.join(desktop.as_str())
    }

    fn locks(&self) -> PathBuf {
        self.parent.join(".locks")
    }

    /// The lock every run of the VM holds for its life.
    fn lifecycle(&self, desktop: &DesktopId) -> PathBuf {
        self.locks().join(format!("{}.lock", desktop.as_str()))
    }

    /// The lock the VM's session keeper holds for its life.
    fn keeper_lock(&self, desktop: &DesktopId) -> PathBuf {
        self.locks()
            .join(format!("{}.keeper.lock", desktop.as_str()))
    }

    /// The station's own exclusion of the desktop, until step 5's VM kind
    /// moves it to the controller.
    fn station_lock(&self, desktop: &DesktopId) -> PathBuf {
        self.locks()
            .join(format!("{}.station.lock", desktop.as_str()))
    }

    fn removals(&self) -> PathBuf {
        self.parent.join(".removals")
    }

    /// The record of the desktop's removal, and while it is there the
    /// barrier no runner or agent passes.
    fn removal(&self, desktop: &DesktopId) -> PathBuf {
        self.removals().join(format!("{}.json", desktop.as_str()))
    }

    fn label(desktop: &DesktopId) -> String {
        format!("com.insanearts.atlas.vm.{}", desktop.as_str())
    }

    /// The label of the VM's session keeper's own agent.
    fn keeper_label(desktop: &DesktopId) -> String {
        format!("com.insanearts.atlas.station.{}", desktop.as_str())
    }

    fn agents(&self) -> PathBuf {
        self.home.join("Library/LaunchAgents")
    }

    fn agent_plist(&self, desktop: &DesktopId) -> PathBuf {
        self.agents()
            .join(format!("{}.plist", Self::label(desktop)))
    }

    fn domain(&self) -> String {
        format!("gui/{}", self.uid)
    }
}

/// What the station asks of the system for a VM, given apart so tests
/// stand in.
trait System {
    /// The job `label` in `domain`, as launchd says now.
    fn job(&self, domain: &str, label: &str) -> Result<Job>;
    /// Run `launchctl` with `arguments`: whether it succeeded.
    fn launchctl(&self, arguments: &[&str]) -> Result<bool>;
    /// Ask the runner on the control socket `socket` the request `request`.
    fn ask(&self, socket: &Path, request: &Value) -> Result<Value>;
    /// Delete the Keychain password of `account`, never asking: whether
    /// there was one.
    fn delete_password(&self, account: &str) -> Result<bool>;
    /// Delete the VM's enrolment, never asking: whether there was one.
    fn delete_enrolment(&self, desktop: &DesktopId) -> Result<bool>;
    /// Whether `instance` has ended; none when that cannot be told.
    fn ended(&self, instance: &Instance) -> Option<bool>;
    /// Every process of the framework's that runs a VM now, of this
    /// account; none when that cannot be told whole.
    fn machines(&self) -> Option<Vec<Instance>>;
    /// The program Atlas VM is, in the Atlas.app the station runs from.
    fn program(&self) -> Result<PathBuf>;
}

/// The lock at `path`, its folder made private first, taken within
/// [`PATIENCE`]; none while another holds it.
fn try_lock(path: &Path) -> Result<Option<LockFile>> {
    if let Some(folder) = path.parent() {
        atlas_core::files::private_folder(folder, Code::State)?;
    }
    take_lock(path)
}

/// The lock at `path`, in a folder that is there, taken within
/// [`PATIENCE`]; none while another holds it.
fn take_lock(path: &Path) -> Result<Option<LockFile>> {
    let file = atlas_core::files::open_lock(path)?;
    match atlas_core::files::lock_within(&file, Lock::Exclusive, PATIENCE) {
        Ok(()) => Ok(Some(file)),
        Err(_) => Ok(None),
    }
}

/// Make what was renamed or removed in `folder` reach the disk.
fn sync(folder: &Path) -> Result<()> {
    File::open(folder)
        .and_then(|folder| folder.sync_all())
        .map_err(|error| Error::writing(folder, error))
}

/// The system as it is.
struct Real;

impl System for Real {
    fn job(&self, domain: &str, label: &str) -> Result<Job> {
        atlas_core::launchd::job(domain, label)
    }

    fn launchctl(&self, arguments: &[&str]) -> Result<bool> {
        atlas_core::launchd::launchctl(arguments)
    }

    fn ask(&self, socket: &Path, request: &Value) -> Result<Value> {
        control::ask(socket, request)
    }

    fn delete_password(&self, account: &str) -> Result<bool> {
        atlas_native::keychain::delete(atlas_native::keychain::Item {
            service: PASSWORDS,
            account,
        })
    }

    fn delete_enrolment(&self, desktop: &DesktopId) -> Result<bool> {
        use crate::station::enrolment::Enrolments;
        crate::station::enrolment::Keychain.forget(desktop)
    }

    fn ended(&self, instance: &atlas_core::process::Instance) -> Option<bool> {
        match instance.state() {
            atlas_core::process::State::Gone => Some(true),
            atlas_core::process::State::Running | atlas_core::process::State::Stopped => {
                Some(false)
            }
            atlas_core::process::State::Unknown => None,
        }
    }

    fn machines(&self) -> Option<Vec<atlas_core::process::Instance>> {
        evidence::machines()
    }

    fn program(&self) -> Result<std::path::PathBuf> {
        Ok(atlas_core::app_bundle::installed_app()?
            .join(atlas_core::app_bundle::VM_BUNDLE)
            .join("Contents/MacOS")
            .join(atlas_core::app_bundle::VM_EXECUTABLE))
    }
}

#[cfg(test)]
#[path = "vm_tests.rs"]
mod tests;

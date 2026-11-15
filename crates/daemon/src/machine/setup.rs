//! Atlas put inside a machine desktop's guest, once, by root, in phases its
//! host runs in order: the bundle published, the jobs that run it, a second
//! administrator, the guest account demoted, and Remote Login closed. Each
//! phase is safe to repeat, writes whole, reaches the disk before it
//! answers, and says what it found.
mod account;
mod jobs;
mod publish;
mod record;
mod system;

use atlas_core::{
    desktop::DesktopId,
    error::{Code, Error, Result},
    files::Token,
};
use serde_json::Value;
use std::path::{Path, PathBuf};

/// One phase of the setup, as `atlas machine-setup` names it.
#[derive(Debug)]
pub enum Step {
    /// Publish the Atlas staged at `source` as `/Applications/Atlas.app`.
    Publish { change: Token, source: PathBuf },
    /// Write and start the machine job and the root installer, and hand
    /// the pairing secret to `account`; `paired` when the station has said
    /// its pairing is complete, so no secret is needed.
    Jobs {
        desktop: DesktopId,
        enrolment: Token,
        account: String,
        paired: bool,
    },
    /// Make the second administrator `name`, its password the second line
    /// of standard input.
    Admin { name: String },
    /// Take `account` out of the administrators.
    Demote { account: String },
    /// Turn Remote Login off, last.
    Close,
}

/// Run `step` as root in the guest, `input` its standard input.
///
/// # Errors
///
/// Refuses unless it runs as root from an Atlas signed with its Developer
/// ID; fails when a phase cannot be done, saying why.
pub fn run(step: Step, input: &mut impl std::io::Read) -> Result<Value> {
    if !nix::unistd::geteuid().is_root() {
        return Err(Error::new(
            Code::Identity,
            "Atlas's setup runs as root alone.",
        ));
    }
    let program = std::env::current_exe()?;
    if !atlas_core::app_bundle::developer_id(&program) {
        return Err(Error::new(
            Code::Identity,
            "Atlas's setup runs from Atlas alone.",
        ));
    }
    let places = Places::guest();
    let system = system::Real;
    match step {
        Step::Publish { change, source } => publish::publish(&places, &system, &change, &source),
        Step::Jobs {
            desktop,
            enrolment,
            account,
            paired,
        } => jobs::jobs(&places, &system, &desktop, &enrolment, &account, paired),
        Step::Admin { name } => {
            let password = account::password_line(input)?;
            account::admin(&places, &system, &name, &password)
        }
        Step::Demote { account } => account::demote(&system, &account),
        Step::Close => account::close(&system),
    }
}

/// Where the setup works: `/` in the guest; a temporary root in tests.
#[derive(Clone, Debug)]
struct Places {
    root: PathBuf,
}

impl Places {
    fn guest() -> Self {
        Self {
            root: PathBuf::from("/"),
        }
    }

    #[cfg(test)]
    fn under(root: &Path) -> Self {
        Self {
            root: root.to_owned(),
        }
    }

    fn applications(&self) -> PathBuf {
        self.root.join("Applications")
    }

    /// Root's own folder for what the setup made and keeps.
    fn bootstrap(&self) -> PathBuf {
        self.root
            .join("Library/Application Support/Atlas/Bootstrap")
    }

    fn agents(&self) -> PathBuf {
        self.root.join("Library/LaunchAgents")
    }

    fn daemons(&self) -> PathBuf {
        self.root.join("Library/LaunchDaemons")
    }

    fn installer(&self) -> PathBuf {
        self.root
            .join("Library/Application Support/Atlas/Installer")
    }

    /// The VM's share, read-only to the guest.
    fn share(&self) -> PathBuf {
        self.root.join("Volumes/My Shared Files")
    }
}

/// One account of the guest, as its directory service gives it.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Account {
    uid: u32,
    gid: u32,
    home: PathBuf,
    full_name: Option<String>,
}

/// What the setup asks of the guest's system, given apart so tests stand in.
trait System {
    /// The code directory hash of `bundle`'s `atlas` when the bundle is
    /// Atlas's, whole and signed with its Developer ID.
    fn verified(&self, bundle: &Path) -> Option<Vec<u8>>;
    /// Run `launchctl` with `arguments`: whether it succeeded.
    fn launchctl(&self, arguments: &[&str]) -> bool;
    /// The account `name`, when there is one.
    fn account(&self, name: &str) -> Option<Account>;
    /// Make the administrator `name`, its full name `full_name` and its
    /// password `password`, given to the tool on its standard input.
    fn create_admin(&self, name: &str, full_name: &str, password: &str) -> bool;
    /// Set `name`'s password, given on the tool's standard input.
    fn set_password(&self, name: &str, password: &str) -> bool;
    /// Whether `name` is an administrator; none when that cannot be told.
    fn is_admin(&self, name: &str) -> Option<bool>;
    /// Make `name` an administrator, or not.
    fn set_admin(&self, name: &str, admin: bool) -> bool;
    /// Hide `name` from the login window.
    fn hide(&self, name: &str) -> bool;
    /// Turn Remote Login off a second from now, apart from this process.
    fn close_remote_login(&self) -> bool;
    /// Whether this process runs as root, so what it makes is made root's.
    fn root(&self) -> bool;
}

/// The refusal of a phase, for `why`.
fn refused(why: &str) -> Error {
    Error::new(
        Code::BadRequest,
        format!("Atlas's setup cannot go on: {why}."),
    )
}

fn io(error: impl std::error::Error + Send + Sync + 'static) -> Error {
    Error::new(Code::Io, "A setup step failed.").with_source(error)
}

/// Make what was written in `folder` reach the disk.
fn sync(folder: &Path) -> Result<()> {
    std::fs::File::open(folder)
        .and_then(|folder| folder.sync_all())
        .map_err(io)
}

#[cfg(test)]
#[path = "setup_tests.rs"]
mod tests;

//! What launchd says of the helper's jobs, and the commands that change them.
use super::{Layout, problem};
use atlas_core::error::Result;
use serde_json::Value;
use std::{
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, Instant},
};

#[derive(Debug, PartialEq, Eq)]
pub(super) enum Job {
    Absent,
    Loaded { pid: Option<u32>, path: PathBuf },
}

/// The worker's inspection projection. Missing and mistyped facts remain unknown;
/// decoding from Value retains its last-member-wins compatibility policy.
#[derive(Debug, PartialEq, Eq)]
pub struct JobReport {
    pub loaded: Option<bool>,
    pub pid: Option<u64>,
    path: Option<String>,
    plist: Option<String>,
    pub sha256: Option<String>,
    pub definition: Definition,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Definition {
    Absent,
    Regular,
    Other,
}

impl From<&Value> for JobReport {
    fn from(value: &Value) -> Self {
        let text = |key: &str| value[key].as_str().map(str::to_owned);
        Self {
            loaded: value["loaded"].as_bool(),
            pid: value["pid"].as_u64(),
            path: text("path"),
            plist: text("plist"),
            sha256: text("plistSha256"),
            definition: match value["plistKind"].as_str() {
                Some("absent") => Definition::Absent,
                Some("regular") => Definition::Regular,
                _ => Definition::Other,
            },
        }
    }
}

impl JobReport {
    /// Whether its definition is a file another version of Atlas wrote, not
    /// the `expected` one.
    pub fn outdated(&self, expected: &str) -> bool {
        match self.definition {
            Definition::Regular => self.sha256.as_deref() != Some(expected),
            Definition::Absent | Definition::Other => false,
        }
    }

    pub fn defined_at(&self, path: &Path) -> bool {
        self.path.as_deref() == Some(&*path.to_string_lossy())
    }

    /// Whether launchd runs it from the regular definition in its place.
    pub fn running(&self) -> bool {
        self.loaded == Some(true)
            && self.pid.is_some_and(|pid| pid > 1)
            && self.path.is_some()
            && self.path == self.plist
            && self.definition == Definition::Regular
    }
}

/// Parse `launchctl print` for a job: its pid, if running, and the definition
/// path launchd recorded, which a loaded job must have.
fn parse_job(text: &str) -> Result<Job> {
    let field = |key: &str| {
        text.lines()
            .find_map(|line| line.trim().strip_prefix(key).map(str::to_owned))
    };
    let path = field("path = ")
        .filter(|path| path.starts_with('/'))
        .ok_or_else(|| problem("launchctl reported a job without a definition path"))?;

    Ok(Job::Loaded {
        pid: field("pid = ").and_then(|pid| pid.parse().ok()),
        path: PathBuf::from(path),
    })
}

/// launchctl's exit status for a user with no domain at all: logged out,
/// or never logged in.
const NO_SUCH_USER_DOMAIN: i32 = 112;
/// launchctl's exit status for a service its domain does not have.
const NO_SUCH_SERVICE: i32 = 113;
/// launchctl's exit status for a domain that does not exist.
const NO_SUCH_DOMAIN: i32 = 125;

/// Fails closed: only launchctl's "no such service", "no such desktop
/// domain" and "no domain for this user" (the helper is signed out, or
/// never signed in) statuses mean absent. Setup reads this every second and
/// a half, so launchctl gets a time limit; one that runs past it is an
/// error, never absent.
pub(super) fn job(layout: &Layout, label: &str) -> Result<Job> {
    let output = atlas_core::bounded::output(
        Command::new("/bin/launchctl").args(["print", &format!("{}/{label}", layout.domain())]),
        atlas_core::bounded::FILE,
    )?;
    match output.status.code() {
        Some(0) => parse_job(&String::from_utf8_lossy(&output.stdout)),
        // When the helper is signed out its desktop domain does not exist,
        // so nothing can be loaded there.
        Some(NO_SUCH_SERVICE | NO_SUCH_DOMAIN | NO_SUCH_USER_DOMAIN) => Ok(Job::Absent),
        _ => Err(problem(format!(
            "launchctl could not inspect {label}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ))),
    }
}

/// The pid of `label`'s job in this login, when launchd has it running.
pub fn job_pid(layout: &Layout, label: &str) -> Option<u32> {
    match job(layout, label).ok()? {
        Job::Loaded { pid, .. } => pid,
        Job::Absent => None,
    }
}

pub(super) fn launchctl(operation: &str, arguments: &[&str]) -> Result<()> {
    let output = atlas_core::spawn::output(
        Command::new("/bin/launchctl")
            .arg(operation)
            .args(arguments),
    )?;
    if output.status.success() {
        Ok(())
    } else {
        Err(problem(format!(
            "launchctl {} failed: {}",
            operation,
            String::from_utf8_lossy(&output.stderr).trim()
        )))
    }
}

/// Poll `label` for up to five seconds until `wanted` accepts its state,
/// returning what `wanted` takes from it; `what` names the awaited state.
pub(super) fn wait_for<T>(
    layout: &Layout,
    label: &str,
    what: &str,
    wanted: impl Fn(Job) -> Option<T>,
) -> Result<T> {
    let deadline = Instant::now() + Duration::from_secs(5);

    loop {
        if let Some(value) = wanted(job(layout, label)?) {
            return Ok(value);
        }
        if Instant::now() >= deadline {
            return Err(problem(format!("{label} did not become {what}")));
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

pub(super) fn as_loaded(job: Job) -> Option<(Option<u32>, PathBuf)> {
    match job {
        Job::Loaded { pid, path } => Some((pid, path)),
        Job::Absent => None,
    }
}

#[cfg(test)]
#[path = "launchd_tests.rs"]
mod tests;

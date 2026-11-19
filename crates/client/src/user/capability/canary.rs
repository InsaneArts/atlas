//! The native Save/Open check in the second login: the fixture runs there
//! through the helper's own `run`, its files are read from this disk, where
//! the second login keeps its canary folder, and its paths are compared as
//! this Mac resolves them.
use super::{Generation, Record, boot_time, generation_at, now, ready_epoch, record_path};
#[cfg(test)]
use crate::capability::canary::{OPEN_TEXT, SAVE_TEXT, button, sheets};
use crate::{
    capability::canary::{
        self, ARCHIVED, Desk, Fixture, PanelScan, cleaned, cleaning, launched, launching, problem,
        scanned,
    },
    client::{Client, Run, timestamp},
};
use atlas_control::Ownership;
use atlas_core::{
    error::{Code, Error, Result},
    files::write_private,
    screen::windows::Shown,
    session::{Display, Epoch, Observation},
    transport::run_output,
};
use serde_json::{Value, json};
use std::{
    fs,
    os::unix::fs::DirBuilderExt,
    path::{Path, PathBuf},
    time::Duration,
};

const MAX_FIXTURE_RECORD: u64 = 1 << 20;

const MAX_ARCHIVED: u64 = 16 << 20;

/// A regular file the fixture wrote in the helper, read whole up to `limit`
/// bytes: links, pipes and devices are refused, and a larger file is an
/// error rather than read without end.
fn read_bounded(path: &Path, limit: u64) -> std::io::Result<Vec<u8>> {
    use std::os::unix::fs::OpenOptionsExt;

    let file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)?;
    if !file.metadata()?.is_file() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("{} is not a regular file", path.display()),
        ));
    }

    let bytes = atlas_core::files::read_up_to(file, limit + 1)?;

    if bytes.len() as u64 > limit {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("{} exceeds {limit} bytes", path.display()),
        ));
    }

    Ok(bytes)
}

fn fixture_text(path: &Path) -> Option<String> {
    String::from_utf8(read_bounded(path, MAX_FIXTURE_RECORD).ok()?).ok()
}

/// Whether `left`, as the fixture said it, and `right` name one file as
/// this Mac resolves them.
pub(super) fn same_path(left: Option<&str>, right: &Path) -> bool {
    left.and_then(|left| fs::canonicalize(left).ok())
        .is_some_and(|left| fs::canonicalize(right).is_ok_and(|right| left == right))
}

/// The second login, as the check drives it: under one controller
/// ownership and in the one display epoch its input is held to.
struct Local<'a> {
    client: &'a Client,
    ownership: &'a Ownership,
    epoch: Epoch,
    program: PathBuf,
}

impl Desk for Local<'_> {
    fn display(&self) -> &Display {
        &self.epoch.display
    }

    async fn launch(&self) -> Result<Fixture> {
        let reply = self
            .client
            .run(launching(&self.program), Run::owned(self.ownership))
            .await?;

        let (pid, directory) = run_output(&reply, 4096)
            .ok()
            .and_then(|bytes| String::from_utf8(bytes).ok())
            .as_deref()
            .and_then(launched)
            .ok_or_else(|| problem("The helper could not launch the canary fixture"))?;

        Ok(Fixture {
            pid,
            program: self.program.clone(),
            directory,
        })
    }

    async fn text(&self, fixture: &Fixture, name: &str) -> Option<String> {
        fixture_text(&fixture.directory.join(name))
    }

    /// The panel observer's latest scan of `attempt`, if it is fresh.
    async fn observed(&self, fixture: &Fixture, attempt: &str) -> Result<Option<PanelScan>> {
        let observed = self
            .text(fixture, "panel-observer.json")
            .await
            .and_then(|text| serde_json::from_str::<Value>(&text).ok());
        let ms = atlas_core::time::now_ms();
        Ok(observed
            .filter(|observed| {
                observed["finishedAt"].as_f64().is_some_and(|finished| {
                    scanned(observed, attempt, fixture.pid)
                        && ms.saturating_sub(finished as u64) <= 1500
                })
            })
            .map(|observed| PanelScan::from(&observed)))
    }

    async fn keys(&self, actions: Value) -> Result<()> {
        self.client
            .input_owned(
                self.ownership,
                atlas_core::actions::Batch::decode(&actions),
                Some((&self.epoch, std::time::Instant::now())),
                None,
            )
            .await
            .map(|_| ())
    }

    /// The helper's on-screen windows, front to back, and where its Dock
    /// takes clicks.
    async fn windows(&self) -> Result<Shown> {
        self.client
            .windows_in(None, true, Duration::from_secs(5))
            .await
    }

    async fn capture(&self, name: &str) -> Option<PathBuf> {
        let frame = self.client.frame(Some(1600)).await.ok()?;
        let directory = self.client.output_dir().ok()?;
        let path = directory.join(format!("native-{}-{name}.png", timestamp()));
        write_private(&path, &frame.png).ok()?;
        Some(path)
    }

    /// Copy the fixture's own records next to the screenshots.
    async fn archive(&self, fixture: &Fixture) -> Result<Vec<PathBuf>> {
        let directory = self
            .client
            .output_dir()?
            .join(format!("native-{}", timestamp()));
        fs::DirBuilder::new().mode(0o700).create(&directory)?;

        for name in ARCHIVED {
            match read_bounded(&fixture.directory.join(name), MAX_ARCHIVED) {
                Ok(bytes) => write_private(&directory.join(name), &bytes)?,
                // Saved output exists only if Save completed.
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
        }

        Ok(vec![directory])
    }

    async fn cleanup(&self, fixture: &Fixture, remove: bool) -> Result<()> {
        let reply = self
            .client
            .run(cleaning(fixture, remove), Run::owned(self.ownership))
            .await?;
        cleaned(&reply)
    }

    async fn same_path(&self, _fixture: &Fixture, path: Option<&str>, expected: &Path) -> bool {
        same_path(path, expected)
    }
}

/// Run the native Save/Open canary in the second login and record the
/// dated outcome, tagged with the session generation it observed.
///
/// # Errors
///
/// Returns ownership, identity, launch or record-storage failures. Individual panel
/// checks and archive/cleanup failures are retained in the returned report.
pub(crate) async fn verify(client: &Client, fixture: &Path) -> Result<Value> {
    let uid = client.user_config()?.helper_uid;
    let ownership = client.controller.acquire(false)?;
    let epoch =
        ready_epoch(&Observation::decode(&client.status().await), uid).ok_or_else(|| {
            Error::new(
                Code::Permission,
                "verify-native needs a hidden helper with ready input; see atlas doctor",
            )
        })?;

    let boot = boot_time().ok_or_else(|| problem("The boot time is unavailable"))?;
    let generation = Generation::of(&epoch, boot);
    let local = Local {
        client,
        ownership: &ownership,
        epoch,
        program: fixture.to_owned(),
    };
    let ran = canary::run(&local).await?;

    let after = generation_at(&Observation::decode(&client.status().await), uid, boot);
    let unchanged = after.as_ref() == Some(&generation);
    let passed = unchanged
        && ran.checks.len() == canary::Panel::ALL.len()
        && ran.checks.iter().all(|check| check.passed);
    let record = Record {
        observed_at: now(),
        passed,
        checks: ran.checks,
        generation,
        evidence: ran.evidence,
    };
    write_record(uid, &record)?;

    let mut report = json!({
        "ok": record.passed,
        "passed": record.passed,
        "checks": record.checks,
        "evidence": record.evidence,
        "record": record_path(uid)?,
    });

    if !unchanged {
        report["warning"] = json!("The helper session changed during the canary");
    }

    if let Err(error) = ran.archived {
        report["archiveError"] = json!(error.error);
        report["fixtureDirectory"] = json!(ran.directory);
    }

    if let Err(error) = ran.cleanup {
        report["cleanupError"] = json!(error.error);
    }

    Ok(report)
}

fn write_record(uid: u32, record: &Record) -> Result<()> {
    let path = record_path(uid)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    write_private(&path, &serde_json::to_vec_pretty(record)?)
}

#[cfg(test)]
#[path = "canary_tests.rs"]
mod tests;

//! The native Save/Open check on a VM. The fixture runs in the guest from
//! the one copy kept there for it, in the one proof the check took, its input sent in
//! that proof's session and generation, and its canary folder read only by
//! the guest, which opens it once with no link followed and reads its fixed
//! files beside that descriptor. A panel's scan is asked with a fresh
//! challenge each time, and counts only when it carries that challenge back
//! within [`SCAN_FRESH`] of being asked, on this Mac's own clock: no clock
//! of the guest's is compared with this Mac's. Its paths are the guest's,
//! resolved by the guest against the canary folder it opened, never looked
//! at on this Mac.
//!
//! What a check came to is kept in the VM's own records folder, tied to
//! the machine epoch its proof pinned, and doctor reports it as passed only
//! in that epoch and within 30 minutes.
use crate::{
    capability::{
        NativeState, allow_in_window,
        canary::{
            self, ARCHIVED, Check, Desk, Fixture, Panel, PanelScan, cleaned, cleaning, launched,
            launching, problem, scanned,
        },
    },
    client::{Client, Run, Verified, timestamp},
};
use atlas_control::Ownership;
use atlas_core::{
    error::{Code, Error, Result},
    files,
    screen::windows::Shown,
    session::{Display, MachineEpoch},
    transport::run_output,
};
use atlas_session::compatibility::NATIVE_CHECK;
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    cell::Cell,
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

/// The VM's native record, in its records folder.
const RECORD: &str = "native.json";
/// How long after it was asked a scan still counts, on this Mac's clock.
const SCAN_FRESH: Duration = Duration::from_secs(3);
/// How long a check's outcome describes the VM.
const FRESH: Duration = Duration::from_secs(30 * 60);
/// Clock skew tolerated before an observation counts as from the future.
const SKEW: u64 = 60;
/// The most of one of the fixture's files kept as evidence.
const MAX_ARCHIVED: u64 = 16 << 20;
/// How long the fixture's files are left still once it stopped, so the
/// guest hands them over as files nothing is writing.
const QUIET: Duration = Duration::from_millis(2500);

/// A VM's check: its outcome, the epoch its proof pinned, and its
/// evidence, kept on this Mac.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Record {
    observed_at: u64,
    passed: bool,
    checks: Vec<Check>,
    epoch: MachineEpoch,
    evidence: Vec<PathBuf>,
}

impl Record {
    fn consistent(&self) -> bool {
        Check::consistent(&self.checks, self.passed)
    }
}

/// Where the VM whose records folder is `records` keeps its native record.
fn record_path(records: &Path) -> PathBuf {
    records.join(RECORD)
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Keep `record` in the VM's records folder `records`, as it must survive
/// a crash.
fn keep(records: &Path, record: &Record) -> Result<PathBuf> {
    let path = record_path(records);
    files::write_durable(&path, &serde_json::to_vec_pretty(record)?, 0o600)?;
    Ok(path)
}

/// What a scan asked of the fixture must carry back.
struct Asked<'a> {
    attempt: &'a str,
    challenge: &'a str,
    epoch: &'a str,
    pid: u32,
}

/// What a guest's answer to a scan comes to.
#[derive(Debug)]
enum Judged {
    Fresh(Value),
    /// Late, or an answer to another challenge: never fresh.
    Late,
    /// The fixture's observer is not allowed Accessibility.
    Untrusted,
    /// A scan of this challenge, not yet whole.
    Unfinished,
}

/// What `reply`, the guest's answer to the scan `asked`, comes to, given
/// `elapsed` since it was asked on this Mac's clock.
fn judged(reply: &Value, asked: &Asked, elapsed: Duration) -> Judged {
    let scan = &reply["scan"];
    if reply["ok"] != true
        || elapsed > SCAN_FRESH
        || scan["challenge"] != asked.challenge
        || scan["epoch"] != asked.epoch
        || scan["attemptID"] != asked.attempt
    {
        return Judged::Late;
    }
    if scan["trusted"] == false {
        return Judged::Untrusted;
    }
    if scanned(scan, asked.attempt, asked.pid) {
        Judged::Fresh(scan.clone())
    } else {
        Judged::Unfinished
    }
}

/// What `expected`, the canary folder `directory` or a file directly in
/// it, is named as in a guest's resolve: empty for the folder itself, else
/// the file's name.
fn relative(expected: &Path, directory: &Path) -> Option<String> {
    if expected == directory {
        return Some(String::new());
    }
    (expected.parent() == Some(directory))
        .then(|| expected.file_name()?.to_str().map(str::to_owned))
        .flatten()
}

/// A VM, as the check drives it: in the one proof `verified`, under one
/// controller ownership.
struct Guest<'a> {
    client: &'a Client,
    ownership: &'a Ownership,
    verified: &'a Verified,
    generation: u64,
    display: Display,
    /// The session the proof pinned, as a scan carries it back.
    epoch: String,
    /// The fixture on this Mac, copied into the guest to run.
    program: PathBuf,
    /// Whether the last scan asked came late.
    late: Cell<bool>,
    /// Whether the fixture was stopped so its files could be kept.
    stopped: Cell<bool>,
}

impl Guest<'_> {
    fn name(&self) -> &str {
        &self.client.selected.name
    }

    /// The guest's answer to the native read `asked` of `fixture`'s folder,
    /// in the proof's session and generation; a refusal other than a late
    /// scan is the guest's, said of the VM.
    async fn native(&self, fixture: &Fixture, mut asked: Value) -> Result<Value> {
        asked["op"] = json!("native");
        asked["directory"] = json!(fixture.directory);
        asked["session"] = json!(self.verified.pin()?);
        let reply = self
            .client
            .link
            .call_in(&asked, Some(self.generation))
            .await
            .map_err(Error::certain)?;
        let reply = crate::vm::guest_words::reply_said_of(reply, self.name());
        if reply["ok"] != true && reply["late"] != true {
            return Err(atlas_core::transport::refusal(&reply));
        }
        Ok(reply)
    }
}

impl Desk for Guest<'_> {
    fn display(&self) -> &Display {
        &self.display
    }

    /// Start the fixture in a new canary folder from the guest's copy of
    /// it, the one kept for this fixture or one made now.
    async fn launch(&self) -> Result<Fixture> {
        let program =
            crate::vm::copies::fixture_copy(self.client, self.verified, &self.program).await?;
        let reply = self
            .client
            .run(
                launching(&program),
                Run::owned(self.ownership).pinned(self.verified),
            )
            .await?;
        let (pid, directory) = run_output(&reply, 4096)
            .ok()
            .and_then(|bytes| String::from_utf8(bytes).ok())
            .as_deref()
            .and_then(launched)
            .ok_or_else(|| problem(format!("{} could not launch the fixture.", self.name())))?;
        Ok(Fixture {
            pid,
            program,
            directory,
        })
    }

    async fn text(&self, fixture: &Fixture, name: &str) -> Option<String> {
        if self.stopped.get() {
            return None;
        }
        let read = self
            .native(fixture, json!({"action": "files"}))
            .await
            .ok()?;
        let bytes = STANDARD.decode(read["files"][name].as_str()?).ok()?;
        String::from_utf8(bytes).ok()
    }

    /// A scan of `attempt` asked with a fresh challenge, counted only when
    /// it carries it back in time.
    async fn observed(&self, fixture: &Fixture, attempt: &str) -> Result<Option<PanelScan>> {
        let challenge = files::token();
        let sent = Instant::now();
        let reply = self
            .native(
                fixture,
                json!({"action": "scan", "attempt": attempt, "challenge": challenge}),
            )
            .await?;
        let asked = Asked {
            attempt,
            challenge: &challenge,
            epoch: &self.epoch,
            pid: fixture.pid,
        };
        let judged = judged(&reply, &asked, sent.elapsed());
        self.late.set(matches!(judged, Judged::Late));
        match judged {
            Judged::Fresh(scan) => Ok(Some(PanelScan::from(&scan))),
            Judged::Untrusted => Err(problem(format!(
                "The fixture in {name} is not allowed Accessibility; allow it in {name}'s window \
                 during maintenance with atlas desktop maintain {name}.",
                name = self.name()
            ))),
            Judged::Late | Judged::Unfinished => Ok(None),
        }
    }

    async fn keys(&self, actions: Value) -> Result<()> {
        self.client
            .input_owned_in(
                self.ownership,
                atlas_core::actions::Batch::decode(&actions),
                self.verified,
                None,
            )
            .await
            .map(|_| ())
    }

    async fn windows(&self) -> Result<Shown> {
        self.client
            .windows_in(Some(self.verified), true, Duration::from_secs(5))
            .await
    }

    async fn capture(&self, name: &str) -> Option<PathBuf> {
        let frame = self.client.frame_in(self.verified, Some(1600)).await.ok()?;
        let directory = self.client.output_dir().ok()?;
        let path = directory.join(format!("native-{}-{name}.png", timestamp()));
        files::write_private(&path, &frame.png).ok()?;
        Some(path)
    }

    /// Stop the fixture, keeping its folder, and once its files are still,
    /// collect them out of the guest: a file written as it is read is
    /// never handed over.
    async fn archive(&self, fixture: &Fixture) -> Result<Vec<PathBuf>> {
        let reply = self
            .client
            .run(
                cleaning(fixture, false),
                Run::owned(self.ownership).pinned(self.verified),
            )
            .await?;
        cleaned(&reply)?;
        self.stopped.set(true);
        tokio::time::sleep(QUIET).await;
        let sources: Vec<String> = ARCHIVED
            .iter()
            .map(|name| fixture.directory.join(name).to_string_lossy().into_owned())
            .collect();
        let collected =
            crate::collect::collect_in(self.client, self.verified, &sources, MAX_ARCHIVED).await?;
        // Saved output exists only if Save completed.
        let refused = collected["failed"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|failed| failed["reason"] != "missing");
        if let Some(refused) = refused {
            return Err(problem(
                refused["error"]
                    .as_str()
                    .unwrap_or("The fixture's files could not be kept."),
            ));
        }
        Ok(collected["collected"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|kept| kept["path"].as_str().map(PathBuf::from))
            .collect())
    }

    /// Stop the fixture and remove its folder when `remove`; the guest's
    /// copy of it stays for the next check.
    async fn cleanup(&self, fixture: &Fixture, remove: bool) -> Result<()> {
        let reply = self
            .client
            .run(
                cleaning(fixture, remove),
                Run::owned(self.ownership).pinned(self.verified),
            )
            .await?;
        cleaned(&reply)
    }

    /// Asked of the guest, which resolves `path` against the canary folder
    /// it opened, never as this Mac would.
    async fn same_path(&self, fixture: &Fixture, path: Option<&str>, expected: &Path) -> bool {
        let (Some(path), Some(named)) = (path, relative(expected, &fixture.directory)) else {
            return false;
        };
        self.native(fixture, json!({"action": "resolve", "paths": [path]}))
            .await
            .is_ok_and(|reply| reply["resolved"][0] == named.as_str())
    }

    fn timed_out(&self, stage: &str) -> Error {
        if self.late.get() {
            return problem(
                "The fixture's scan did not answer this check's challenge within 3 seconds.",
            );
        }
        problem(format!("Timed out: {stage}"))
    }
}

/// Run the native Save/Open check on the VM `client` names, with the
/// fixture at `fixture` on this Mac, in one proof of it, and keep its dated
/// outcome in the VM's records folder.
///
/// # Errors
///
/// Refuses a VM that does not prove or is held, one whose Atlas cannot
/// check native panels and one whose input is not allowed, before anything
/// is copied or launched; returns copy, launch or record-storage failures.
/// Individual panel checks and archive or cleanup failures are in the
/// report.
pub(crate) async fn verify(client: &Client, fixture: &Path) -> Result<Value> {
    let ownership = client.controller.acquire(false)?;
    let verified = client.verify_vm().await?;
    let name = client.selected.name.clone();
    let generation = client.generation_offering(&verified, NATIVE_CHECK, |name| {
        format!("{name}'s Atlas is from an older build and cannot check native Save and Open.")
    })?;
    let pinned = verified.machine().ok_or_else(|| client.unreachable())?;
    if let Some((code, why)) = &pinned.input_held {
        let held = Error::new(*code, why.clone());
        return Err(if *code == Code::Permission {
            held.with("next", json!(allow_in_window(&name, "Accessibility")))
        } else {
            held
        });
    }
    let epoch = pinned.epoch.clone();
    let guest = Guest {
        client,
        ownership: &ownership,
        verified: &verified,
        generation,
        display: verified.session().epoch.display.clone(),
        epoch: atlas_core::native::epoch(&verified.pin()?),
        program: fixture.to_owned(),
        late: Cell::new(false),
        stopped: Cell::new(false),
    };
    let ran = canary::run(&guest).await?;

    let unchanged = client.reproved(pinned).await.is_ok();
    let passed = unchanged
        && ran.checks.len() == Panel::ALL.len()
        && ran.checks.iter().all(|check| check.passed);
    let record = Record {
        observed_at: now(),
        passed,
        checks: ran.checks,
        epoch,
        evidence: ran.evidence,
    };
    let kept = keep(client.output_dir()?, &record)?;

    let mut report = json!({
        "ok": record.passed,
        "passed": record.passed,
        "checks": record.checks,
        "evidence": record.evidence,
        "record": kept,
    });
    if !unchanged {
        report["warning"] = json!(format!(
            "{name} changed, or its window was opened, during the check."
        ));
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

/// What the VM's record file holds.
enum Evidence {
    Missing,
    Unreadable,
    Present(Record),
}

fn read_record(records: &Path) -> Evidence {
    match std::fs::read(record_path(records)) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_or(Evidence::Unreadable, Evidence::Present),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Evidence::Missing,
        Err(_) => Evidence::Unreadable,
    }
}

/// Doctor's view of native dialogs on the VM `name`, whose status is
/// `status` and whose records folder is `records`, at `at` on this Mac's
/// clock: passed only while the fresh proof's epoch is the one the check
/// was observed in, within 30 minutes; a record from anywhere else never
/// counts.
fn assessment(name: &str, records: &Path, status: &Value, at: u64) -> (NativeState, Value) {
    use NativeState::{Failed, NotChecked, Passed, Stale, Unknown};
    let said =
        |state: NativeState, reason: String| (state, json!({"state": state, "reason": reason}));
    let current = (status["ok"] == true && status["proven"] == true)
        .then(|| serde_json::from_value::<MachineEpoch>(status["proof"]["epoch"].clone()).ok())
        .flatten();
    let Some(current) = current else {
        return said(
            NotChecked,
            format!("Whether native Save/Open works on {name} is judged once it is proven again."),
        );
    };
    let record = match read_record(records) {
        Evidence::Present(record) => record,
        Evidence::Missing => {
            return said(
                NotChecked,
                format!(
                    "Native Save/Open has not been checked on {name}; use atlas run or atlas open \
                     instead, or check it with atlas --desktop {name} verify-native."
                ),
            );
        }
        Evidence::Unreadable => {
            return said(
                Unknown,
                format!(
                    "Native Save/Open's recorded check of {name} could not be read; use atlas run \
                     or atlas open instead."
                ),
            );
        }
    };
    let (state, reason) = if !record.consistent() {
        (
            Unknown,
            "The recorded observation is inconsistent and was ignored.".to_owned(),
        )
    } else if record.observed_at > at.saturating_add(SKEW) {
        (
            Unknown,
            "The recorded observation is dated in the future.".to_owned(),
        )
    } else if record.epoch != current {
        (
            Stale,
            format!(
                "{name}'s session, display, capture or input process changed since it was checked."
            ),
        )
    } else if at.saturating_sub(record.observed_at) > FRESH.as_secs() {
        (
            Stale,
            "The observation is older than 30 minutes.".to_owned(),
        )
    } else if record.passed {
        (
            Passed,
            format!(
                "Native Save and Open completed with exact bytes in {name} at the recorded time, \
                 as the check's files in {name} report it; this is not a connection check."
            ),
        )
    } else {
        (
            Failed,
            format!("Native Save/Open did not complete in {name}'s session."),
        )
    };
    let report = json!({
        "state": state,
        "reason": reason,
        "observedAt": record.observed_at,
        "ageSeconds": at.saturating_sub(record.observed_at),
        "checks": record.checks,
        "evidence": record.evidence,
    });
    (state, report)
}

/// Doctor's word on native dialogs on the VM `client` names, whose status
/// is `status`, now.
pub(crate) fn native_dialogs(client: &Client, status: &Value) -> (NativeState, Value) {
    assessment(&client.selected.name, client.records(), status, now())
}

#[cfg(test)]
#[path = "native_tests.rs"]
mod tests;

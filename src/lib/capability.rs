//! Native dialog capability: an explicit, mutating canary and its dated record.
//!
//! Transport, identity and permissions were all green while native Save/Open
//! failed, so readiness never implies dialogs work. `verify-native` launches
//! a fresh copy of the maintained AppKit fixture, drives its real NSSavePanel
//! and NSOpenPanel with guarded keyboard input under one controller ownership
//! and one display epoch, checks exact file bytes, archives the fixture's
//! evidence, and records the outcome tagged with the session generation it
//! observed. Doctor reports a pass only while that generation is unchanged,
//! the observation is fresh, and both Save and Open passed.
use crate::{
    client::{Client, timestamp},
    config::home,
    control::{Ownership, write_private},
    error::{Error, Result},
    native::windows::{Bounds, Window, frontmost_pid, unobstructed, visible_point},
    session::{Display, Epoch, require_hidden},
    transport::run_output,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    fs,
    os::unix::fs::DirBuilderExt,
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

/// Default location of the compiled fixture (tests/live/DesktopFixture.swift).
pub const FIXTURE: &str = "/Users/Shared/UnderstudyTests/DesktopFixture";
/// How long an observation stays meaningful.
const FRESH: Duration = Duration::from_secs(30 * 60);
/// Clock skew tolerated before an observation counts as from the future.
const SKEW: u64 = 60;
// Explicit escapes: the fixture's Open file ends in a decomposed e + U+0301,
// which renders like the precomposed U+00E9 typed for Save but differs in bytes.
const SAVE_TEXT: &str = "Understudy native Save \u{2014} \u{10e5}\u{10d0}\u{10e0}\u{10d7}\u{10e3}\u{10da}\u{10d8} \u{65e5}\u{672c}\u{8a9e} \u{1f9d1}\u{1f3fd}\u{200d}\u{1f4bb} \u{e9}";
const OPEN_TEXT: &str = "Understudy native Open \u{2014} \u{10e5}\u{10d0}\u{10e0}\u{10d7}\u{10e3}\u{10da}\u{10d8} \u{65e5}\u{672c}\u{8a9e} \u{1f9d1}\u{1f3fd}\u{200d}\u{1f4bb} e\u{301}\n";

/// What must be unchanged for an observation to still describe this session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Generation {
    pub capture_pid: u32,
    pub input_pid: u32,
    pub security_session_id: u64,
    pub display_id: u64,
    pub display: Display,
    pub boot: u64,
}

impl Generation {
    fn of(epoch: &Epoch, boot: u64) -> Self {
        Self {
            capture_pid: epoch.capture_pid,
            input_pid: epoch.input_pid,
            security_session_id: epoch.security_session_id,
            display_id: epoch.display_id,
            display: epoch.display.clone(),
            boot,
        }
    }
}

/// One native operation's outcome and, if it failed, where.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Check {
    pub kind: String,
    pub passed: bool,
    pub stage: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Record {
    pub observed_at: u64,
    pub passed: bool,
    pub checks: Vec<Check>,
    pub generation: Generation,
    pub evidence: Vec<PathBuf>,
}

impl Record {
    /// Exactly one Save and one Open check, and `passed` only if both passed.
    fn consistent(&self) -> bool {
        let outcome = |kind: &str| {
            let mut matches = self.checks.iter().filter(|check| check.kind == kind);
            let first = matches.next().map(|check| check.passed);
            if matches.next().is_some() {
                None
            } else {
                first
            }
        };
        match (self.checks.len(), outcome("save"), outcome("open")) {
            (2, Some(save), Some(open)) => self.passed == (save && open),
            _ => false,
        }
    }
}

fn problem(message: impl Into<String>) -> Error {
    Error::new("native", message)
}

fn record_path(uid: u32) -> Result<PathBuf> {
    Ok(home()?
        .join("Library/Application Support/Understudy")
        .join(format!("native-{uid}.json")))
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Seconds since the epoch at which this machine booted.
// FFI: sysctl kern.boottime has no safe binding.
#[allow(unsafe_code)]
fn boot_time() -> Option<u64> {
    let mut boot = libc::timeval {
        tv_sec: 0,
        tv_usec: 0,
    };
    let mut size = std::mem::size_of::<libc::timeval>();
    let mut name = [libc::CTL_KERN, libc::KERN_BOOTTIME];
    // SAFETY: valid MIB, output buffer and size for kern.boottime.
    let status = unsafe {
        libc::sysctl(
            name.as_mut_ptr(),
            2,
            (&mut boot as *mut libc::timeval).cast(),
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    };
    (status == 0).then(|| u64::try_from(boot.tv_sec).unwrap_or_default())
}

/// The generation a status snapshot describes, if it shows a ready helper.
pub fn generation(status: &Value, uid: u32) -> Option<Generation> {
    let session = require_hidden(status, uid).ok().filter(|s| s.input)?;
    Some(Generation::of(&session.epoch, boot_time()?))
}

/// Doctor's view of native dialogs: unknown, stale, passed or failed.
pub fn assess(record: Option<&Record>, current: Option<&Generation>, at: u64) -> Value {
    let Some(record) = record else {
        return json!({
            "state": "unknown",
            "reason": "No native Save/Open observation. Run understudy verify-native; dialogs \
                       were qualified only with an authenticated Screen Sharing session.",
        });
    };
    let (state, reason) = if !record.consistent() {
        (
            "unknown",
            "The recorded observation is inconsistent and was ignored",
        )
    } else if record.observed_at > at + SKEW {
        ("unknown", "The recorded observation is dated in the future")
    } else if current != Some(&record.generation) {
        (
            "stale",
            "The helper session, display, capture or input process changed since it was observed",
        )
    } else if at.saturating_sub(record.observed_at) > FRESH.as_secs() {
        ("stale", "The observation is older than 30 minutes")
    } else if record.passed {
        ("passed", "Native Save and Open completed with exact bytes")
    } else {
        (
            "failed",
            "Native Save/Open did not complete in this session",
        )
    };
    json!({
        "state": state,
        "reason": reason,
        "observedAt": record.observed_at,
        "ageSeconds": at.saturating_sub(record.observed_at),
        "checks": record.checks,
        "evidence": record.evidence,
    })
}

/// Doctor's native-dialog assessment for a status snapshot.
pub fn native_dialogs(status: &Value, uid: u32) -> Value {
    let record = record_path(uid)
        .ok()
        .and_then(|path| fs::read(path).ok())
        .and_then(|bytes| serde_json::from_slice::<Record>(&bytes).ok());
    assess(record.as_ref(), generation(status, uid).as_ref(), now())
}

/// Doctor's report with the recorded native-dialog observation in place of
/// the default unknown.
pub fn doctor(status: Value, uid: u32) -> Value {
    let native = native_dialogs(&status, uid);
    let mut report = crate::session::diagnose(status, uid);
    report["capabilities"]["nativeDialogs"] = native;
    report
}

/// Doctor's report without the raw helper status it was judged from: the
/// verdict, each check, what to do, and which helper and display. `doctor
/// --verbose` keeps the status for diagnosis.
pub fn summary(mut report: Value) -> Value {
    let session = report
        .as_object_mut()
        .and_then(|report| report.remove("session"))
        .unwrap_or(Value::Null);
    report["helper"] = json!({
        "name": session["user"]["name"],
        "uid": session["user"]["uid"],
    });
    report["display"] = session["display"].clone();
    report
}

/// A fresh fixture process running in the helper.
struct Fixture {
    pid: u32,
    program: PathBuf,
    directory: PathBuf,
}

impl Fixture {
    fn read(&self, name: &str) -> Option<Value> {
        serde_json::from_slice(&fs::read(self.directory.join(name)).ok()?).ok()
    }
    fn state(&self) -> Value {
        self.read("state.json").unwrap_or(Value::Null)
    }
    /// The panel observer's latest scan of `attempt`, if it is fresh.
    fn observed(&self, attempt: &str) -> Option<Value> {
        let observed = self.read("panel-observer.json")?;
        let ms = u64::try_from(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .ok()?
                .as_millis(),
        )
        .ok()?;
        let finished = observed["finishedAt"].as_f64()? as u64;
        (observed["attemptID"] == attempt
            && observed["trusted"] == true
            && observed["parentPID"] == self.pid
            && observed["scanComplete"] == true
            && ms.saturating_sub(finished) <= 1500)
            .then_some(observed)
    }
    fn completion(&self, attempt: &str, kind: &str) -> Option<Value> {
        fs::read_to_string(self.directory.join("events.jsonl"))
            .ok()?
            .lines()
            .filter_map(|line| serde_json::from_str::<Value>(line).ok())
            .find(|event| {
                event["attemptID"] == attempt && event["event"] == format!("{kind}Closed")
            })
    }
    fn file(&self, name: &str) -> PathBuf {
        self.directory.join(name)
    }
}

fn button<'a>(observed: &'a Value, title: &str) -> Option<&'a Value> {
    let mut matches = observed["controls"]
        .as_array()?
        .iter()
        .filter(|control| control["role"] == "AXButton" && control["title"] == title);
    let first = matches.next();
    matches.next().is_none().then_some(first).flatten()
}

fn sheets(observed: &Value) -> usize {
    observed["controls"]
        .as_array()
        .map(|controls| {
            controls
                .iter()
                .filter(|control| control["role"] == "AXSheet")
                .count()
        })
        .unwrap_or(0)
}

fn same_path(left: &Value, right: &Path) -> bool {
    left.as_str()
        .and_then(|left| fs::canonicalize(left).ok())
        .is_some_and(|left| fs::canonicalize(right).is_ok_and(|right| left == right))
}

/// Poll `check` until it yields a value or `stage` times out.
async fn until<T>(stage: &str, mut check: impl FnMut() -> Option<T>) -> Result<T> {
    let deadline = Instant::now() + Duration::from_secs(6);
    loop {
        if let Some(value) = check() {
            return Ok(value);
        }
        if Instant::now() >= deadline {
            return Err(problem(format!("Timed out: {stage}")));
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// Parse the launcher's `PID DIRECTORY` line, accepting only a directory
/// `mktemp` created directly in /private/tmp with the canary prefix.
fn launched(output: &str) -> Option<(u32, PathBuf)> {
    let (pid, directory) = output.trim().split_once(' ')?;
    let directory = PathBuf::from(directory);
    let suffix = directory
        .file_name()?
        .to_str()?
        .strip_prefix("understudy-canary.")?;
    let canary = directory.parent() == Some(Path::new("/private/tmp"))
        && !suffix.is_empty()
        && suffix.bytes().all(|c| c.is_ascii_alphanumeric());
    let pid = pid.parse().ok().filter(|pid| *pid > 1)?;
    canary.then_some((pid, directory))
}

struct Canary<'a> {
    client: &'a Client,
    ownership: &'a Ownership,
    epoch: Epoch,
    fixture: Fixture,
    /// The step in progress, reported if a check fails there.
    stage: RefCell<String>,
    evidence: Vec<PathBuf>,
}

impl Canary<'_> {
    fn at(&self, stage: &str) {
        *self.stage.borrow_mut() = stage.to_owned();
    }
    fn stage(&self) -> String {
        self.stage.borrow().clone()
    }
    fn check(&self, kind: &str, outcome: Result<()>) -> Check {
        Check {
            kind: kind.to_owned(),
            passed: outcome.is_ok(),
            stage: outcome.as_ref().err().map(|_| self.stage()),
            error: outcome.err().map(|error| error.error),
        }
    }
    async fn run(&self, argv: Vec<String>) -> Result<Value> {
        self.client.run_owned(self.ownership, argv).await
    }
    async fn keys(&self, actions: Value) -> Result<()> {
        self.client
            .input_owned(self.ownership, &actions, &self.epoch)
            .await
            .map(|_| ())
    }
    async fn press(&self, key: &str) -> Result<()> {
        self.keys(json!([{ "type": "key", "key": key }])).await
    }
    async fn capture(&mut self, name: &str) {
        if let Ok(frame) = self.client.frame(Some(1600)).await
            && let Ok(directory) = self.client.output_dir()
        {
            let path = directory.join(format!("native-{}-{name}.png", timestamp()));
            if write_private(&path, &frame.png).is_ok() {
                self.evidence.push(path);
            }
        }
    }
    async fn ready(&self) -> Result<()> {
        self.at("fixture becomes ready");
        let fixture = &self.fixture;
        until(&self.stage(), || {
            let state = fixture.state();
            (state["pid"] == fixture.pid
                && state["windowX"].is_number()
                && state["panelInstrumentationVersion"] == 2)
                .then_some(())
        })
        .await
    }
    /// The helper's on-screen windows, front to back.
    async fn windows(&self) -> Result<Vec<Window>> {
        let probe = self.client.config.identity_probe.to_string_lossy();
        let reply = self
            .run(vec![probe.into_owned(), "windows-info".into()])
            .await?;
        run_output(&reply, 1 << 20)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
            .and_then(|value| serde_json::from_value(value["windows"].clone()).ok())
            .ok_or_else(|| problem("The helper window list is unavailable"))
    }
    fn display(&self) -> Bounds {
        Bounds {
            x: 0.0,
            y: 0.0,
            width: self.epoch.display.width,
            height: self.epoch.display.height,
        }
    }
    /// The fixture's ordinary window in a fresh window list.
    fn fixture_window(&self, windows: &[Window]) -> Option<i64> {
        let pid = i64::from(self.fixture.pid);
        windows
            .iter()
            .find(|window| window.pid == pid && window.layer == 0)
            .map(|window| window.number)
    }
    /// Click an uncovered part of the fixture's window until it is the
    /// frontmost app window. A fresh app launched from a daemon is not
    /// activated by macOS on its own, and a panel may have lost focus.
    async fn focus(&self) -> Result<()> {
        self.at("fixture window comes forward");
        let pid = i64::from(self.fixture.pid);
        let deadline = Instant::now() + Duration::from_secs(6);
        loop {
            let windows = self.windows().await?;
            if frontmost_pid(&windows) == Some(pid) {
                return Ok(());
            }
            let point = self
                .fixture_window(&windows)
                .and_then(|window| visible_point(&windows, window, &self.display()));
            if let Some((x, y)) = point {
                self.keys(json!([{ "type": "click", "x": x, "y": y }]))
                    .await?;
            }
            if Instant::now() >= deadline {
                return Err(problem("The fixture window did not come forward"));
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    }
    /// Centre the focused fixture with its own shortcut, so the text field
    /// and panels land on the display rather than wherever it opened.
    async fn center(&self) -> Result<()> {
        self.at("fixture window centred");
        let previous = self.fixture.state()["centerCount"].as_u64();
        self.press("ctrl+cmd+0").await?;
        let fixture = &self.fixture;
        until(&self.stage(), || {
            (fixture.state()["centerCount"].as_u64() > previous).then_some(())
        })
        .await
    }
    /// Escape any panel still presented and confirm it closed; a check never
    /// starts on top of a previous panel or sheet. The fixture is brought
    /// forward first so the Escape cannot reach another application.
    async fn dismissed(&self) -> Result<()> {
        if self.fixture.state()["panel"]["presented"] != true {
            return Ok(());
        }
        self.focus().await?;
        self.at("previous panel dismissed");
        for _ in 0..2 {
            if self.fixture.state()["panel"]["presented"] != true {
                return Ok(());
            }
            self.press("escape").await?;
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
        let fixture = &self.fixture;
        until(&self.stage(), || {
            (fixture.state()["panel"]["presented"] != true).then_some(())
        })
        .await
    }
    /// Open a panel with `shortcut` and return its new attempt ID once both
    /// AppKit and the AX observer show it focused with Cancel enabled.
    async fn open_panel(&self, kind: &str, shortcut: &str) -> Result<String> {
        let previous = self.fixture.state()["panel"]["attemptID"].clone();
        self.press(shortcut).await?;
        self.at(&format!("{kind} panel appears"));
        let fixture = &self.fixture;
        let attempt = until(&self.stage(), || {
            let panel = &fixture.state()["panel"];
            (panel["kind"] == kind && panel["attemptID"] != previous)
                .then(|| panel["attemptID"].as_str().map(str::to_owned))
                .flatten()
        })
        .await?;
        until(&self.stage(), || {
            let panel = &fixture.state()["panel"];
            (panel["presented"] == true
                && (panel["visible"] == true || !panel["attachedSheet"].is_null())
                && same_path(&panel["directory"], &fixture.directory))
            .then_some(())
        })
        .await?;
        self.at(&format!("{kind} panel focused"));
        until(&self.stage(), || {
            fixture
                .observed(&attempt)
                .filter(|o| o["panelExists"] == true && o["focusInPanel"] == true)
                .filter(|o| button(o, "Cancel").is_some_and(|b| b["enabled"] == true))
        })
        .await?;
        Ok(attempt)
    }
    async fn confirm(&self, kind: &str, attempt: &str, expected: &Path) -> Result<()> {
        if self.fixture.completion(attempt, kind).is_none() {
            self.at(&format!("{kind} confirmation enabled"));
            let fixture = &self.fixture;
            let title = if kind == "save" { "Save" } else { "Open" };
            until(&self.stage(), || {
                fixture
                    .observed(attempt)
                    .filter(|o| o["focusInPanel"] == true)
                    .filter(|o| button(o, title).is_some_and(|b| b["enabled"] == true))
            })
            .await?;
            self.press("return").await?;
        }
        self.at(&format!("{kind} completes"));
        let fixture = &self.fixture;
        let done = until(&self.stage(), || fixture.completion(attempt, kind)).await?;
        if done["response"] != 1 {
            return Err(problem(format!(
                "The {kind} panel did not accept the operation"
            )));
        }
        let key = if kind == "save" {
            "savedPath"
        } else {
            "openedPath"
        };
        until(&self.stage(), || {
            same_path(&fixture.state()[key], expected).then_some(())
        })
        .await
    }
    async fn save(&mut self) -> Result<()> {
        let expected = self.fixture.file("understudy-fixture.txt");
        self.at("fixture text receives focus and input");
        let state = self.fixture.state();
        let x = state["windowX"].as_f64().unwrap_or_default() + 100.0;
        let y = self.epoch.display.height - state["windowY"].as_f64().unwrap_or_default() - 520.0;
        // Never click or type unless the text field's point is the fixture's.
        let windows = self.windows().await?;
        let target = self.fixture_window(&windows);
        if !target.is_some_and(|window| unobstructed(&windows, window, x, y, &self.display())) {
            return Err(problem("The fixture text field is not uncovered"));
        }
        // The first click may only activate the window; the second focuses text.
        for _ in 0..2 {
            self.keys(json!([{ "type": "click", "x": x, "y": y }]))
                .await?;
        }
        if frontmost_pid(&self.windows().await?) != Some(i64::from(self.fixture.pid)) {
            return Err(problem("The fixture lost focus before typing"));
        }
        self.keys(json!([
            { "type": "key", "key": "cmd+a" },
            { "type": "type", "text": SAVE_TEXT },
        ]))
        .await?;
        let fixture = &self.fixture;
        until(&self.stage(), || {
            (fixture.state()["text"] == SAVE_TEXT).then_some(())
        })
        .await?;
        let attempt = self.open_panel("save", "cmd+s").await?;
        self.capture("save-panel").await;
        self.confirm("save", &attempt, &expected).await?;
        self.at("saved bytes match");
        if fs::read_to_string(&expected).ok().as_deref() != Some(SAVE_TEXT) {
            return Err(problem("The saved file does not contain the exact text"));
        }
        Ok(())
    }
    async fn open(&mut self) -> Result<()> {
        let expected = self.fixture.file("understudy-open-fixture.txt");
        self.at("open fixture file prepared");
        if fs::read_to_string(&expected).ok().as_deref() != Some(OPEN_TEXT) {
            return Err(problem("The fixture did not prepare its Open file"));
        }
        let attempt = self.open_panel("open", "cmd+o").await?;
        let fixture = &self.fixture;
        let before = fixture.observed(&attempt).map(|o| sheets(&o)).unwrap_or(0);
        self.press("cmd+shift+g").await?;
        self.at("Go to Folder field focused");
        let fixture = &self.fixture;
        until(&self.stage(), || {
            fixture.observed(&attempt).filter(|o| {
                sheets(o) > before
                    && ["AXTextField", "AXComboBox"]
                        .contains(&o["focused"]["role"].as_str().unwrap_or(""))
            })
        })
        .await?;
        let path = expected.to_string_lossy().into_owned();
        self.keys(json!([
            { "type": "key", "key": "cmd+a" },
            { "type": "type", "text": path },
        ]))
        .await?;
        self.at("Go to Folder receives the path");
        let fixture = &self.fixture;
        until(&self.stage(), || {
            fixture
                .observed(&attempt)
                .filter(|o| o["focused"]["value"] == path.as_str())
        })
        .await?;
        self.press("return").await?;
        self.capture("open-panel").await;
        self.confirm("open", &attempt, &expected).await?;
        self.at("opened text matches");
        if self.fixture.state()["openedText"] != OPEN_TEXT {
            return Err(problem("The fixture did not load the exact Open text"));
        }
        Ok(())
    }
    /// One independent check: no leftover panel, the fixture in front, then
    /// the operation itself.
    async fn attempt(&mut self, kind: &str) -> Check {
        let outcome = async {
            self.dismissed().await?;
            self.focus().await?;
            self.center().await?;
            if kind == "save" {
                self.save().await
            } else {
                self.open().await
            }
        }
        .await;
        self.check(kind, outcome)
    }
    /// Copy the fixture's own records next to the screenshots before cleanup
    /// removes its directory.
    fn archive(&mut self) -> Result<()> {
        let directory = self
            .client
            .output_dir()?
            .join(format!("native-{}", timestamp()));
        fs::DirBuilder::new().mode(0o700).create(&directory)?;
        for name in [
            "state.json",
            "events.jsonl",
            "panel-observer.json",
            "fixture.log",
            "understudy-fixture.txt",
            "understudy-open-fixture.txt",
        ] {
            match fs::read(self.fixture.file(name)) {
                Ok(bytes) => write_private(&directory.join(name), &bytes)?,
                // Saved output exists only if Save completed.
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
        }
        self.evidence.push(directory);
        Ok(())
    }
    /// Stop exactly the fixture this canary launched, confirm it exited, and
    /// only then remove its canary directory, unless its evidence could not
    /// be archived elsewhere.
    async fn cleanup(&self, remove: bool) -> Result<()> {
        let _ = self.dismissed().await;
        let script = concat!(
            "if kill -0 \"$1\" 2>/dev/null; then ",
            "[ \"$(/bin/ps -o comm= -p \"$1\")\" = \"$2\" ] || exit 3; ",
            "[ \"$(/bin/ps -o args= -p \"$1\")\" = \"$2 $3\" ] || exit 3; ",
            "kill -TERM \"$1\"; i=0; ",
            "while kill -0 \"$1\" 2>/dev/null; do ",
            "i=$((i+1)); [ $i -gt 50 ] && exit 4; sleep 0.1; done; ",
            "fi; ",
            "[ \"$4\" = remove ] || exit 0; ",
            "case \"$3\" in /private/tmp/understudy-canary.?*) /bin/rm -rf -- \"$3\";; ",
            "*) exit 5;; esac",
        );
        let reply = self
            .run(vec![
                "/bin/sh".into(),
                "-c".into(),
                script.into(),
                "sh".into(),
                self.fixture.pid.to_string(),
                self.fixture.program.to_string_lossy().into_owned(),
                self.fixture.directory.to_string_lossy().into_owned(),
                if remove { "remove" } else { "keep" }.into(),
            ])
            .await?;
        if reply["exitCode"] != 0 {
            return Err(problem(format!(
                "The canary fixture could not be stopped and removed (exit {})",
                reply["exitCode"]
            )));
        }
        Ok(())
    }
}

/// Start the fixture in a new canary directory; readiness is checked later so
/// a slow or outdated fixture is still cleaned up.
async fn launch(client: &Client, ownership: &Ownership, program: &Path) -> Result<Fixture> {
    let script = concat!(
        "d=$(/usr/bin/mktemp -d /private/tmp/understudy-canary.XXXXXX) || exit 1; ",
        "/bin/chmod 755 \"$d\" || exit 1; umask 022; ",
        "nohup \"$1\" \"$d\" >\"$d/fixture.log\" 2>&1 </dev/null & ",
        "echo \"$! $d\"",
    );
    let reply = client
        .run_owned(
            ownership,
            vec![
                "/bin/sh".into(),
                "-c".into(),
                script.into(),
                "sh".into(),
                program.to_string_lossy().into_owned(),
            ],
        )
        .await?;
    let (pid, directory) = run_output(&reply, 4096)
        .ok()
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .as_deref()
        .and_then(launched)
        .ok_or_else(|| problem("The helper could not launch the canary fixture"))?;
    Ok(Fixture {
        pid,
        program: program.to_owned(),
        directory,
    })
}

/// Run the native Save/Open canary and record the dated outcome.
pub async fn verify(client: &Client, fixture: &Path) -> Result<Value> {
    let uid = client.config.helper_uid;
    let ownership = client.controller.acquire(false)?;
    let status = client.status().await;
    let epoch = require_hidden(&status, uid)
        .ok()
        .filter(|session| session.input)
        .map(|session| session.epoch)
        .ok_or_else(|| {
            Error::new(
                "permission",
                "verify-native needs a hidden helper with ready input; see understudy doctor",
            )
        })?;
    let boot = boot_time().ok_or_else(|| problem("The boot time is unavailable"))?;
    let generation = Generation::of(&epoch, boot);
    let mut canary = Canary {
        client,
        ownership: &ownership,
        epoch,
        fixture: launch(client, &ownership, fixture).await?,
        stage: RefCell::new("fixture launched".into()),
        evidence: Vec::new(),
    };
    let checks = match canary.ready().await {
        Err(error) => vec![canary.check("launch", Err(error))],
        // Save and Open are independent observations.
        Ok(()) => vec![canary.attempt("save").await, canary.attempt("open").await],
    };
    canary.capture("after").await;
    let archived = canary.archive();
    let cleanup = canary.cleanup(archived.is_ok()).await;
    let after = generation_of(client, uid, boot).await;
    let unchanged = after.as_ref() == Some(&generation);
    let passed = unchanged && checks.len() == 2 && checks.iter().all(|check| check.passed);
    let record = Record {
        observed_at: now(),
        passed,
        checks,
        generation,
        evidence: canary.evidence.clone(),
    };
    write_record(uid, &record)?;
    let mut report = json!({
        "ok": true,
        "passed": record.passed,
        "checks": record.checks,
        "evidence": record.evidence,
        "record": record_path(uid)?,
    });
    if !unchanged {
        report["warning"] = json!("The helper session changed during the canary");
    }
    if let Err(error) = archived {
        report["archiveError"] = json!(error.error);
        report["fixtureDirectory"] = json!(canary.fixture.directory);
    }
    if let Err(error) = cleanup {
        report["cleanupError"] = json!(error.error);
    }
    Ok(report)
}

async fn generation_of(client: &Client, uid: u32, boot: u64) -> Option<Generation> {
    let session = require_hidden(&client.status().await, uid)
        .ok()
        .filter(|s| s.input)?;
    Some(Generation::of(&session.epoch, boot))
}

fn write_record(uid: u32, record: &Record) -> Result<()> {
    let path = record_path(uid)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    write_private(&path, &serde_json::to_vec_pretty(record)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn generation() -> Generation {
        Generation {
            capture_pid: 1,
            input_pid: 2,
            security_session_id: 3,
            display_id: 4,
            display: Display {
                width: 2560.0,
                height: 1440.0,
                scale: 1.0,
            },
            boot: 5,
        }
    }
    fn check(kind: &str, passed: bool) -> Check {
        Check {
            kind: kind.into(),
            passed,
            stage: (!passed).then(|| format!("{kind} confirmation enabled")),
            error: None,
        }
    }
    fn record(save: bool, open: bool, observed_at: u64) -> Record {
        Record {
            observed_at,
            passed: save && open,
            checks: vec![check("save", save), check("open", open)],
            generation: generation(),
            evidence: Vec::new(),
        }
    }

    #[test]
    fn observations_count_only_for_the_same_fresh_generation() {
        let current = generation();
        let state = |record: Option<&Record>, current: Option<&Generation>, at| {
            assess(record, current, at)["state"].clone()
        };
        assert_eq!(state(None, Some(&current), 100), "unknown");
        let passed = record(true, true, 100);
        assert_eq!(state(Some(&passed), Some(&current), 160), "passed");
        let failed = record(true, false, 100);
        assert_eq!(state(Some(&failed), Some(&current), 160), "failed");
        assert_eq!(state(Some(&passed), Some(&current), 100 + 31 * 60), "stale");
        let mut resized = generation();
        resized.display.width = 1920.0;
        assert_eq!(state(Some(&passed), Some(&resized), 160), "stale");
        let mut restarted = generation();
        restarted.input_pid = 9;
        assert_eq!(state(Some(&passed), Some(&restarted), 160), "stale");
        assert_eq!(state(Some(&passed), None, 160), "stale");
    }

    #[test]
    fn inconsistent_or_future_records_are_not_trusted() {
        let current = generation();
        let state = |record: &Record| assess(Some(record), Some(&current), 1000)["state"].clone();
        let mut save_only = record(true, true, 900);
        save_only.checks.pop();
        assert_eq!(state(&save_only), "unknown");
        let mut duplicated = record(true, true, 900);
        duplicated.checks[1] = check("save", true);
        assert_eq!(state(&duplicated), "unknown");
        let mut claimed = record(true, false, 900);
        claimed.passed = true;
        assert_eq!(state(&claimed), "unknown");
        let mut launch = record(false, false, 900);
        launch.checks = vec![check("launch", false)];
        assert_eq!(state(&launch), "unknown");
        assert_eq!(state(&record(true, true, 1000 + SKEW + 1)), "unknown");
        assert_eq!(state(&record(true, true, 1000 + SKEW)), "passed");
    }

    #[test]
    fn launcher_output_must_name_a_canary_directory() {
        assert_eq!(
            launched("123 /private/tmp/understudy-canary.Ab12Cd\n"),
            Some((123, "/private/tmp/understudy-canary.Ab12Cd".into()))
        );
        for output in [
            "123 /private/tmp/understudy-canary.",
            "123 /private/tmp/other.Ab12Cd",
            "123 /private/tmp/understudy-canary.x/nested",
            "123 /private/tmp/understudy-canary.a-b",
            "123 /tmp/understudy-canary.Ab12Cd",
            "1 /private/tmp/understudy-canary.Ab12Cd",
            "x /private/tmp/understudy-canary.Ab12Cd",
            "",
        ] {
            assert_eq!(launched(output), None, "{output:?}");
        }
    }

    #[test]
    fn canary_texts_match_the_fixture_bytes() {
        // tests/live/DesktopFixture.swift writes this exact Open text.
        let fixture = include_str!("../../tests/live/DesktopFixture.swift");
        assert!(fixture.contains(&OPEN_TEXT.replace('\n', "\\n")));
        assert!(SAVE_TEXT.ends_with('\u{e9}') && OPEN_TEXT.ends_with("e\u{301}\n"));
    }

    #[test]
    fn buttons_must_be_unique_and_sheets_are_counted() {
        let observed = json!({"controls": [
            {"role": "AXButton", "title": "Save", "enabled": true},
            {"role": "AXButton", "title": "Cancel", "enabled": true},
            {"role": "AXButton", "title": "Cancel", "enabled": false},
            {"role": "AXSheet"},
        ]});
        assert_eq!(button(&observed, "Save").unwrap()["enabled"], true);
        assert!(button(&observed, "Cancel").is_none());
        assert!(button(&observed, "Open").is_none());
        assert_eq!(sheets(&observed), 1);
    }
}

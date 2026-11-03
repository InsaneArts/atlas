use super::{
    Check, Generation, Panel, Record, boot_time, generation_at, now, problem, ready_epoch,
    record_path,
};
use crate::client::{Client, Run, timestamp};
use atlas_control::Ownership;
use atlas_core::{
    error::{Code, Error, Result},
    files::write_private,
    screen::windows::{Bounds, Screen, Shown, Window, frontmost_pid, unobstructed, visible_point},
    session::{Epoch, Observation},
    transport::run_output,
};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    fs,
    os::unix::fs::DirBuilderExt,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

// Explicit escapes: the fixture's Open file ends in a decomposed e + U+0301,
// which renders like the precomposed U+00E9 typed for Save but differs in bytes.
const SAVE_TEXT: &str = "Atlas native Save \u{2014} \u{10e5}\u{10d0}\u{10e0}\u{10d7}\u{10e3}\u{10da}\u{10d8} \u{65e5}\u{672c}\u{8a9e} \u{1f9d1}\u{1f3fd}\u{200d}\u{1f4bb} \u{e9}";

const OPEN_TEXT: &str = "Atlas native Open \u{2014} \u{10e5}\u{10d0}\u{10e0}\u{10d7}\u{10e3}\u{10da}\u{10d8} \u{65e5}\u{672c}\u{8a9e} \u{1f9d1}\u{1f3fd}\u{200d}\u{1f4bb} e\u{301}\n";

struct Fixture {
    pid: u32,
    program: PathBuf,
    directory: PathBuf,
}

/// What the fixture says of itself. It is read while it writes: a field
/// missing or of another type is unknown, never an error.
struct FixtureState {
    pid: Option<u64>,
    window_x: Option<f64>,
    window_y: Option<f64>,
    instrumentation: Option<i64>,
    center_count: Option<u64>,
    text: Option<String>,
    opened_text: Option<String>,
    saved_path: Option<String>,
    opened_path: Option<String>,
    panel: PanelState,
}

struct PanelState {
    presented: bool,
    visible: bool,
    sheet: bool,
    kind: Option<String>,
    attempt: Option<String>,
    directory: Option<String>,
}

struct PanelScan {
    panel_exists: bool,
    focus_in_panel: bool,
    focused_role: Option<String>,
    focused_value: Option<String>,
    controls: Vec<Control>,
}

struct Control {
    role: Option<String>,
    title: Option<String>,
    enabled: bool,
}

fn text(value: &Value) -> Option<String> {
    value.as_str().map(str::to_owned)
}

impl From<&Value> for FixtureState {
    fn from(state: &Value) -> Self {
        let panel = &state["panel"];
        Self {
            pid: state["pid"].as_u64(),
            window_x: state["windowX"].as_f64(),
            window_y: state["windowY"].as_f64(),
            instrumentation: state["panelInstrumentationVersion"].as_i64(),
            center_count: state["centerCount"].as_u64(),
            text: text(&state["text"]),
            opened_text: text(&state["openedText"]),
            saved_path: text(&state["savedPath"]),
            opened_path: text(&state["openedPath"]),
            panel: PanelState {
                presented: panel["presented"] == true,
                visible: panel["visible"] == true,
                sheet: !panel["attachedSheet"].is_null(),
                kind: text(&panel["kind"]),
                attempt: text(&panel["attemptID"]),
                directory: text(&panel["directory"]),
            },
        }
    }
}

impl From<&Value> for PanelScan {
    fn from(scan: &Value) -> Self {
        let controls = scan["controls"].as_array().into_iter().flatten();
        Self {
            panel_exists: scan["panelExists"] == true,
            focus_in_panel: scan["focusInPanel"] == true,
            focused_role: text(&scan["focused"]["role"]),
            focused_value: text(&scan["focused"]["value"]),
            controls: controls
                .map(|control| Control {
                    role: text(&control["role"]),
                    title: text(&control["title"]),
                    enabled: control["enabled"] == true,
                })
                .collect(),
        }
    }
}

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

impl Fixture {
    fn read(&self, name: &str) -> Option<Value> {
        serde_json::from_slice(&read_bounded(&self.directory.join(name), MAX_FIXTURE_RECORD).ok()?)
            .ok()
    }

    fn state(&self) -> FixtureState {
        FixtureState::from(&self.read("state.json").unwrap_or(Value::Null))
    }

    /// The panel observer's latest scan of `attempt`, if it is fresh.
    fn observed(&self, attempt: &str) -> Option<PanelScan> {
        let observed = self.read("panel-observer.json")?;
        let ms = atlas_core::time::now_ms();
        let finished = observed["finishedAt"].as_f64()? as u64;

        (observed["attemptID"] == attempt
            && observed["trusted"] == true
            && observed["parentPID"] == self.pid
            && observed["scanComplete"] == true
            && ms.saturating_sub(finished) <= 1500)
            .then(|| PanelScan::from(&observed))
    }

    /// Whether the panel `attempt` opened accepted its operation, once it closed.
    fn completion(&self, attempt: &str, kind: &str) -> Option<bool> {
        fixture_text(&self.directory.join("events.jsonl"))?
            .lines()
            .filter_map(|line| serde_json::from_str::<Value>(line).ok())
            .find(|event| {
                event["attemptID"] == attempt && event["event"] == format!("{kind}Closed")
            })
            .map(|event| event["response"] == 1)
    }
}

fn button<'a>(observed: &'a PanelScan, title: &str) -> Option<&'a Control> {
    let mut matches = observed.controls.iter().filter(|control| {
        control.role.as_deref() == Some("AXButton") && control.title.as_deref() == Some(title)
    });
    let first = matches.next();
    matches.next().is_none().then_some(first).flatten()
}

fn sheets(observed: &PanelScan) -> usize {
    observed
        .controls
        .iter()
        .filter(|control| control.role.as_deref() == Some("AXSheet"))
        .count()
}

fn same_path(left: Option<&str>, right: &Path) -> bool {
    left.and_then(|left| fs::canonicalize(left).ok())
        .is_some_and(|left| fs::canonicalize(right).is_ok_and(|right| left == right))
}

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
        .strip_prefix("atlas-canary.")?;
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
            (state.pid == Some(u64::from(fixture.pid))
                && state.window_x.is_some()
                && state.instrumentation == Some(2))
            .then_some(())
        })
        .await
    }

    /// The helper's on-screen windows, front to back, and where its Dock
    /// takes clicks.
    async fn windows(&self) -> Result<Shown> {
        self.client
            .windows_in(None, true, Duration::from_secs(5))
            .await
    }

    fn screen(&self, shown: &Shown) -> Screen {
        let display = Bounds::display(self.epoch.display.width, self.epoch.display.height);
        Screen::new(display, shown.dock.clone())
    }

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
            let shown = self.windows().await?;
            if frontmost_pid(&shown.windows) == Some(pid) {
                return Ok(());
            }

            let point = self
                .fixture_window(&shown.windows)
                .and_then(|window| visible_point(&shown.windows, window, &self.screen(&shown)));
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
        let previous = self.fixture.state().center_count;
        self.press("ctrl+cmd+0").await?;
        let fixture = &self.fixture;

        until(&self.stage(), || {
            (fixture.state().center_count > previous).then_some(())
        })
        .await
    }

    /// Escape any panel still presented and confirm it closed; a check never
    /// starts on top of a previous panel or sheet. The fixture is brought
    /// forward first so the Escape cannot reach another application.
    async fn dismissed(&self) -> Result<()> {
        if !self.fixture.state().panel.presented {
            return Ok(());
        }

        self.focus().await?;
        self.at("previous panel dismissed");

        for _ in 0..2 {
            if !self.fixture.state().panel.presented {
                return Ok(());
            }

            self.press("escape").await?;
            tokio::time::sleep(Duration::from_millis(500)).await;
        }

        let fixture = &self.fixture;
        until(&self.stage(), || {
            (!fixture.state().panel.presented).then_some(())
        })
        .await
    }

    /// Open `panel` with its shortcut and return its new attempt ID once both
    /// AppKit and the AX observer show it focused with Cancel enabled.
    async fn open_panel(&self, panel: Panel) -> Result<String> {
        let kind = panel.kind();
        let shortcut = match panel {
            Panel::Save => "cmd+s",
            Panel::Open => "cmd+o",
        };
        let previous = self.fixture.state().panel.attempt;
        self.press(shortcut).await?;

        self.at(&format!("{kind} panel appears"));
        let fixture = &self.fixture;
        let attempt = until(&self.stage(), || {
            let panel = fixture.state().panel;
            (panel.kind.as_deref() == Some(kind) && panel.attempt != previous)
                .then_some(panel.attempt)
                .flatten()
        })
        .await?;

        until(&self.stage(), || {
            let panel = fixture.state().panel;
            (panel.presented
                && (panel.visible || panel.sheet)
                && same_path(panel.directory.as_deref(), &fixture.directory))
            .then_some(())
        })
        .await?;

        self.at(&format!("{kind} panel focused"));
        until(&self.stage(), || {
            fixture
                .observed(&attempt)
                .filter(|o| o.panel_exists && o.focus_in_panel)
                .filter(|o| button(o, "Cancel").is_some_and(|b| b.enabled))
        })
        .await?;

        Ok(attempt)
    }

    async fn confirm(&self, panel: Panel, attempt: &str, expected: &Path) -> Result<()> {
        let kind = panel.kind();
        let title = match panel {
            Panel::Save => "Save",
            Panel::Open => "Open",
        };
        let fixture = &self.fixture;

        if fixture.completion(attempt, kind).is_none() {
            self.at(&format!("{kind} confirmation enabled"));
            until(&self.stage(), || {
                fixture
                    .observed(attempt)
                    .filter(|o| o.focus_in_panel)
                    .filter(|o| button(o, title).is_some_and(|b| b.enabled))
            })
            .await?;

            self.press("return").await?;
        }

        self.at(&format!("{kind} completes"));
        let accepted = until(&self.stage(), || fixture.completion(attempt, kind)).await?;
        if !accepted {
            return Err(problem(format!(
                "The {kind} panel did not accept the operation"
            )));
        }

        until(&self.stage(), || {
            let state = fixture.state();
            let path = match panel {
                Panel::Save => state.saved_path,
                Panel::Open => state.opened_path,
            };
            same_path(path.as_deref(), expected).then_some(())
        })
        .await
    }

    async fn save(&mut self) -> Result<()> {
        let expected = self.fixture.directory.join("atlas-fixture.txt");
        self.at("fixture text receives focus and input");
        let state = self.fixture.state();
        let x = state.window_x.unwrap_or_default() + 100.0;
        let y = self.epoch.display.height - state.window_y.unwrap_or_default() - 520.0;

        // Never click or type unless the text field's point is the fixture's.
        let shown = self.windows().await?;
        let screen = self.screen(&shown);
        let target = self.fixture_window(&shown.windows);
        if !target.is_some_and(|window| unobstructed(&shown.windows, window, x, y, &screen)) {
            return Err(problem("The fixture text field is not uncovered"));
        }

        // The first click may only activate the window; the second focuses text.
        for _ in 0..2 {
            self.keys(json!([{ "type": "click", "x": x, "y": y }]))
                .await?;
        }

        if frontmost_pid(&self.windows().await?.windows) != Some(i64::from(self.fixture.pid)) {
            return Err(problem("The fixture lost focus before typing"));
        }

        self.keys(json!([
            { "type": "key", "key": "cmd+a" },
            { "type": "type", "text": SAVE_TEXT },
        ]))
        .await?;

        let fixture = &self.fixture;
        until(&self.stage(), || {
            (fixture.state().text.as_deref() == Some(SAVE_TEXT)).then_some(())
        })
        .await?;

        let attempt = self.open_panel(Panel::Save).await?;
        self.capture("save-panel").await;
        self.confirm(Panel::Save, &attempt, &expected).await?;

        self.at("saved bytes match");
        if fixture_text(&expected).as_deref() != Some(SAVE_TEXT) {
            return Err(problem("The saved file does not contain the exact text"));
        }

        Ok(())
    }

    async fn open(&mut self) -> Result<()> {
        let expected = self.fixture.directory.join("atlas-open-fixture.txt");
        self.at("open fixture file prepared");
        if fixture_text(&expected).as_deref() != Some(OPEN_TEXT) {
            return Err(problem("The fixture did not prepare its Open file"));
        }

        let attempt = self.open_panel(Panel::Open).await?;
        let fixture = &self.fixture;
        let before = fixture.observed(&attempt).map(|o| sheets(&o)).unwrap_or(0);

        self.press("cmd+shift+g").await?;
        self.at("Go to Folder field focused");
        until(&self.stage(), || {
            fixture.observed(&attempt).filter(|o| {
                sheets(o) > before
                    && ["AXTextField", "AXComboBox"]
                        .contains(&o.focused_role.as_deref().unwrap_or(""))
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
        until(&self.stage(), || {
            fixture
                .observed(&attempt)
                .filter(|o| o.focused_value.as_deref() == Some(path.as_str()))
        })
        .await?;

        self.press("return").await?;
        self.capture("open-panel").await;
        self.confirm(Panel::Open, &attempt, &expected).await?;

        self.at("opened text matches");
        if self.fixture.state().opened_text.as_deref() != Some(OPEN_TEXT) {
            return Err(problem("The fixture did not load the exact Open text"));
        }

        Ok(())
    }

    /// One independent check: no leftover panel, the fixture in front, then
    /// the operation itself.
    async fn attempt(&mut self, panel: Panel) -> Check {
        let outcome = async {
            self.dismissed().await?;
            self.focus().await?;
            self.center().await?;
            match panel {
                Panel::Save => self.save().await,
                Panel::Open => self.open().await,
            }
        }
        .await;

        self.check(panel.kind(), outcome)
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
            "atlas-fixture.txt",
            "atlas-open-fixture.txt",
        ] {
            match read_bounded(&self.fixture.directory.join(name), MAX_ARCHIVED) {
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
            "case \"$3\" in /private/tmp/atlas-canary.?*) /bin/rm -rf -- \"$3\";; ",
            "*) exit 5;; esac",
        );

        let argv = vec![
            "/bin/sh".into(),
            "-c".into(),
            script.into(),
            "sh".into(),
            self.fixture.pid.to_string(),
            self.fixture.program.to_string_lossy().into_owned(),
            self.fixture.directory.to_string_lossy().into_owned(),
            if remove { "remove" } else { "keep" }.into(),
        ];
        let reply = self.client.run(argv, Run::owned(self.ownership)).await?;
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
        "d=$(/usr/bin/mktemp -d /private/tmp/atlas-canary.XXXXXX) || exit 1; ",
        "/bin/chmod 755 \"$d\" || exit 1; umask 022; ",
        "nohup \"$1\" \"$d\" >\"$d/fixture.log\" 2>&1 </dev/null & ",
        "echo \"$! $d\"",
    );

    let reply = client
        .run(
            vec![
                "/bin/sh".into(),
                "-c".into(),
                script.into(),
                "sh".into(),
                program.to_string_lossy().into_owned(),
            ],
            Run::owned(ownership),
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
///
/// # Errors
///
/// Returns fixture, ownership, identity, launch or record-storage failures. Individual
/// panel checks and archive/cleanup failures are retained in the returned report. This
/// actively exercises the fixture; inspect passed and cleanupError before claiming proof.
pub async fn verify(client: &Client, fixture: &Path) -> Result<Value> {
    // A development check: its test app is built from a checkout, never shipped.
    if !fixture.is_file() {
        return Err(problem(format!(
            "verify-native is a development check: it needs Atlas's test app at {}, built from \
             tests/live/DesktopFixture.swift in a checkout.",
            fixture.display()
        )));
    }

    let uid = client.config.helper_uid;
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
        // Each panel is an independent observation.
        Ok(()) => {
            let mut checks = Vec::new();
            for panel in Panel::ALL {
                checks.push(canary.attempt(panel).await);
            }
            checks
        }
    };

    canary.capture("after").await;
    let archived = canary.archive();
    let cleanup = canary.cleanup(archived.is_ok()).await;

    let after = generation_at(&Observation::decode(&client.status().await), uid, boot);
    let unchanged = after.as_ref() == Some(&generation);
    let passed =
        unchanged && checks.len() == Panel::ALL.len() && checks.iter().all(|check| check.passed);
    let record = Record {
        observed_at: now(),
        passed,
        checks,
        generation,
        evidence: std::mem::take(&mut canary.evidence),
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

    if let Err(error) = archived {
        report["archiveError"] = json!(error.error);
        report["fixtureDirectory"] = json!(canary.fixture.directory);
    }

    if let Err(error) = cleanup {
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
mod tests;

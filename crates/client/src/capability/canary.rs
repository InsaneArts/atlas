//! The native Save/Open check, driven the same way on every desktop: a
//! fresh copy of the maintained AppKit fixture is launched in a canary
//! folder of its own, its real NSSavePanel and NSOpenPanel are driven with
//! guarded keyboard input, exact file bytes are checked, its evidence is
//! archived and it is cleaned up. What differs between desktops, how the
//! fixture is launched, how its files and scans are read, where input and
//! pictures go and how two paths are compared, is the [`Desk`]'s: Second
//! Login's reads this Mac's disk, a VM's asks its guest.
use crate::client::Client;
use atlas_core::{
    error::{Code, Error, Result},
    screen::windows::{Bounds, Screen, Shown, Window, frontmost_pid, unobstructed, visible_point},
    session::Display,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

// Explicit escapes: the fixture's Open file ends in a decomposed e + U+0301,
// which renders like the precomposed U+00E9 typed for Save but differs in bytes.
pub(crate) const SAVE_TEXT: &str = "Atlas native Save \u{2014} \u{10e5}\u{10d0}\u{10e0}\u{10d7}\u{10e3}\u{10da}\u{10d8} \u{65e5}\u{672c}\u{8a9e} \u{1f9d1}\u{1f3fd}\u{200d}\u{1f4bb} \u{e9}";

pub(crate) const OPEN_TEXT: &str = "Atlas native Open \u{2014} \u{10e5}\u{10d0}\u{10e0}\u{10d7}\u{10e3}\u{10da}\u{10d8} \u{65e5}\u{672c}\u{8a9e} \u{1f9d1}\u{1f3fd}\u{200d}\u{1f4bb} e\u{301}\n";

/// The fixture's files a check keeps as its evidence.
pub(crate) const ARCHIVED: [&str; 6] = [
    "state.json",
    "events.jsonl",
    "panel-observer.json",
    "fixture.log",
    "atlas-fixture.txt",
    "atlas-open-fixture.txt",
];

/// How long one stage of a check waits for what it looks for.
const STAGE: Duration = Duration::from_secs(6);

/// Start the fixture given as `$1` in a new canary folder, detached, and
/// say its PID and folder.
const LAUNCH: &str = concat!(
    "d=$(/usr/bin/mktemp -d /private/tmp/atlas-canary.XXXXXX) || exit 1; ",
    "/bin/chmod 755 \"$d\" || exit 1; umask 022; ",
    "nohup \"$1\" \"$d\" >\"$d/fixture.log\" 2>&1 </dev/null & ",
    "echo \"$! $d\"",
);

/// Stop exactly the fixture `$1`, running `$2` in `$3`, confirm it exited,
/// and only then remove its canary folder when `$4` says so.
const CLEANUP: &str = concat!(
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

/// One native panel a check drives.
#[derive(Clone, Copy)]
pub(crate) enum Panel {
    Save,
    Open,
}

impl Panel {
    /// Every panel a passing record covers, in the order they run.
    pub(crate) const ALL: [Self; 2] = [Self::Save, Self::Open];

    /// The name the fixture and the record know this panel by.
    pub(crate) fn kind(self) -> &'static str {
        match self {
            Self::Save => "save",
            Self::Open => "open",
        }
    }
}

/// One native operation's outcome and, if it failed, where.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Check {
    pub(crate) kind: String,
    pub(crate) passed: bool,
    pub(crate) stage: Option<String>,
    pub(crate) error: Option<String>,
}

impl Check {
    /// Exactly one check of every panel in `checks`, and `passed` only if
    /// all passed.
    pub(crate) fn consistent(checks: &[Self], passed: bool) -> bool {
        let covered = |panel: Panel| checks.iter().any(|check| check.kind == panel.kind());

        checks.len() == Panel::ALL.len()
            && Panel::ALL.into_iter().all(covered)
            && passed == checks.iter().all(|check| check.passed)
    }
}

pub(crate) fn problem(message: impl Into<String>) -> Error {
    Error::new(Code::Native, message)
}

/// The fixture a check launched: its process, the program it runs and its
/// canary folder, as the desktop it runs on names them.
pub(crate) struct Fixture {
    pub(crate) pid: u32,
    pub(crate) program: PathBuf,
    pub(crate) directory: PathBuf,
}

/// What the fixture says of itself. It is read while it writes: a field
/// missing or of another type is unknown, never an error.
pub(crate) struct FixtureState {
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

/// The fixture's scan of a panel through the accessibility tree.
pub(crate) struct PanelScan {
    panel_exists: bool,
    focus_in_panel: bool,
    focused_role: Option<String>,
    focused_value: Option<String>,
    controls: Vec<Control>,
}

/// One control of a panel, as the fixture's scan saw it.
pub(crate) struct Control {
    role: Option<String>,
    title: Option<String>,
    pub(crate) enabled: bool,
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

/// Whether `observed` is a whole, trusted scan of the panel `attempt` by
/// the observer of the fixture `pid`.
pub(crate) fn scanned(observed: &Value, attempt: &str, pid: u32) -> bool {
    observed["attemptID"] == attempt
        && observed["trusted"] == true
        && observed["parentPID"] == pid
        && observed["scanComplete"] == true
}

pub(crate) fn button<'a>(observed: &'a PanelScan, title: &str) -> Option<&'a Control> {
    let mut matches = observed.controls.iter().filter(|control| {
        control.role.as_deref() == Some("AXButton") && control.title.as_deref() == Some(title)
    });
    let first = matches.next();
    matches.next().is_none().then_some(first).flatten()
}

pub(crate) fn sheets(observed: &PanelScan) -> usize {
    observed
        .controls
        .iter()
        .filter(|control| control.role.as_deref() == Some("AXSheet"))
        .count()
}

/// Parse the launcher's `PID DIRECTORY` line, accepting only a directory
/// `mktemp` created directly in /private/tmp with the canary prefix.
pub(crate) fn launched(output: &str) -> Option<(u32, PathBuf)> {
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

/// The command that launches `program` in a new canary folder.
pub(crate) fn launching(program: &Path) -> Vec<String> {
    vec![
        "/bin/sh".into(),
        "-c".into(),
        LAUNCH.into(),
        "sh".into(),
        program.to_string_lossy().into_owned(),
    ]
}

/// The command that stops `fixture` and removes its folder when `remove`.
pub(crate) fn cleaning(fixture: &Fixture, remove: bool) -> Vec<String> {
    vec![
        "/bin/sh".into(),
        "-c".into(),
        CLEANUP.into(),
        "sh".into(),
        fixture.pid.to_string(),
        fixture.program.to_string_lossy().into_owned(),
        fixture.directory.to_string_lossy().into_owned(),
        if remove { "remove" } else { "keep" }.into(),
    ]
}

/// The cleanup's refusal, by the exit code its command answered with.
pub(crate) fn cleaned(reply: &Value) -> Result<()> {
    if reply["exitCode"] != 0 {
        return Err(problem(format!(
            "The canary fixture could not be stopped and removed (exit {})",
            reply["exitCode"]
        )));
    }
    Ok(())
}

/// A desktop the check runs on: how its fixture is launched and cleaned
/// up, how its files and its panel scans are read, where its input and
/// pictures go, and how it compares two of its paths.
pub(crate) trait Desk {
    /// The display the fixture's window is on, in points.
    fn display(&self) -> &Display;

    /// Start the fixture in a new canary folder; readiness is checked
    /// later so a slow or outdated fixture is still cleaned up.
    async fn launch(&self) -> Result<Fixture>;

    /// The fixture's file `name`, a regular file read within a bound, as
    /// text; none when it is not there or cannot be read.
    async fn text(&self, fixture: &Fixture, name: &str) -> Option<String>;

    /// What the fixture says of itself now.
    async fn state(&self, fixture: &Fixture) -> FixtureState {
        let state = self
            .text(fixture, "state.json")
            .await
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or(Value::Null);
        FixtureState::from(&state)
    }

    /// The panel observer's scan of `attempt`, when it is fresh by this
    /// desktop's rule; a refusal ends the stage that asked.
    async fn observed(&self, fixture: &Fixture, attempt: &str) -> Result<Option<PanelScan>>;

    /// Whether the panel `attempt` opened accepted its operation, once it
    /// closed.
    async fn completion(&self, fixture: &Fixture, attempt: &str, kind: &str) -> Option<bool> {
        self.text(fixture, "events.jsonl")
            .await?
            .lines()
            .filter_map(|line| serde_json::from_str::<Value>(line).ok())
            .find(|event| {
                event["attemptID"] == attempt && event["event"] == format!("{kind}Closed")
            })
            .map(|event| event["response"] == 1)
    }

    /// Send `actions` as the check's input, guarded by its session.
    async fn keys(&self, actions: Value) -> Result<()>;

    /// The on-screen windows, front to back, and where the Dock takes
    /// clicks.
    async fn windows(&self) -> Result<Shown>;

    /// A picture of the screen kept as evidence named `name`, when one
    /// could be taken.
    async fn capture(&self, name: &str) -> Option<PathBuf>;

    /// Keep the fixture's own records as evidence, before cleanup removes
    /// its folder: where they were kept.
    async fn archive(&self, fixture: &Fixture) -> Result<Vec<PathBuf>>;

    /// Stop exactly `fixture`, confirm it exited, and only then remove its
    /// canary folder when `remove`.
    async fn cleanup(&self, fixture: &Fixture, remove: bool) -> Result<()>;

    /// Whether `path`, as `fixture` said it, names `expected`, the canary
    /// folder or a file in it.
    async fn same_path(&self, fixture: &Fixture, path: Option<&str>, expected: &Path) -> bool;

    /// Why the stage `stage` ended without what it looked for.
    fn timed_out(&self, stage: &str) -> Error {
        problem(format!("Timed out: {stage}"))
    }
}

/// What one check came to on a desktop.
pub(crate) struct Ran {
    pub(crate) checks: Vec<Check>,
    pub(crate) evidence: Vec<PathBuf>,
    pub(crate) archived: Result<()>,
    pub(crate) cleanup: Result<()>,
    /// The fixture's canary folder, kept when its evidence was not.
    pub(crate) directory: PathBuf,
}

/// Run the check on `desk`: launch the fixture, drive each panel as an
/// independent observation, keep the evidence, then clean up.
///
/// # Errors
///
/// Returns the launch's failure; every later failure is in what it ran to.
pub(crate) async fn run<D: Desk>(desk: &D) -> Result<Ran> {
    let mut canary = Canary {
        desk,
        fixture: desk.launch().await?,
        stage: RefCell::new("fixture launched".into()),
        evidence: Vec::new(),
    };

    let checks = match canary.ready().await {
        Err(error) => vec![canary.check("launch", Err(error))],
        Ok(()) => {
            let mut checks = Vec::new();
            for panel in Panel::ALL {
                checks.push(canary.attempt(panel).await);
            }
            checks
        }
    };

    canary.capture("after").await;
    let archived = desk
        .archive(&canary.fixture)
        .await
        .map(|kept| canary.evidence.extend(kept));
    let _ = canary.dismissed().await;
    let cleanup = desk.cleanup(&canary.fixture, archived.is_ok()).await;

    Ok(Ran {
        checks,
        evidence: canary.evidence,
        archived,
        cleanup,
        directory: canary.fixture.directory,
    })
}

struct Canary<'a, D> {
    desk: &'a D,
    fixture: Fixture,
    /// The step in progress, reported if a check fails there.
    stage: RefCell<String>,
    evidence: Vec<PathBuf>,
}

impl<D: Desk> Canary<'_, D> {
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

    /// What `check` finds, looked for again until it does or the stage's
    /// bound passes; a refusal of a look ends it at once.
    async fn until<T>(&self, mut check: impl AsyncFnMut() -> Result<Option<T>>) -> Result<T> {
        let deadline = Instant::now() + STAGE;

        loop {
            if let Some(value) = check().await? {
                return Ok(value);
            }

            if Instant::now() >= deadline {
                return Err(self.desk.timed_out(&self.stage()));
            }

            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    async fn state(&self) -> FixtureState {
        self.desk.state(&self.fixture).await
    }

    async fn observed(&self, attempt: &str) -> Result<Option<PanelScan>> {
        self.desk.observed(&self.fixture, attempt).await
    }

    async fn press(&self, key: &str) -> Result<()> {
        self.desk.keys(json!([{ "type": "key", "key": key }])).await
    }

    async fn capture(&mut self, name: &str) {
        if let Some(path) = self.desk.capture(name).await {
            self.evidence.push(path);
        }
    }

    async fn ready(&self) -> Result<()> {
        self.at("fixture becomes ready");
        let pid = u64::from(self.fixture.pid);
        self.until(async || {
            let state = self.state().await;
            Ok((state.pid == Some(pid)
                && state.window_x.is_some()
                && state.instrumentation == Some(2))
            .then_some(()))
        })
        .await
    }

    fn screen(&self, shown: &Shown) -> Screen {
        let display = self.desk.display();
        Screen::new(
            Bounds::display(display.width, display.height),
            shown.dock.clone(),
        )
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
        let deadline = Instant::now() + STAGE;

        loop {
            let shown = self.desk.windows().await?;
            if frontmost_pid(&shown.windows) == Some(pid) {
                return Ok(());
            }

            let point = self
                .fixture_window(&shown.windows)
                .and_then(|window| visible_point(&shown.windows, window, &self.screen(&shown)));
            if let Some((x, y)) = point {
                self.desk
                    .keys(json!([{ "type": "click", "x": x, "y": y }]))
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
        let previous = self.state().await.center_count;
        self.press("ctrl+cmd+0").await?;

        self.until(async || Ok((self.state().await.center_count > previous).then_some(())))
            .await
    }

    /// Escape any panel still presented and confirm it closed; a check never
    /// starts on top of a previous panel or sheet. The fixture is brought
    /// forward first so the Escape cannot reach another application.
    async fn dismissed(&self) -> Result<()> {
        if !self.state().await.panel.presented {
            return Ok(());
        }

        self.focus().await?;
        self.at("previous panel dismissed");

        for _ in 0..2 {
            if !self.state().await.panel.presented {
                return Ok(());
            }

            self.press("escape").await?;
            tokio::time::sleep(Duration::from_millis(500)).await;
        }

        self.until(async || Ok((!self.state().await.panel.presented).then_some(())))
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
        let previous = self.state().await.panel.attempt;
        self.press(shortcut).await?;

        self.at(&format!("{kind} panel appears"));
        let attempt = self
            .until(async || {
                let panel = self.state().await.panel;
                Ok(
                    (panel.kind.as_deref() == Some(kind) && panel.attempt != previous)
                        .then_some(panel.attempt)
                        .flatten(),
                )
            })
            .await?;

        let directory = &self.fixture.directory;
        self.until(async || {
            let panel = self.state().await.panel;
            let shown = panel.presented && (panel.visible || panel.sheet);
            Ok((shown
                && self
                    .desk
                    .same_path(&self.fixture, panel.directory.as_deref(), directory)
                    .await)
                .then_some(()))
        })
        .await?;

        self.at(&format!("{kind} panel focused"));
        self.until(async || {
            Ok(self
                .observed(&attempt)
                .await?
                .filter(|o| o.panel_exists && o.focus_in_panel)
                .filter(|o| button(o, "Cancel").is_some_and(|b| b.enabled)))
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

        if self
            .desk
            .completion(&self.fixture, attempt, kind)
            .await
            .is_none()
        {
            self.at(&format!("{kind} confirmation enabled"));
            self.until(async || {
                Ok(self
                    .observed(attempt)
                    .await?
                    .filter(|o| o.focus_in_panel)
                    .filter(|o| button(o, title).is_some_and(|b| b.enabled)))
            })
            .await?;

            self.press("return").await?;
        }

        self.at(&format!("{kind} completes"));
        let accepted = self
            .until(async || Ok(self.desk.completion(&self.fixture, attempt, kind).await))
            .await?;
        if !accepted {
            return Err(problem(format!(
                "The {kind} panel did not accept the operation"
            )));
        }

        self.until(async || {
            let state = self.state().await;
            let path = match panel {
                Panel::Save => state.saved_path,
                Panel::Open => state.opened_path,
            };
            Ok(self
                .desk
                .same_path(&self.fixture, path.as_deref(), expected)
                .await
                .then_some(()))
        })
        .await
    }

    async fn save(&mut self) -> Result<()> {
        let expected = self.fixture.directory.join("atlas-fixture.txt");
        self.at("fixture text receives focus and input");
        let state = self.state().await;
        let x = state.window_x.unwrap_or_default() + 100.0;
        let y = self.desk.display().height - state.window_y.unwrap_or_default() - 520.0;

        // Never click or type unless the text field's point is the fixture's.
        let shown = self.desk.windows().await?;
        let screen = self.screen(&shown);
        let target = self.fixture_window(&shown.windows);
        if !target.is_some_and(|window| unobstructed(&shown.windows, window, x, y, &screen)) {
            return Err(problem("The fixture text field is not uncovered"));
        }

        // The first click may only activate the window; the second focuses text.
        for _ in 0..2 {
            self.desk
                .keys(json!([{ "type": "click", "x": x, "y": y }]))
                .await?;
        }

        if frontmost_pid(&self.desk.windows().await?.windows) != Some(i64::from(self.fixture.pid)) {
            return Err(problem("The fixture lost focus before typing"));
        }

        self.desk
            .keys(json!([
                { "type": "key", "key": "cmd+a" },
                { "type": "type", "text": SAVE_TEXT },
            ]))
            .await?;

        self.until(async || {
            Ok((self.state().await.text.as_deref() == Some(SAVE_TEXT)).then_some(()))
        })
        .await?;

        let attempt = self.open_panel(Panel::Save).await?;
        self.capture("save-panel").await;
        self.confirm(Panel::Save, &attempt, &expected).await?;

        self.at("saved bytes match");
        if self
            .desk
            .text(&self.fixture, "atlas-fixture.txt")
            .await
            .as_deref()
            != Some(SAVE_TEXT)
        {
            return Err(problem("The saved file does not contain the exact text"));
        }

        Ok(())
    }

    async fn open(&mut self) -> Result<()> {
        let expected = self.fixture.directory.join("atlas-open-fixture.txt");
        self.at("open fixture file prepared");
        if self
            .desk
            .text(&self.fixture, "atlas-open-fixture.txt")
            .await
            .as_deref()
            != Some(OPEN_TEXT)
        {
            return Err(problem("The fixture did not prepare its Open file"));
        }

        let attempt = self.open_panel(Panel::Open).await?;
        let before = self
            .observed(&attempt)
            .await?
            .map(|o| sheets(&o))
            .unwrap_or(0);

        self.press("cmd+shift+g").await?;
        self.at("Go to Folder field focused");
        self.until(async || {
            Ok(self.observed(&attempt).await?.filter(|o| {
                sheets(o) > before
                    && ["AXTextField", "AXComboBox"]
                        .contains(&o.focused_role.as_deref().unwrap_or(""))
            }))
        })
        .await?;

        let path = expected.to_string_lossy().into_owned();
        self.desk
            .keys(json!([
                { "type": "key", "key": "cmd+a" },
                { "type": "type", "text": path },
            ]))
            .await?;

        self.at("Go to Folder receives the path");
        self.until(async || {
            Ok(self
                .observed(&attempt)
                .await?
                .filter(|o| o.focused_value.as_deref() == Some(path.as_str())))
        })
        .await?;

        self.press("return").await?;
        self.capture("open-panel").await;
        self.confirm(Panel::Open, &attempt, &expected).await?;

        self.at("opened text matches");
        if self.state().await.opened_text.as_deref() != Some(OPEN_TEXT) {
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
}

/// Run the native Save/Open check on `client`'s desktop with the fixture
/// at `fixture`, a path on this Mac, and record its dated outcome: in the
/// second login, as it runs there; on a VM, copied into its guest first.
///
/// # Errors
///
/// Returns fixture, ownership, identity, launch or record-storage failures. Individual
/// panel checks and archive/cleanup failures are retained in the returned report. This
/// actively exercises the fixture; inspect passed and cleanupError before claiming proof.
pub async fn verify(client: &Client, fixture: &Path) -> Result<Value> {
    atlas_core::own_folder::refuse(fixture)?;
    // A development check: its test app is built from a checkout, never shipped.
    if !fixture.is_file() {
        return Err(problem(format!(
            "verify-native is a development check: it needs Atlas's test app at {}, built from \
             tests/live/DesktopFixture.swift in a checkout.",
            fixture.display()
        )));
    }
    match client.desktop().kind() {
        atlas_core::desktop::Kind::User(_) => {
            crate::user::capability::canary::verify(client, fixture).await
        }
        atlas_core::desktop::Kind::Vm(_) => crate::vm::native::verify(client, fixture).await,
    }
}

#[cfg(test)]
#[path = "canary_tests.rs"]
mod tests;

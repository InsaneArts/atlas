use super::{Check, Generation, Record, boot_time, generation_at, now, problem, record_path};
use crate::{
    client::{Client, Run, timestamp},
    control::Ownership,
    error::{Code, Error, Result},
    files::write_private,
    screen::windows::{Bounds, Screen, Shown, Window, frontmost_pid, unobstructed, visible_point},
    session::{Epoch, require_hidden},
    transport::run_output,
};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    fs,
    os::unix::fs::DirBuilderExt,
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

// Explicit escapes: the fixture's Open file ends in a decomposed e + U+0301,
// which renders like the precomposed U+00E9 typed for Save but differs in bytes.
const SAVE_TEXT: &str = "Atlas native Save \u{2014} \u{10e5}\u{10d0}\u{10e0}\u{10d7}\u{10e3}\u{10da}\u{10d8} \u{65e5}\u{672c}\u{8a9e} \u{1f9d1}\u{1f3fd}\u{200d}\u{1f4bb} \u{e9}";

const OPEN_TEXT: &str = "Atlas native Open \u{2014} \u{10e5}\u{10d0}\u{10e0}\u{10d7}\u{10e3}\u{10da}\u{10d8} \u{65e5}\u{672c}\u{8a9e} \u{1f9d1}\u{1f3fd}\u{200d}\u{1f4bb} e\u{301}\n";

/// A fresh fixture process running in the helper.
struct Fixture {
    pid: u32,
    program: PathBuf,
    directory: PathBuf,
}

/// The most read of one fixture record while polling it.
const MAX_FIXTURE_RECORD: u64 = 1 << 20;

/// The most archived of one fixture file.
const MAX_ARCHIVED: u64 = 16 << 20;

/// A regular file the fixture wrote in the helper, read whole up to `limit`
/// bytes: links, pipes and devices are refused, and a larger file is an
/// error rather than read without end.
fn read_bounded(path: &Path, limit: u64) -> std::io::Result<Vec<u8>> {
    use std::{io::Read, os::unix::fs::OpenOptionsExt};

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

    let mut bytes = Vec::new();
    file.take(limit + 1).read_to_end(&mut bytes)?;

    if bytes.len() as u64 > limit {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("{} exceeds {limit} bytes", path.display()),
        ));
    }

    Ok(bytes)
}

/// A small text file the fixture wrote, if it is one.
fn fixture_text(path: &Path) -> Option<String> {
    String::from_utf8(read_bounded(path, MAX_FIXTURE_RECORD).ok()?).ok()
}

impl Fixture {
    fn read(&self, name: &str) -> Option<Value> {
        serde_json::from_slice(&read_bounded(&self.directory.join(name), MAX_FIXTURE_RECORD).ok()?)
            .ok()
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
        fixture_text(&self.directory.join("events.jsonl"))?
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

    async fn run(&self, argv: Vec<String>) -> Result<Value> {
        self.client.run(argv, Run::owned(self.ownership)).await
    }

    async fn keys(&self, actions: Value) -> Result<()> {
        self.client
            .input_owned(
                self.ownership,
                crate::actions::Batch::decode(&actions),
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
            (state["pid"] == fixture.pid
                && state["windowX"].is_number()
                && state["panelInstrumentationVersion"] == 2)
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

    /// The helper's display, with its Dock as `shown` says.
    fn screen(&self, shown: &Shown) -> Screen {
        let display = Bounds::display(self.epoch.display.width, self.epoch.display.height);
        Screen::new(display, shown.dock.clone())
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
        let fixture = &self.fixture;

        if fixture.completion(attempt, kind).is_none() {
            self.at(&format!("{kind} confirmation enabled"));
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
        let expected = self.fixture.file("atlas-fixture.txt");
        self.at("fixture text receives focus and input");
        let state = self.fixture.state();
        let x = state["windowX"].as_f64().unwrap_or_default() + 100.0;
        let y = self.epoch.display.height - state["windowY"].as_f64().unwrap_or_default() - 520.0;

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
            (fixture.state()["text"] == SAVE_TEXT).then_some(())
        })
        .await?;

        let attempt = self.open_panel("save", "cmd+s").await?;
        self.capture("save-panel").await;
        self.confirm("save", &attempt, &expected).await?;

        self.at("saved bytes match");
        if fixture_text(&expected).as_deref() != Some(SAVE_TEXT) {
            return Err(problem("The saved file does not contain the exact text"));
        }

        Ok(())
    }

    async fn open(&mut self) -> Result<()> {
        let expected = self.fixture.file("atlas-open-fixture.txt");
        self.at("open fixture file prepared");
        if fixture_text(&expected).as_deref() != Some(OPEN_TEXT) {
            return Err(problem("The fixture did not prepare its Open file"));
        }

        let attempt = self.open_panel("open", "cmd+o").await?;
        let fixture = &self.fixture;
        let before = fixture.observed(&attempt).map(|o| sheets(&o)).unwrap_or(0);

        self.press("cmd+shift+g").await?;
        self.at("Go to Folder field focused");
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
            "atlas-fixture.txt",
            "atlas-open-fixture.txt",
        ] {
            match read_bounded(&self.fixture.file(name), MAX_ARCHIVED) {
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
        return Err(Error::new(
            Code::Native,
            format!(
                "verify-native is a development check: it needs Atlas's test app at {}, built \
                 from tests/live/DesktopFixture.swift in a checkout.",
                fixture.display()
            ),
        ));
    }

    let uid = client.config.helper_uid;
    let ownership = client.controller.acquire(false)?;
    let status = client.status().await;
    let epoch = require_hidden(&status, uid)
        .ok()
        .filter(|session| session.input)
        .map(|session| session.epoch)
        .ok_or_else(|| {
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
        // Save and Open are independent observations.
        Ok(()) => vec![canary.attempt("save").await, canary.attempt("open").await],
    };

    canary.capture("after").await;
    let archived = canary.archive();
    let cleanup = canary.cleanup(archived.is_ok()).await;

    let after = generation_at(&client.status().await, uid, boot);
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
mod tests {
    use super::*;
    #[test]
    fn launcher_output_must_name_a_canary_directory() {
        assert_eq!(
            launched("123 /private/tmp/atlas-canary.Ab12Cd\n"),
            Some((123, "/private/tmp/atlas-canary.Ab12Cd".into()))
        );

        for output in [
            "123 /private/tmp/atlas-canary.",
            "123 /private/tmp/other.Ab12Cd",
            "123 /private/tmp/atlas-canary.x/nested",
            "123 /private/tmp/atlas-canary.a-b",
            "123 /tmp/atlas-canary.Ab12Cd",
            "1 /private/tmp/atlas-canary.Ab12Cd",
            "x /private/tmp/atlas-canary.Ab12Cd",
            "",
        ] {
            assert_eq!(launched(output), None, "{output:?}");
        }
    }

    #[test]
    fn canary_texts_match_the_fixture_bytes() {
        // tests/live/DesktopFixture.swift writes this exact Open text.
        let fixture = include_str!("../../../tests/live/DesktopFixture.swift");
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

    /// The fixture's files are the helper's: read within a bound, and only
    /// as regular files.
    #[test]
    fn fixture_files_are_read_within_a_bound() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.json");
        fs::write(&path, b"{\"ok\":true}").unwrap();

        assert_eq!(read_bounded(&path, 11).unwrap(), b"{\"ok\":true}");
        assert!(read_bounded(&path, 10).is_err());

        let link = dir.path().join("link");
        std::os::unix::fs::symlink(&path, &link).unwrap();
        assert!(read_bounded(&link, 1024).is_err());
        assert!(read_bounded(dir.path(), 1024).is_err());

        assert_eq!(fixture_text(&path).as_deref(), Some("{\"ok\":true}"));
        assert_eq!(fixture_text(&dir.path().join("missing")), None);
    }
}

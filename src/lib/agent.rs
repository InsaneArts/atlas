//! Agent-facing commands: small, stable verbs over the guarded input path,
//! the helper's window list, accessibility elements, app launching and
//! waiting for windows. Every action is an ordinary validated batch with the
//! usual ownership and epoch checks; nothing here bypasses them.
use crate::{
    actions::Button,
    client::Client,
    error::{Error, Result},
    handoff,
    native::{
        ax::{Element, Frame},
        windows::{Bounds, Window, unobstructed},
    },
    transport::run_output,
};
use serde_json::{Value, json};
use std::time::{Duration, Instant};
use unicode_segmentation::UnicodeSegmentation;

/// The working contract printed by `atlas agent-guide`.
pub const GUIDE: &str = include_str!("agent_guide.md");

/// When a skill loads: what Atlas is for, in the words an agent matches.
const SKILL_DESCRIPTION: &str = "See and use macOS GUIs in a hidden second login (the helper) \
without touching the person's desktop, windows, keyboard or mouse. Use it whenever you change \
something a person could see in a Mac app (a window, dialog, menu, menu bar item, layout or \
icon), open the build in the helper and check it before saying it works. Also use it to \
reproduce on-screen bugs, run installers or anything that opens windows or takes focus, test \
a web app in Safari, take screenshots, and click or type in apps. Not for unit tests, builds \
or reading files.";

/// The rules every agent follows, set between the guide's introduction and
/// its first section in the plugin's skill.
const SKILL_RULES: &str = "## Rules

- Never act on the person's own desktop; everything happens in the helper.
- Check your on-screen work in the helper before saying it works, and show the
  screenshots.
- Do the work yourself: opening builds, copying files in, cleaning up. The
  person only ran `atlas init`.
- Never use sudo, type a password or change privacy settings. If `doctor` is
  not ready, run the Atlas commands it suggests; tell the person only what
  needs them (or use the setup skill).
- Never take control from the person: on `busy`, wait.
- Ask the person before `atlas switch`, and before anything that locks, sleeps
  or logs out.
- After an `uncertain` result, never repeat the action: take a screenshot, run
  `atlas restart`, and look again before going on.
- Leave the helper as you found it: close what you opened, roll back what you
  copied.

";

/// The agent plugin's skill: the guide with its rules. The plugin's SKILL.md
/// is this text (a test keeps them equal).
pub fn skill() -> String {
    let guide = GUIDE
        .split_once('\n')
        .map_or(GUIDE, |(_, rest)| rest)
        .trim_start();
    let (introduction, sections) = guide.split_once("## ").unwrap_or(("", guide));
    format!(
        "---\nname: atlas\ndescription: \"{SKILL_DESCRIPTION}\"\n---\n\n# Atlas\n\n\
{introduction}{SKILL_RULES}## {sections}\n\
## References\n\n\
- [references/commands.md](references/commands.md): every command and its options.\n\
- [references/troubleshooting.md](references/troubleshooting.md): what each error \
code means and what to do.\n"
    )
}

/// The longest text a single `type` action accepts.
const TYPE_LIMIT: usize = 256;

fn button(button: Button) -> &'static str {
    match button {
        Button::Left => "left",
        Button::Middle => "middle",
        Button::Right => "right",
    }
}

pub fn click(x: f64, y: f64, which: Button, count: u8, modifiers: &[String]) -> Value {
    json!([{
        "type": "click",
        "x": x,
        "y": y,
        "button": button(which),
        "count": count,
        "modifiers": modifiers,
    }])
}

pub fn drag(from: (f64, f64), to: (f64, f64), which: Button) -> Value {
    json!([{
        "type": "drag",
        "fromX": from.0,
        "fromY": from.1,
        "toX": to.0,
        "toY": to.1,
        "button": button(which),
    }])
}

/// Text as `type` batches of at most 256 characters, never splitting a
/// grapheme cluster (an emoji sequence or a letter with combining marks).
pub fn type_batches(text: &str) -> Result<Vec<String>> {
    let mut batches = Vec::new();
    let mut chunk = String::new();
    let mut length = 0;
    for grapheme in text.graphemes(true) {
        let size = grapheme.chars().count();
        if size > TYPE_LIMIT {
            return Err(Error::new(
                "input",
                "A single character cluster is too long to type",
            ));
        }
        if length + size > TYPE_LIMIT {
            batches.push(std::mem::take(&mut chunk));
            length = 0;
        }
        chunk.push_str(grapheme);
        length += size;
    }
    if !chunk.is_empty() {
        batches.push(chunk);
    }
    if batches.is_empty() {
        return Err(Error::new("input", "Provide text to type"));
    }
    Ok(batches)
}

/// Type `text` batch by batch, stopping at the first batch that is not
/// confirmed. The report says exactly how much was confirmed typed and, on
/// failure, whether the failed batch's delivery is uncertain.
pub async fn type_text(client: &Client, text: &str) -> Result<Value> {
    let batches = type_batches(text)?;
    let (mut typed, mut confirmed) = (0, 0);
    for batch in &batches {
        let outcome = client
            .input(&json!([{ "type": "type", "text": batch }]), None, None)
            .await;
        let (failure, uncertain) = match outcome {
            Ok(reply) if reply["ok"] == true => {
                confirmed += 1;
                typed += batch.chars().count();
                continue;
            }
            Ok(reply) => (
                reply["error"]
                    .as_str()
                    .unwrap_or("The helper did not confirm the text")
                    .to_owned(),
                reply["uncertain"] == true,
            ),
            Err(error) => (error.error, error.uncertain),
        };
        return Ok(json!({
            "ok": false,
            "typed": typed,
            "batches": batches.len(),
            "confirmedBatches": confirmed,
            "error": failure,
            "uncertain": uncertain,
        }));
    }
    Ok(json!({ "ok": true, "typed": typed, "batches": batches.len() }))
}

/// Keys pressed in order, as one batch; an unknown key is named.
pub fn keys(keys: &[String]) -> Result<Value> {
    if let Some(unknown) = keys.iter().find(|key| !crate::actions::valid_key(key)) {
        return Err(Error::new(
            "usage",
            format!(
                "{unknown} is not a key Atlas knows. Use a letter, digit or symbol, a name such \
                 as return, tab, escape, space, delete, up or f5, with modifiers like cmd+s or \
                 cmd+shift+z."
            ),
        ));
    }
    Ok(Value::Array(
        keys.iter()
            .map(|key| json!({ "type": "key", "key": key }))
            .collect(),
    ))
}

/// Which helper windows a command is about.
#[derive(Debug, Default, Clone)]
pub struct WindowFilter {
    /// Owning application name, compared case-insensitively.
    pub app: Option<String>,
    /// Text the window title must contain, compared case-insensitively.
    pub title: Option<String>,
    /// Include menu bar, Dock and other non-document layers.
    pub all: bool,
}

impl WindowFilter {
    pub fn matches(&self, window: &Window) -> bool {
        let lower = |text: &Option<String>| text.as_deref().map(str::to_lowercase);
        // With --app, an app's floating panels and alerts count as its own.
        (self.all || window.layer == 0 || (self.app.is_some() && app_layer(window)))
            && self
                .app
                .as_ref()
                .is_none_or(|app| lower(&window.owner) == Some(app.to_lowercase()))
            && self.title.as_ref().is_none_or(|title| {
                lower(&window.title).is_some_and(|own| own.contains(&title.to_lowercase()))
            })
    }
}

/// The helper's on-screen windows, front to back, through a read-only probe.
pub async fn windows(client: &Client) -> Result<Vec<Window>> {
    windows_within(client, Duration::from_secs(5)).await
}

/// The window list, with the helper probe bounded by `limit`.
async fn windows_within(client: &Client, limit: Duration) -> Result<Vec<Window>> {
    let probe = client.config.identity_probe.to_string_lossy().into_owned();
    let reply = client
        .inspect_within(vec![probe, "windows-info".into()], limit)
        .await?;
    run_output(&reply, 1 << 20)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .filter(|value| value["ok"] == true)
        .and_then(|value| serde_json::from_value(value["windows"].clone()).ok())
        .ok_or_else(|| Error::new("windows", "The helper window list is unavailable"))
}

pub async fn list(client: &Client, filter: &WindowFilter) -> Result<Value> {
    let windows = windows(client).await?;
    let matching: Vec<_> = windows
        .into_iter()
        .filter(|window| filter.matches(window))
        .collect();
    Ok(json!({ "ok": true, "windows": matching }))
}

/// `open` inside the helper session: an app by name, files or URLs, or both.
pub async fn open(
    client: &Client,
    app: Option<&str>,
    targets: &[String],
    environment: &[String],
) -> Result<Value> {
    if app.is_none() && targets.is_empty() {
        return Err(Error::new(
            "usage",
            "Name an app with --app, a file or URL, or both",
        ));
    }
    let mut argv = vec!["/usr/bin/open".to_owned()];
    for variable in environment {
        if variable
            .split_once('=')
            .is_none_or(|(name, _)| name.is_empty())
        {
            return Err(Error::new(
                "usage",
                format!("--env takes NAME=VALUE, not {variable}"),
            ));
        }
        argv.extend(["--env".to_owned(), variable.clone()]);
    }
    // Copying, launching and pruning happen under one ownership, so no other
    // open, close or controller interleaves.
    let ownership = client.controller.acquire(false)?;
    let apps = client.apps().await?;
    handoff::prune(&app_names(&apps));
    let running: Vec<handoff::Running> = apps["apps"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|app| handoff::Running {
            name: app["name"].as_str().unwrap_or("").to_owned(),
            executable: app["executable"].as_str().unwrap_or("").to_owned(),
        })
        .collect();
    // A name is the helper's to look up; a path is this side's file.
    let app_target = app.map(|app| {
        if app.contains('/') {
            handoff::resolve(app)
        } else {
            handoff::Target::Name(app.to_owned())
        }
    });
    let mut wanted: Vec<handoff::Target> = app_target.into_iter().collect();
    wanted.extend(targets.iter().map(|target| handoff::resolve(target)));
    let mut copied = Vec::new();
    let mut given = handoff::hand_over(client, &ownership, wanted, &running, &mut copied)
        .await?
        .into_iter();
    if app.is_some() {
        argv.extend(["-a".to_owned(), given.next().unwrap_or_default()]);
    }
    if !targets.is_empty() {
        argv.push("--".to_owned());
        argv.extend(given);
    }
    let reply = client.run_owned(&ownership, argv).await?;
    let opened = reply["exitCode"] == 0;
    let mut value = json!({ "ok": opened, "opened": opened });
    if !copied.is_empty() {
        value["copied"] = json!(copied);
    }
    if !opened {
        let output = run_output(&reply, 4096).unwrap_or_default();
        value["error"] = json!(String::from_utf8_lossy(&output).trim());
    }
    Ok(value)
}

fn app_names(apps: &Value) -> Vec<&str> {
    apps["apps"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|app| app["name"].as_str())
        .collect()
}

/// Quit `app` in the helper (every process of that name), wait until it is
/// gone, then remove the copies `open` made of it.
pub async fn close(client: &Client, app: &str) -> Result<Value> {
    if app.contains('/') {
        return Err(Error::new(
            "usage",
            "close takes the app's name as atlas apps lists it, not a path",
        ));
    }
    let ownership = client.controller.acquire(false)?;
    let running = |apps: &Value| -> Vec<(u64, String)> {
        apps["apps"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|running| running["name"] == app)
            .filter_map(|running| {
                Some((
                    running["pid"].as_u64()?,
                    running["executable"].as_str().unwrap_or("").to_owned(),
                ))
            })
            .collect()
    };
    let processes = running(&client.apps().await?);
    // The helper's session and Atlas's own services are never closed.
    let own = std::env::current_exe()
        .ok()
        .and_then(|exe| crate::update::app_of(&exe))
        .map(|bundle| bundle.to_string_lossy().into_owned());
    if processes.iter().any(|(_, executable)| {
        executable.starts_with("/System/Library/CoreServices/")
            || own
                .as_deref()
                .is_some_and(|bundle| executable.starts_with(bundle))
    }) {
        return Err(Error::new(
            "input",
            format!("Atlas does not close {app}: it is part of macOS or of Atlas"),
        ));
    }
    if !processes.is_empty() {
        // In the helper, each process is signalled only while it is still
        // the executable listed, so a reused process ID is left alone.
        const QUIT: &str = r#"while [ $# -ge 2 ]; do
  pid=$1 exe=$2; shift 2
  [ -n "$exe" ] && [ "$(/bin/ps -o comm= -p "$pid")" = "$exe" ] && /bin/kill "$pid"
done
exit 0"#;
        let mut argv = vec![
            "/bin/sh".to_owned(),
            "-c".to_owned(),
            QUIT.to_owned(),
            "sh".to_owned(),
        ];
        for (pid, executable) in &processes {
            argv.extend([pid.to_string(), executable.clone()]);
        }
        client.run_owned(&ownership, argv).await?;
        let deadline = Instant::now() + Duration::from_secs(10);
        while !running(&client.apps().await?).is_empty() {
            if Instant::now() >= deadline {
                return Err(Error::new(
                    "wait-failed",
                    format!("{app} did not quit within 10 seconds"),
                ));
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    }
    let removed = handoff::remove_copies_of(app);
    Ok(json!({
        "ok": true,
        "app": app,
        "closed": !processes.is_empty(),
        "removed": removed,
    }))
}

/// Poll the window list until a matching window appears (or, with `gone`,
/// until none remains). The timeout is a deadline: an observation counts only
/// if it completed before it, and a zero timeout means one observation.
pub async fn wait(
    client: &Client,
    filter: &WindowFilter,
    gone: bool,
    timeout: Duration,
) -> Result<Value> {
    if filter.app.is_none() && filter.title.is_none() {
        return Err(Error::new("usage", "Wait for --app, --title, or both"));
    }
    let started = Instant::now();
    let deadline = started + timeout;
    loop {
        // Each observation gets what is left of the deadline (at least half
        // a second); a zero timeout is exactly one observation.
        let remaining = deadline.saturating_duration_since(Instant::now());
        let budget = if timeout.is_zero() {
            Duration::from_secs(5)
        } else {
            remaining.max(Duration::from_millis(500))
        };
        let matching: Vec<_> = windows_within(client, budget)
            .await?
            .into_iter()
            .filter(|window| filter.matches(window))
            .collect();
        let observed = Instant::now();
        if matching.is_empty() == gone && (timeout.is_zero() || observed <= deadline) {
            return Ok(json!({
                "ok": true,
                "waitedMs": started.elapsed().as_millis() as u64,
                "windows": matching,
            }));
        }
        if timeout.is_zero() || observed >= deadline {
            return Ok(json!({
                "ok": false,
                "timedOut": true,
                "error": if gone {
                    "The window did not close in time"
                } else {
                    "No matching window appeared in time"
                },
            }));
        }
        tokio::time::sleep((deadline - observed).min(Duration::from_millis(250))).await;
    }
}

/// Which accessibility elements a command is about.
#[derive(Debug, Default, Clone)]
pub struct ElementFilter {
    /// AX role, for example `AXButton`, compared case-insensitively.
    pub role: Option<String>,
    /// Exact title, label text, description or identifier, compared case-insensitively.
    pub name: Option<String>,
}

impl ElementFilter {
    /// Whether `element` could match once attributes it failed to read are
    /// treated as unknown.
    pub fn might_match(&self, element: &Element) -> bool {
        let unknown = |attribute: &str| element.unreadable.iter().any(|a| a == attribute);
        let same = |left: &Option<String>, right: &str, attribute: &str| {
            unknown(attribute)
                || left
                    .as_deref()
                    .is_some_and(|left| left.eq_ignore_ascii_case(right))
        };
        self.role
            .as_deref()
            .is_none_or(|role| same(&element.role, role, "AXRole"))
            && self.name.as_deref().is_none_or(|name| {
                same(&element.title, name, "AXTitle")
                    || same(&element.description, name, "AXDescription")
                    || same(&element.identifier, name, "AXIdentifier")
                    || label(element).is_some_and(|text| text.eq_ignore_ascii_case(name))
            })
    }
    pub fn matches(&self, element: &Element) -> bool {
        let same = |left: &Option<String>, right: &str| {
            left.as_deref()
                .is_some_and(|left| left.eq_ignore_ascii_case(right))
        };
        self.role
            .as_deref()
            .is_none_or(|role| same(&element.role, role))
            && self.name.as_deref().is_none_or(|name| {
                same(&element.title, name)
                    || same(&element.description, name)
                    || same(&element.identifier, name)
                    || label(element).is_some_and(|text| text.eq_ignore_ascii_case(name))
            })
    }
}

/// The text of a plain label (SwiftUI and AppKit put it in the value of a
/// static text), which names it like a title. A text field's value is typed
/// content, never a name.
fn label(element: &Element) -> Option<&str> {
    (element.role.as_deref() == Some("AXStaticText"))
        .then_some(element.value.as_deref())
        .flatten()
}

/// Layers from the Dock's up hold the Dock, the menu bar and system overlays,
/// never an app's own windows.
const DOCK_LAYER: i64 = 20;

/// Whether `window` is one of an app's own visible windows: an ordinary one,
/// or a floating panel or alert below the Dock's layer.
fn app_layer(window: &Window) -> bool {
    window.layer == 0 || ((1..DOCK_LAYER).contains(&window.layer) && window.alpha > 0.0)
}

/// The app an element command is about: a PID, or a name that must resolve
/// to exactly one process with a window of its own on screen.
#[derive(Debug, Clone)]
pub enum Target {
    Name(String),
    Pid(i64),
}

fn app_pid(windows: &[Window], target: &Target) -> Result<i64> {
    let name = match target {
        Target::Pid(pid) => return Ok(*pid),
        Target::Name(name) => name,
    };
    let candidates: Vec<&Window> = windows
        .iter()
        .filter(|window| {
            app_layer(window)
                && window
                    .owner
                    .as_deref()
                    .is_some_and(|owner| owner.eq_ignore_ascii_case(name))
        })
        .collect();
    // Its ordinary windows first; an app showing only a floating panel or an
    // alert is found by those.
    let ordinary: Vec<i64> = candidates
        .iter()
        .filter(|window| window.layer == 0)
        .map(|window| window.pid)
        .collect();
    let mut pids = if ordinary.is_empty() {
        candidates.iter().map(|window| window.pid).collect()
    } else {
        ordinary
    };
    pids.sort_unstable();
    pids.dedup();
    match pids.as_slice() {
        [pid] => Ok(*pid),
        [] => Err(Error::new(
            "elements",
            format!("No window of {name} is on screen"),
        )),
        _ => Err(Error::new(
            "elements",
            format!("Several {name} processes have windows ({pids:?}); pass --pid"),
        )),
    }
}

async fn app_elements(client: &Client, pid: i64) -> Result<(Vec<Element>, bool)> {
    let probe = client.config.identity_probe.to_string_lossy().into_owned();
    let reply = client
        .inspect(vec![probe, "elements-info".into(), pid.to_string()])
        .await?;
    let value = run_output(&reply, 8 << 20)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .ok_or_else(|| Error::new("elements", "The accessibility probe returned no result"))?;
    if value["ok"] != true {
        return Err(Error::new(
            "elements",
            value["error"]
                .as_str()
                .unwrap_or("The accessibility probe failed")
                .to_owned(),
        ));
    }
    let elements = serde_json::from_value(value["elements"].clone())
        .map_err(|_| Error::new("elements", "Malformed accessibility elements"))?;
    Ok((elements, value["truncated"] == true))
}

/// Accessibility elements of the target app's windows that match `filter`.
pub async fn elements(
    client: &Client,
    target: &Target,
    filter: &ElementFilter,
    max: usize,
) -> Result<Value> {
    let pid = app_pid(&windows(client).await?, target)?;
    let (all, incomplete) = app_elements(client, pid).await?;
    let total = all.len();
    let matching: Vec<_> = all
        .into_iter()
        .filter(|element| filter.matches(element))
        .collect();
    let count = matching.len();
    Ok(json!({
        "ok": true,
        "pid": pid,
        "scanned": total,
        "scanIncomplete": incomplete,
        "matching": count,
        "elements": matching.into_iter().take(max).collect::<Vec<_>>(),
    }))
}

/// The CoreGraphics window whose bounds are an element's AX window frame,
/// or None when no window or more than one window of the process matches.
fn owning_window(windows: &[Window], pid: i64, frame: &Frame) -> Option<i64> {
    let near = |a: f64, b: f64| (a - b).abs() <= 1.0;
    let matching: Vec<&Window> = windows
        .iter()
        .filter(|window| {
            window.pid == pid
                && app_layer(window)
                && near(window.bounds.x, frame.x)
                && near(window.bounds.y, frame.y)
                && near(window.bounds.width, frame.width)
                && near(window.bounds.height, frame.height)
        })
        .collect();
    // An ordinary window wins over a same-sized overlay of the same app.
    let unique = |windows: Vec<&Window>| match windows.as_slice() {
        [window] => Some(window.number),
        _ => None,
    };
    unique(
        matching
            .iter()
            .copied()
            .filter(|window| window.layer == 0)
            .collect(),
    )
    .or_else(|| unique(matching))
}

/// Click the centre of exactly one matching element. Uniqueness needs a
/// complete scan; the point must be uncovered in the element's own window in
/// a window list read after the scan; and the click is posted under one
/// ownership and the epoch observed at the start.
pub async fn click_element(
    client: &Client,
    target: &Target,
    filter: &ElementFilter,
    which: Button,
    count: u8,
    modifiers: &[String],
) -> Result<Value> {
    if filter.role.is_none() && filter.name.is_none() {
        return Err(Error::new(
            "input",
            "Select the element with --name, --role or both",
        ));
    }
    let ownership = client.controller.acquire(false)?;
    let epoch = client.identity().await?.epoch;
    let pid = app_pid(&windows(client).await?, target)?;
    let (elements, incomplete) = app_elements(client, pid).await?;
    if incomplete {
        return Err(Error::new(
            "elements",
            "The accessibility scan was incomplete, so a unique match cannot be established",
        ));
    }
    let matching: Vec<_> = elements
        .iter()
        .filter(|element| filter.matches(element) && element.frame.is_some())
        .collect();
    let [element] = matching.as_slice() else {
        return Err(Error::new(
            "elements",
            match matching.len() {
                0 => "No visible element matches. Run atlas elements --app NAME to see \
                      what is there."
                    .to_owned(),
                count => format!(
                    "{count} elements match. Run atlas elements with the same --name to \
                     tell them apart, then add --role."
                ),
            },
        ));
    };
    // An element whose selector attributes could not be read may be a
    // second, unseen match.
    let doubtful = elements.iter().any(|other| {
        other.index != element.index && !other.unreadable.is_empty() && filter.might_match(other)
    });
    if doubtful {
        return Err(Error::new(
            "elements",
            "Another element's name could not be read, so this match may not be unique",
        ));
    }
    let (x, y) = element
        .frame
        .as_ref()
        .map(Frame::center)
        .unwrap_or_default();
    let display = Bounds {
        x: 0.0,
        y: 0.0,
        width: epoch.display.width,
        height: epoch.display.height,
    };
    // A fresh list, read after the scan, judged for the element's own window.
    let windows = windows(client).await?;
    let window = element
        .window
        .as_ref()
        .and_then(|frame| owning_window(&windows, pid, frame));
    if !window.is_some_and(|window| unobstructed(&windows, window, x, y, &display)) {
        return Err(Error::new(
            "elements",
            "The element is not visible in its window: covered, moved or off screen",
        ));
    }
    let actions = click(x, y, which, count, modifiers);
    let mut result = client.input_owned(&ownership, &actions, &epoch).await?;
    result["element"] = json!(element);
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::Display;

    fn display() -> Display {
        Display {
            width: 2560.0,
            height: 1440.0,
            scale: 1.0,
        }
    }

    fn window(number: i64, pid: i64, owner: &str, layer: i64) -> Window {
        Window {
            number,
            pid,
            owner: Some(owner.into()),
            title: None,
            layer,
            alpha: 1.0,
            bounds: Bounds {
                x: 10.0,
                y: 20.0,
                width: 300.0,
                height: 200.0,
            },
        }
    }

    #[test]
    fn the_guide_names_only_real_commands() {
        for command in [
            "doctor",
            "readiness",
            "screenshot",
            "windows",
            "elements",
            "click",
            "type",
            "key",
            "drag",
            "scroll",
            "open",
            "wait",
            "run",
            "restart",
            "close",
        ] {
            assert!(GUIDE.contains(&format!("atlas {command}")), "{command}");
        }
    }

    /// Agents read the skill's description from YAML front matter: it stays
    /// one double-quoted line with nothing that would end or break it.
    #[test]
    fn the_skill_description_is_valid_front_matter() {
        assert!(!SKILL_DESCRIPTION.contains(['"', '\\', '\n']));
        assert!(skill().starts_with(&format!(
            "---\nname: atlas\ndescription: \"{SKILL_DESCRIPTION}\"\n---\n"
        )));
    }

    #[test]
    fn sugar_produces_valid_actions() {
        for actions in [
            click(
                10.0,
                20.0,
                Button::Right,
                2,
                &["cmd".into(), "shift".into()],
            ),
            drag((1.0, 2.0), (300.0, 400.0), Button::Left),
            keys(&["cmd+s".into(), "return".into()]).unwrap(),
        ] {
            crate::actions::validate(&actions, &display()).unwrap();
        }
    }

    #[test]
    fn typing_splits_long_text_on_grapheme_boundaries() {
        let family = "👨‍👩‍👧‍👦";
        let text = format!("{}{family}{}", "a".repeat(250), "b".repeat(300));
        let batches = type_batches(&text).unwrap();
        assert_eq!(batches.concat(), text);
        assert!(batches.iter().all(|t| t.chars().count() <= TYPE_LIMIT));
        assert!(
            batches.iter().any(|t| t.contains(family)),
            "cluster kept whole"
        );
        for batch in &batches {
            crate::actions::validate(&json!([{ "type": "type", "text": batch }]), &display())
                .unwrap();
        }
        let decomposed = "e\u{301}".repeat(200);
        let batches = type_batches(&decomposed).unwrap();
        assert!(batches.iter().all(|t| !t.starts_with('\u{301}')));
        assert_eq!(batches.concat(), decomposed);
        assert!(type_batches("").is_err());
    }

    #[test]
    fn window_filters_match_app_title_and_layer() {
        let mut edit = window(1, 2, "TextEdit", 0);
        edit.title = Some("Untitled — Edited".into());
        let mut menu = window(3, 4, "Window Server", 24);
        menu.title = Some("Menubar".into());
        let by_app = WindowFilter {
            app: Some("textedit".into()),
            ..Default::default()
        };
        assert!(by_app.matches(&edit));
        let by_title = WindowFilter {
            title: Some("untitled".into()),
            ..Default::default()
        };
        assert!(by_title.matches(&edit));
        assert!(!WindowFilter::default().matches(&menu));
        assert!(
            WindowFilter {
                all: true,
                ..Default::default()
            }
            .matches(&menu)
        );
        assert!(!by_title.matches(&window(5, 6, "Preview", 0)));
    }

    fn element(role: &str, title: Option<&str>, description: Option<&str>) -> Element {
        Element {
            index: 0,
            depth: 1,
            window: None,
            role: Some(role.into()),
            subrole: None,
            title: title.map(Into::into),
            description: description.map(Into::into),
            value: None,
            identifier: None,
            enabled: Some(true),
            focused: None,
            frame: None,
            unreadable: Vec::new(),
            truncated: Vec::new(),
        }
    }

    #[test]
    fn element_filters_match_role_and_exact_name() {
        let save = element("AXButton", Some("Save"), None);
        let seven = element("AXButton", None, Some("7"));
        let label = element("AXStaticText", Some("Save"), None);
        let filter = |role: Option<&str>, name: Option<&str>| ElementFilter {
            role: role.map(Into::into),
            name: name.map(Into::into),
        };
        assert!(filter(Some("axbutton"), Some("save")).matches(&save));
        assert!(!filter(Some("AXButton"), Some("Save")).matches(&label));
        assert!(
            filter(None, Some("7")).matches(&seven),
            "description counts"
        );
        assert!(!filter(None, Some("Sav")).matches(&save), "names are exact");
        assert!(filter(None, None).matches(&label));
        let mut unread = element("AXButton", None, None);
        unread.unreadable = vec!["AXTitle".into()];
        assert!(!filter(Some("AXButton"), Some("Save")).matches(&unread));
        assert!(filter(Some("AXButton"), Some("Save")).might_match(&unread));
        assert!(!filter(Some("AXStaticText"), Some("Save")).might_match(&unread));
    }

    #[test]
    fn a_label_is_named_by_its_text_but_a_field_is_not() {
        let filter = ElementFilter {
            role: None,
            name: Some("Main".into()),
        };
        let mut label = element("AXStaticText", None, None);
        label.value = Some("Main".into());
        assert!(filter.matches(&label) && filter.might_match(&label));
        let mut field = element("AXTextField", None, None);
        field.value = Some("Main".into());
        assert!(!filter.matches(&field), "typed text is not a name");
    }

    #[test]
    fn an_apps_own_windows_include_visible_floating_ones_only() {
        let name = Target::Name("Opie".into());
        let mut hidden = window(4, 6, "Opie", 3);
        hidden.alpha = 0.0;
        assert!(
            app_pid(&[hidden], &name).is_err(),
            "an invisible panel is no window"
        );
        let panel = window(5, 6, "Opie", 3);
        assert_eq!(app_pid(std::slice::from_ref(&panel), &name).unwrap(), 6);
        let frame = Frame {
            x: 10.0,
            y: 20.0,
            width: 300.0,
            height: 200.0,
        };
        let ordinary = window(7, 6, "Opie", 0);
        assert_eq!(
            owning_window(&[panel.clone(), ordinary], 6, &frame),
            Some(7),
            "the ordinary window wins over a same-sized overlay"
        );
        assert_eq!(
            owning_window(std::slice::from_ref(&panel), 6, &frame),
            Some(5)
        );
        let filter = WindowFilter {
            app: Some("opie".into()),
            ..WindowFilter::default()
        };
        assert!(
            filter.matches(&panel),
            "windows --app shows its floating panel"
        );
        assert!(!WindowFilter::default().matches(&panel));
    }

    #[test]
    fn apps_resolve_to_exactly_one_process() {
        let name = |name: &str| Target::Name(name.into());
        let windows = [window(1, 7, "Dock", 20), window(2, 8, "Calculator", 0)];
        assert_eq!(app_pid(&windows, &name("calculator")).unwrap(), 8);
        assert!(
            app_pid(&windows, &name("Dock")).is_err(),
            "only document windows"
        );
        assert!(app_pid(&windows, &name("Preview")).is_err());
        let alert = [window(3, 5, "Atlas", 8), window(1, 7, "Dock", 20)];
        assert_eq!(
            app_pid(&alert, &name("Atlas")).unwrap(),
            5,
            "a floating alert"
        );
        let twice = [window(1, 8, "TextEdit", 0), window(2, 9, "TextEdit", 0)];
        let error = app_pid(&twice, &name("TextEdit")).unwrap_err();
        assert!(error.error.contains("--pid"), "{}", error.error);
        assert_eq!(app_pid(&twice, &Target::Pid(9)).unwrap(), 9);
    }

    #[test]
    fn elements_belong_to_the_window_with_their_frame() {
        let windows = [window(4, 8, "TextEdit", 0), {
            let mut other = window(5, 8, "TextEdit", 0);
            other.bounds.x = 400.0;
            other
        }];
        let frame = |x: f64| Frame {
            x,
            y: 20.0,
            width: 300.0,
            height: 200.0,
        };
        assert_eq!(owning_window(&windows, 8, &frame(400.5)), Some(5));
        assert_eq!(owning_window(&windows, 8, &frame(10.0)), Some(4));
        assert_eq!(
            owning_window(&windows, 9, &frame(10.0)),
            None,
            "other process"
        );
        assert_eq!(owning_window(&windows, 8, &frame(200.0)), None);
        let twins = [window(4, 8, "TextEdit", 0), window(5, 8, "TextEdit", 0)];
        assert_eq!(
            owning_window(&twins, 8, &frame(10.0)),
            None,
            "identical geometry is ambiguous"
        );
    }
}

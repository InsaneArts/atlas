//! Agent-facing commands: small, stable verbs over the guarded input path,
//! the helper's window list, accessibility elements, app launching and
//! waiting for windows. Every action is an ordinary validated batch with the
//! usual ownership and epoch checks; nothing here bypasses them.
use crate::{
    actions::Button,
    client::Client,
    error::{Error, Result},
    native::{
        ax::{Element, Frame},
        windows::{Bounds, Window, unobstructed},
    },
    transport::run_output,
};
use serde_json::{Value, json};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use unicode_segmentation::UnicodeSegmentation;

/// The working contract printed by `understudy agent-guide`.
pub const GUIDE: &str = include_str!("agent_guide.md");

/// When a skill loads: what Understudy is for, in the words an agent matches.
const SKILL_DESCRIPTION: &str = "Test or operate macOS GUI apps, dialogs, menu bar items, \
installers and command-line tools in a hidden second login (the helper), without touching \
the person's own desktop, windows, keyboard or mouse. Use it to take screenshots of an app, \
click or type in a GUI, run something that would steal focus, or check that a GUI change \
actually works.";

/// The agent plugin's skill: when to use Understudy and its rules, then the
/// guide itself. The plugin's SKILL.md is this text (a test keeps them equal).
pub fn skill() -> String {
    let guide = GUIDE
        .split_once('\n')
        .map_or(GUIDE, |(_, rest)| rest)
        .trim_start();
    format!(
        "---\nname: understudy\ndescription: {SKILL_DESCRIPTION}\n---\n\n\
# Understudy\n\n\
Use it for anything that would otherwise touch the person's desktop: launching or \
clicking through an app, testing a dialog, a menu bar item or an installer, taking \
screenshots of a GUI. Do not use it for pure unit tests or for files you can read \
directly.\n\n\
## Rules\n\n\
- Never act on the person's own desktop; everything happens in the helper.\n\
- Never use sudo or change privacy settings. If `doctor` is not ready, tell the person \
what it says (or use the setup skill).\n\
- If a command reports `on-console`, the helper is on screen: ask the person to switch \
back to their own account. Do not retry.\n\
- Ask the person before `understudy switch`, and before anything that locks, sleeps \
or logs out.\n\
- After an `uncertain` result, run `understudy restart` before anything else.\n\
- The person can watch or take control at any time with `understudy menubar` or \
`understudy monitor`; mention it when you start a long task.\n\n\
{guide}\n\
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

/// Keys pressed in order, as one batch.
pub fn keys(keys: &[String]) -> Value {
    Value::Array(
        keys.iter()
            .map(|key| json!({ "type": "key", "key": key }))
            .collect(),
    )
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
        (self.all || window.layer == 0)
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
            "input",
            "Name an app with --app, a target, or both",
        ));
    }
    let helper = client.config.helper_uid;
    let mut copied = Vec::new();
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
    if let Some(app) = app {
        // A name is the helper's to look up; a path is this side's file.
        let app = if app.contains('/') {
            hand_over(app, helper, &mut copied)?
        } else {
            app.to_owned()
        };
        argv.extend(["-a".to_owned(), app]);
    }
    if !targets.is_empty() {
        argv.push("--".to_owned());
        for target in targets {
            argv.push(hand_over(target, helper, &mut copied)?);
        }
    }
    let reply = client.run(argv).await?;
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

/// Quit `app` in the helper (every process of that name), wait until it is
/// gone, then remove the copy `open` made of it, if there is one.
pub async fn close(client: &Client, app: &str) -> Result<Value> {
    let pids = |apps: &Value| -> Vec<u64> {
        apps["apps"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|running| running["name"] == app)
            .filter_map(|running| running["pid"].as_u64())
            .collect()
    };
    let running = pids(&client.apps().await?);
    if !running.is_empty() {
        let mut argv = vec!["/bin/kill".to_owned()];
        argv.extend(running.iter().map(u64::to_string));
        client.run(argv).await?;
        let deadline = Instant::now() + Duration::from_secs(10);
        while !pids(&client.apps().await?).is_empty() {
            if Instant::now() >= deadline {
                return Err(Error::new(
                    "input",
                    format!("{app} did not quit within 10 seconds"),
                ));
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    }
    let removed: Vec<String> = copies_of(app)
        .into_iter()
        .filter(|copy| fs::remove_dir_all(copy).is_ok())
        .map(|copy| copy.to_string_lossy().into_owned())
        .collect();
    Ok(json!({
        "ok": true,
        "app": app,
        "closed": !running.is_empty(),
        "removed": removed,
    }))
}

/// The apps in HANDOFF that are `app`: named so, or whose bundle calls itself so.
fn copies_of(app: &str) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(HANDOFF) else {
        return Vec::new();
    };
    entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "app"))
        .filter(|path| {
            path.file_stem().is_some_and(|stem| stem == app)
                || ["CFBundleDisplayName", "CFBundleName", "CFBundleExecutable"]
                    .iter()
                    .any(|key| bundle_value(path, key).as_deref() == Some(app))
        })
        .collect()
}

/// One string from a bundle's Info.plist.
fn bundle_value(bundle: &Path, key: &str) -> Option<String> {
    let output = std::process::Command::new("/usr/libexec/PlistBuddy")
        .arg("-c")
        .arg(format!("Print :{key}"))
        .arg(bundle.join("Contents/Info.plist"))
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

/// Where `open` copies an app or file the helper cannot read, such as a
/// build in a private folder. Only the helper account is let in.
pub const HANDOFF: &str = "/Users/Shared/Understudy/Open";

/// `target` as the helper can open it. URLs and names pass through; a path
/// here becomes absolute (the helper has its own working directory), and
/// one the helper cannot read is copied into HANDOFF first.
fn hand_over(target: &str, helper: u32, copied: &mut Vec<Value>) -> Result<String> {
    let Ok(path) = fs::canonicalize(target) else {
        return Ok(target.to_owned());
    };
    if readable_by_others(&path) {
        return Ok(path.to_string_lossy().into_owned());
    }
    let name = path
        .file_name()
        .ok_or_else(|| Error::new("input", format!("Cannot open {target}")))?;
    let handoff = handoff(helper)?;
    let copy = handoff.join(name);
    match fs::symlink_metadata(&copy) {
        Ok(metadata) if metadata.is_dir() => fs::remove_dir_all(&copy)?,
        Ok(_) => fs::remove_file(&copy)?,
        Err(_) => {}
    }
    let status = std::process::Command::new("/usr/bin/ditto")
        .arg("--noacl")
        .arg(&path)
        .arg(&copy)
        .stdin(std::process::Stdio::null())
        .status()?;
    if !status.success() {
        return Err(Error::new(
            "io",
            format!("Could not copy {} for the helper", path.display()),
        ));
    }
    copied.push(json!({"from": path, "to": copy}));
    Ok(copy.to_string_lossy().into_owned())
}

/// Whether accounts other than this one may read `path`: it is readable
/// (and, for a folder, enterable) by others, and so is every folder above.
fn readable_by_others(path: &Path) -> bool {
    let mode = |path: &Path| fs::metadata(path).map(|metadata| metadata.permissions().mode());
    let own = if path.is_dir() { 0o005 } else { 0o004 };
    mode(path).is_ok_and(|mode| mode & own == own)
        && path
            .ancestors()
            .skip(1)
            .all(|folder| mode(folder).is_ok_and(|mode| mode & 0o001 != 0))
}

/// HANDOFF, private to this account apart from an inherited read-only
/// grant to the helper.
fn handoff(helper: u32) -> Result<PathBuf> {
    let folder = PathBuf::from(HANDOFF);
    fs::create_dir_all(&folder)?;
    fs::set_permissions(&folder, fs::Permissions::from_mode(0o700))?;
    let account = nix::unistd::User::from_uid(nix::unistd::Uid::from_raw(helper))
        .ok()
        .flatten()
        .ok_or_else(|| Error::new("identity", "The helper account was not found"))?;
    let grant = format!(
        "{} allow list,search,readattr,readextattr,readsecurity,read,execute,file_inherit,directory_inherit",
        account.name
    );
    for args in [vec!["-N"], vec!["+a", grant.as_str()]] {
        let status = std::process::Command::new("/bin/chmod")
            .args(&args)
            .arg(&folder)
            .stdin(std::process::Stdio::null())
            .status()?;
        if !status.success() {
            return Err(Error::new(
                "io",
                format!("Could not let the helper into {HANDOFF}"),
            ));
        }
    }
    Ok(folder)
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
        return Err(Error::new("input", "Wait for --app, --title, or both"));
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
    /// Exact title, description or identifier, compared case-insensitively.
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
            })
    }
}

/// The app an element command is about: a PID, or a name that must resolve
/// to exactly one process owning a document window.
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
    let mut pids: Vec<i64> = windows
        .iter()
        .filter(|window| {
            window.layer == 0
                && window
                    .owner
                    .as_deref()
                    .is_some_and(|owner| owner.eq_ignore_ascii_case(name))
        })
        .map(|window| window.pid)
        .collect();
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
    let mut matching = windows.iter().filter(|window| {
        window.pid == pid
            && window.layer == 0
            && near(window.bounds.x, frame.x)
            && near(window.bounds.y, frame.y)
            && near(window.bounds.width, frame.width)
            && near(window.bounds.height, frame.height)
    });
    let first = matching.next()?;
    matching.next().is_none().then_some(first.number)
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
                0 => "No visible element matches. Run understudy elements --app NAME to see \
                      what is there."
                    .to_owned(),
                count => format!(
                    "{count} elements match. Run understudy elements with the same --name to \
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
            "verify-native",
        ] {
            assert!(
                GUIDE.contains(&format!("understudy {command}")),
                "{command}"
            );
        }
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
            keys(&["cmd+s".into(), "return".into()]),
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
    fn apps_resolve_to_exactly_one_process() {
        let name = |name: &str| Target::Name(name.into());
        let windows = [window(1, 7, "Dock", 20), window(2, 8, "Calculator", 0)];
        assert_eq!(app_pid(&windows, &name("calculator")).unwrap(), 8);
        assert!(
            app_pid(&windows, &name("Dock")).is_err(),
            "only document windows"
        );
        assert!(app_pid(&windows, &name("Preview")).is_err());
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

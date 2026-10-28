//! Agent-facing commands: small, stable verbs over the guarded input path,
//! the helper's window list, accessibility elements, app launching and
//! waiting for windows. Every action is an ordinary validated batch with the
//! usual ownership and epoch checks; nothing here bypasses them.
pub use crate::app_selection::Closing;
pub(crate) use crate::app_selection::same_text;

use crate::{
    actions::{Button, Expect},
    app_selection::{Process, atlas_bundle, selected},
    client::{Check, Client},
    control::Ownership,
    daemon::protocol::{Crop, Region},
    error::{Error, Holder, Result},
    handoff,
    host_session::HostSession,
    interrupt::Interrupt,
    native::{
        ax::{self, Element, Frame, Stop},
        windows::{Bounds, Dock, MENU_LAYER, Screen, Shown, Window, top_at, unobstructed},
    },
    session::Session,
    transport::{accepted, run_output},
};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};
use unicode_segmentation::UnicodeSegmentation;

/// The guide's core: what every agent reads first.
pub const CORE: &str = include_str!("agent_guide.md");

/// What an agent reads only when a task needs it, in the skill's folder:
/// (file, what it holds).
pub const REFERENCES: &[(&str, &str)] = &[
    ("testing.md", "Test your own app."),
    (
        "environment.md",
        "Give the app what it needs: copy files in and roll them back.",
    ),
    (
        "limits.md",
        "Limits, and details of typing, keys, scrolling and `atlas run`.",
    ),
    ("commands.md", "every command and its options."),
    (
        "troubleshooting.md",
        "the error reference, every code and what to do.",
    ),
];

/// The whole guide `atlas agent-guide` prints: the core, then each
/// reference written for the guide, then where the rest is.
pub const GUIDE: &str = concat!(
    include_str!("agent_guide.md"),
    "\n",
    include_str!("../../agents/atlas/skills/atlas/references/testing.md"),
    "\n",
    include_str!("../../agents/atlas/skills/atlas/references/environment.md"),
    "\n",
    include_str!("../../agents/atlas/skills/atlas/references/limits.md"),
    "\n",
    "## References\n\n`atlas agent-guide --commands` prints every command and its options, \
and `atlas agent-guide --errors` the error reference: every code and what to do.\n",
);

/// The error reference, printed by `atlas agent-guide --errors`.
pub const ERRORS: &str =
    include_str!("../../agents/atlas/skills/atlas/references/troubleshooting.md");

/// When a skill loads: what Atlas is for, in the words an agent matches.
const SKILL_DESCRIPTION: &str = "See and use macOS GUIs in a hidden second login (the helper) \
without touching the person's desktop, windows, keyboard or mouse. Use it whenever you change \
something a person could see in a Mac app (a window, dialog, menu, menu bar item, layout or \
icon), open the build in the helper and check it before saying it works. Also use it to \
reproduce on-screen bugs, run installers or anything that opens windows or takes focus, test \
a web app in Google Chrome or Firefox, take screenshots, and click or type in apps. Also use it when the person \
asks to watch the helper, see what you are doing or take over: open the live view for them. Not for unit tests, builds \
or reading files.";

/// The agent plugin's skill: the core under the skill's own title, then
/// the references it points to. The plugin's SKILL.md is this text (a test
/// keeps them equal).
pub fn skill() -> String {
    let core = CORE
        .split_once('\n')
        .map_or(CORE, |(_, rest)| rest)
        .trim_start();
    let references: String = REFERENCES
        .iter()
        .map(|(file, about)| format!("- [references/{file}](references/{file}): {about}\n"))
        .collect();

    format!(
        "---\nname: atlas\ndescription: \"{SKILL_DESCRIPTION}\"\n---\n\n# Atlas\n\n\
{core}\n## References\n\nRead one when the task needs it:\n\n{references}"
    )
}

/// How much text `atlas type` sends at once: 64 characters, about half a
/// second of typing, so a busy Mac typing several times slower than usual
/// still finishes each part well within the helper's deadline. Control is
/// held across the parts, so nothing lands between them.
const TYPE_BATCH: usize = 64;

/// How much text one `type` action holds: 16 characters. With a target the
/// helper checks focus before each action, so at most this much can follow
/// a focus change before Atlas sees it.
const TYPE_PART: usize = 16;

/// How long a read-only look at the helper (its window list) may take.
const PROBE_LIMIT: Duration = Duration::from_secs(5);

pub fn click(x: f64, y: f64, which: Button, count: u8, modifiers: &[String]) -> Value {
    json!([{
        "type": "click",
        "x": x,
        "y": y,
        "button": which,
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
        "button": which,
    }])
}

/// Text in pieces of at most `limit` characters, never splitting a
/// grapheme cluster (an emoji sequence or a letter with combining marks); a
/// longer cluster goes on its own.
fn split_text(text: &str, limit: usize) -> Vec<String> {
    let mut pieces = Vec::new();
    let mut piece = String::new();
    let mut length = 0;

    for grapheme in text.graphemes(true) {
        let size = grapheme.chars().count();
        if length > 0 && length + size > limit {
            pieces.push(std::mem::take(&mut piece));
            length = 0;
        }

        piece.push_str(grapheme);
        length += size;
    }

    if !piece.is_empty() {
        pieces.push(piece);
    }

    pieces
}

/// Text as `type` batches of at most [`TYPE_BATCH`] characters, never
/// splitting a grapheme cluster; a longer cluster goes on its own, up to
/// [`TEXT_MAX`](crate::actions::TEXT_MAX).
pub(crate) fn type_batches(text: &str) -> Result<Vec<String>> {
    if text
        .graphemes(true)
        .any(|grapheme| grapheme.chars().count() > crate::actions::TEXT_MAX)
    {
        return Err(Error::new(
            "input",
            "A single character cluster is too long to type",
        ));
    }

    let batches = split_text(text, TYPE_BATCH);
    if batches.is_empty() {
        return Err(Error::new("input", "Provide text to type"));
    }

    Ok(batches)
}

/// A batch as `type` actions of at most [`TYPE_PART`] characters, never
/// splitting a grapheme cluster; a longer cluster goes on its own.
fn type_parts(batch: &str) -> Vec<String> {
    split_text(batch, TYPE_PART)
}

/// Send `actions` as one batch. With a target, the app is found once
/// control is held, and the helper refuses each action before it starts
/// unless it lands in that app: text and keys while the app has keyboard
/// focus, pointer actions where its own window is the top one.
pub async fn act(client: &Client, target: Option<&Target>, actions: &Value) -> Result<Value> {
    let Some(target) = target else {
        return client.input(actions, None, None).await;
    };

    let ownership = client.controller.acquire(false)?;
    let verified = client.identity().await?;
    let expect = Expect::new(target_pid(client, &verified, target).await?)?;
    client
        .input_owned(
            &ownership,
            actions,
            &verified.epoch,
            Some(Check::Required(expect)),
        )
        .await
}

/// Type `text` batch by batch, stopping at the first batch that is not
/// confirmed. Each batch goes as `type` actions of [`TYPE_PART`]
/// characters, and with a target each is sent only while that app has
/// keyboard focus. The report says exactly how much was confirmed typed
/// and, on failure, whether the failed batch's delivery is uncertain. The
/// whole text is typed under one ownership into the helper as it was at
/// the start: no other input lands between batches, and a helper that
/// restarts midway is refused rather than typed into. A signal's stop ends
/// it before the next batch, with the same report.
pub async fn type_text(client: &Client, text: &str, target: Option<&Target>) -> Result<Value> {
    let batches = type_batches(text)?;

    let ownership = client.controller.acquire(false)?;
    // A signal's stop waits for the report of how much was typed.
    let interrupt = client.controller.interrupt();
    let _typing = interrupt.step();
    let verified = client.identity().await?;
    let check = match target {
        Some(target) => Some(Check::Required(Expect::new(
            target_pid(client, &verified, target).await?,
        )?)),
        None => None,
    };
    let epoch = verified.epoch;

    let send = async |actions: &Value| client.input_owned(&ownership, actions, &epoch, check).await;
    Ok(type_each(&batches, interrupt, send).await)
}

/// Send each of `batches` as `type` actions through `send`, stopping at
/// the first that is not confirmed, or before the next once `interrupt`
/// was asked to stop: the report says how much was confirmed typed.
async fn type_each(
    batches: &[String],
    interrupt: &Interrupt,
    mut send: impl AsyncFnMut(&Value) -> Result<Value>,
) -> Value {
    let (mut typed, mut confirmed) = (0, 0);

    for batch in batches {
        let parts = type_parts(batch);
        let actions: Vec<Value> = parts
            .iter()
            .map(|part| json!({ "type": "type", "text": part }))
            .collect();
        let outcome = match interrupt.check() {
            Ok(()) => send(&Value::Array(actions)).await,
            Err(stop) => Err(stop),
        };

        let failure = match outcome {
            Ok(reply) if reply["ok"] == true => {
                confirmed += 1;
                typed += batch.chars().count();
                continue;
            }
            Ok(reply) => Unconfirmed::from_reply(&reply),
            Err(error) => Unconfirmed::from(error),
        };

        return stopped_typing(typed, batches.len(), confirmed, &parts, failure);
    }

    json!({ "ok": true, "typed": typed, "batches": batches.len() })
}

/// A batch the helper did not confirm: why, how many of its actions were
/// performed, and whether the failing one had sent some of its events.
struct Unconfirmed {
    error: Error,
    performed: usize,
    partial: bool,
}

impl Unconfirmed {
    /// A failure the helper replied with. Agents act on the code (busy,
    /// lease-lost, session-changed, focus-changed), on who holds control
    /// when busy and on what was found instead of the app, so those are
    /// kept; nothing else of the reply is.
    fn from_reply(reply: &Value) -> Self {
        let code = reply["code"]
            .as_str()
            .and_then(crate::error::code)
            .unwrap_or("input");
        let message = reply["error"]
            .as_str()
            .unwrap_or("The helper did not confirm the text");
        let mut error = Error::new(code, message);
        if reply["uncertain"] == true {
            error = error.uncertain();
        }
        if let Some(holder) = reply["holder"].as_str().and_then(Holder::parse) {
            error = error.held_by(holder);
        }
        for kept in ["reason", "found"] {
            if !reply[kept].is_null() {
                error = error.with(kept, &reply[kept]);
            }
        }

        Self {
            error,
            performed: reply["performed"]
                .as_u64()
                .and_then(|performed| usize::try_from(performed).ok())
                .unwrap_or(0),
            partial: reply["partial"] == true,
        }
    }
}

impl From<Error> for Unconfirmed {
    /// A failure before or around the batch: none of it is known performed.
    fn from(error: Error) -> Self {
        Self {
            error,
            performed: 0,
            partial: false,
        }
    }
}

/// The report of a `type` that stopped at a batch: everything confirmed
/// before it, `typed` characters in `confirmed` of `batches`, plus the
/// `parts` of that batch the helper says it performed. `partial` says some
/// of the text was sent.
fn stopped_typing(
    typed: usize,
    batches: usize,
    confirmed: usize,
    parts: &[String],
    failure: Unconfirmed,
) -> Value {
    let typed = typed
        + parts
            .iter()
            .take(failure.performed)
            .map(|part| part.chars().count())
            .sum::<usize>();

    let mut report = json!(failure.error);
    report["typed"] = json!(typed);
    report["batches"] = json!(batches);
    report["confirmedBatches"] = json!(confirmed);
    report["partial"] = json!(typed > 0 || failure.partial);

    report
}

/// Keys pressed in order, as one batch; an unknown key is named.
pub fn keys(keys: &[String]) -> Result<Value> {
    if let Some(unknown) = keys.iter().find(|key| !crate::actions::valid_key(key)) {
        return Err(Error::new(
            "usage",
            format!(
                "{unknown} is not a key Atlas knows. Use a letter, digit or symbol, a name such \
                 as return, tab, escape, space, backspace, up or f5, with modifiers like cmd+s \
                 or cmd+shift+z."
            ),
        ));
    }

    Ok(Value::Array(
        keys.iter()
            .map(|key| json!({ "type": "key", "key": key }))
            .collect(),
    ))
}

/// How text shown only in part matches `name`. Its first ", " part is
/// whole, so it is judged like any other; the rest proves nothing, unless
/// `unknown` lets a possible match count.
fn cut_match(shown: Option<&str>, name: &str, unknown: bool) -> Option<NameMatch> {
    let shown = shown?;
    if let Some((first, _)) = shown.split_once(", ")
        && same_text(first, name)
    {
        return Some(NameMatch::Leading);
    }

    (unknown && could_begin(shown, name)).then_some(NameMatch::Exact)
}

/// Whether `text`, which is only the start of a longer value, could begin
/// a value named `name`: the part shown must begin the name.
fn could_begin(text: &str, name: &str) -> bool {
    let mut name = name.chars().flat_map(char::to_lowercase);
    text.chars()
        .flat_map(char::to_lowercase)
        .all(|c| name.next() == Some(c))
}

/// Which helper windows a command is about.
#[derive(Debug, Default, Clone)]
pub struct WindowFilter {
    /// Owning application name, compared case-insensitively.
    pub app: Option<String>,
    /// Text the window title must contain, compared case-insensitively.
    pub title: Option<String>,
    /// Include menus, the menu bar, the Dock and other system layers.
    pub all: bool,
}

impl WindowFilter {
    pub fn matches(&self, window: &Window) -> bool {
        // An app's floating panels and alerts count as its windows, so a
        // system alert in front of everything is listed too.
        (self.all || app_layer(window))
            && self.app.as_ref().is_none_or(|app| {
                window
                    .owner
                    .as_deref()
                    .is_some_and(|owner| same_text(owner, app))
            })
            && self.title.as_ref().is_none_or(|title| {
                window
                    .title
                    .as_deref()
                    .is_some_and(|own| own.to_lowercase().contains(&title.to_lowercase()))
            })
    }
}

/// The helper's on-screen windows, front to back, through a read-only probe.
pub(crate) async fn windows(client: &Client) -> Result<Vec<Window>> {
    window_list(client, PROBE_LIMIT).await
}

/// The window list alone, with the helper probe bounded by `limit`.
async fn window_list(client: &Client, limit: Duration) -> Result<Vec<Window>> {
    let probe = windows_probe(client, DockRead::Skip);
    let reply = client.inspect_within(probe, limit).await?;
    Ok(shown(&reply)?.windows)
}

/// The window list and where the Dock takes clicks, to judge a point by.
async fn pointer_view(client: &Client) -> Result<Shown> {
    let probe = windows_probe(client, DockRead::Read);
    let reply = client.inspect_within(probe, PROBE_LIMIT).await?;
    shown(&reply)
}

/// The window list alone, in the helper `verified` describes, which the
/// caller verified just before.
async fn verified_window_list(client: &Client, verified: &Session) -> Result<Vec<Window>> {
    let probe = windows_probe(client, DockRead::Skip);
    let reply = client.inspect_in(verified, probe, PROBE_LIMIT).await?;
    Ok(shown(&reply)?.windows)
}

/// Whether a window probe also reads where the Dock takes clicks, which
/// only a check of a point needs.
#[derive(Clone, Copy)]
enum DockRead {
    Skip,
    Read,
}

/// The probe that lists the helper's windows.
fn windows_probe(client: &Client, dock: DockRead) -> Vec<String> {
    let probe = client.config.identity_probe.to_string_lossy().into_owned();
    let mut argv = vec![probe, "windows-info".into()];
    if matches!(dock, DockRead::Read) {
        argv.push("--dock".into());
    }
    argv
}

/// What the window probe's `reply` shows.
fn shown(reply: &Value) -> Result<Shown> {
    crate::native::windows::from_probe(reply)
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

    let handed = handoff::hand_over(client, &ownership, wanted, &running).await?;
    let copied = handed.copied;
    let mut given = handed.paths.into_iter();

    if app.is_some() {
        argv.extend(["-a".to_owned(), given.next().unwrap_or_default()]);
    }

    if !targets.is_empty() {
        argv.push("--".to_owned());
        argv.extend(given);
    }

    let mut reply = accepted(client.run_owned(&ownership, argv.clone()).await?)?;

    if reply["exitCode"] != 0 && cleared_stale_launch(client, &ownership, &reply).await? {
        reply = accepted(client.run_owned(&ownership, argv).await?)?;
    }

    let opened = reply["exitCode"] == 0;
    let mut value = json!({ "ok": opened, "opened": opened });

    if !copied.is_empty() {
        value["copied"] = json!(copied);
    }

    if !opened {
        // Said with its code, and with the copies made for it, which stay
        // reported: the launch may still be on its way (a Gatekeeper
        // question, say), and close removes them.
        let output = run_output(&reply, 4096).unwrap_or_default();
        let said = String::from_utf8_lossy(&output).trim().to_owned();

        if reply["timedOut"] == true {
            value["code"] = json!("wait-failed");
            value["timedOut"] = json!(true);
            value["error"] = json!(
                "macOS did not finish opening that in time; it may still be launching: \
                 run atlas wait --app NAME, or look with atlas apps"
            );
        } else {
            value["code"] = json!("spawn-failed");
            value["error"] = json!(if said.is_empty() {
                "macOS could not open that in the helper".to_owned()
            } else {
                said
            });
        }

        value["uncertain"] = json!(false);
    }

    Ok(value)
}

/// After a failed `open`, remove the finished job launchd still holds for
/// its app, when that is why it failed: an app opened again just as it quit
/// can leave one, and every later launch fails until it is gone. Whether
/// one was removed, so the launch is worth trying again.
async fn cleared_stale_launch(
    client: &Client,
    ownership: &Ownership,
    reply: &Value,
) -> Result<bool> {
    let output = run_output(reply, 4096).unwrap_or_default();
    let said = String::from_utf8_lossy(&output);

    let Some(bundle) = stale_launch(&said) else {
        return Ok(false);
    };

    let argv = vec![
        "/bin/sh".to_owned(),
        "-c".to_owned(),
        CLEAR_STALE_JOB.to_owned(),
        "sh".to_owned(),
        bundle.to_owned(),
    ];
    let cleared = accepted(client.run_owned(ownership, argv).await?)?;

    Ok(!run_output(&cleared, 4096).unwrap_or_default().is_empty())
}

/// The app whose launch launchd refused because it still holds that app's
/// finished job (error 37, "Operation already in progress"), from what
/// `open` said.
fn stale_launch(said: &str) -> Option<&str> {
    if !said.contains("OSLaunchdErrorDomain Code=37") {
        return None;
    }

    let rest = said.split_once("The application ")?.1;
    let end = rest.find(".app ")? + ".app".len();
    let app = &rest[..end];

    app.starts_with('/').then_some(app)
}

/// In the helper, remove launchd's jobs for the app at `$1` that are not
/// running (none of its windows or processes go with them): exactly the
/// labels `application.<its bundle id>.<n>.<n>`, never an app whose bundle
/// id merely begins with it. It prints only the labels it removed. The next
/// launch submits a fresh job.
const CLEAR_STALE_JOB: &str = r#"id=$(/usr/bin/plutil -extract CFBundleIdentifier raw -o - "$1/Contents/Info.plist" 2>/dev/null) || exit 0
case "$id" in *[!A-Za-z0-9.-]*|'') exit 0;; esac
/bin/launchctl list | /usr/bin/awk -v prefix="application.$id." '
  $1 == "-" && index(tolower($3), tolower(prefix)) == 1 &&
  substr($3, length(prefix) + 1) ~ /^[0-9]+\.[0-9]+$/ { print $3 }' |
while read -r label; do
  /bin/launchctl remove "$label" 2>/dev/null && echo "$label"
done
exit 0"#;

fn app_names(apps: &Value) -> Vec<&str> {
    apps["apps"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|app| app["name"].as_str())
        .collect()
}

/// The names of the selected processes that were (or, with `quit` false,
/// were not) among the `signalled` ones, sorted, each once.
fn names_by_quit(processes: &[Process], signalled: &[u64], quit: bool) -> Vec<String> {
    let mut names: Vec<String> = processes
        .iter()
        .filter(|process| signalled.contains(&process.pid) == quit)
        .map(|process| process.name.clone())
        .collect();
    names.sort();
    names.dedup();

    names
}

/// The processes the quit script signalled, one ID per line of its output.
fn signalled(output: &[u8]) -> Vec<u64> {
    String::from_utf8_lossy(output)
        .lines()
        .filter_map(|line| line.trim().parse().ok())
        .collect()
}

/// How long `close` waits for the apps it signalled to quit.
const QUIT_WAIT: Duration = Duration::from_secs(10);

/// Quit apps in the helper: one by name (every process of it), or with
/// [`Closing::All`] every app that is not part of macOS or Atlas. Waits
/// until they are gone, then removes the copies `open` made of them.
pub async fn close(client: &Client, closing: Closing<'_>) -> Result<Value> {
    if let Closing::App(app) = closing
        && app.contains('/')
    {
        return Err(Error::new(
            "usage",
            "close takes the app's name as atlas apps lists it, not a path",
        ));
    }

    let ownership = client.controller.acquire(false)?;
    let atlas = atlas_bundle();
    let apps = client.apps().await?;
    let processes = selected(&apps, closing, atlas.as_deref());

    // The helper's session and Atlas's own services are never closed.
    if let Closing::App(app) = closing
        && processes.iter().any(|process| process.protected)
    {
        return Err(Error::new(
            "input",
            format!("Atlas does not close {app}: it is part of macOS or of Atlas"),
        ));
    }

    let names_of =
        |apps: &Value| -> Vec<String> { app_names(apps).into_iter().map(str::to_owned).collect() };
    // What runs in the helper once the apps are gone; with nothing to quit,
    // what ran already.
    let mut running = names_of(&apps);
    let mut quit = Vec::new();

    if !processes.is_empty() {
        // In the helper, each process is signalled only while it is still
        // the executable listed, so a reused process ID is left alone.
        // It prints each process it signalled.
        const QUIT: &str = r#"while [ $# -ge 2 ]; do
  pid=$1 exe=$2; shift 2
  [ -n "$exe" ] && [ "$(/bin/ps -o comm= -p "$pid")" = "$exe" ] && /bin/kill "$pid" && echo "$pid"
done
exit 0"#;

        let mut argv = vec![
            "/bin/sh".to_owned(),
            "-c".to_owned(),
            QUIT.to_owned(),
            "sh".to_owned(),
        ];

        for process in &processes {
            argv.extend([process.pid.to_string(), process.executable.clone()]);
        }

        let reply = accepted(client.run_owned(&ownership, argv).await?)?;
        if reply["timedOut"] == true || reply["exitCode"] != 0 {
            return Err(Error::new(
                "wait-failed",
                "The helper did not finish quitting the apps; run atlas apps to see what is open",
            ));
        }

        // Only the processes signalled are waited for: one whose program
        // did not check out was left alone.
        quit = signalled(&run_output(&reply, 64 * 1024).unwrap_or_default());
        let pids = &quit;
        let deadline = Instant::now() + QUIT_WAIT;

        loop {
            let apps = client.apps().await?;

            // One app is closed once no process of its name is left (a
            // relaunch counts); everything once each process signalled is
            // gone, whatever started since.
            let mut left: Vec<String> = selected(&apps, closing, atlas.as_deref())
                .into_iter()
                .filter(|process| matches!(closing, Closing::App(_)) || pids.contains(&process.pid))
                .map(|process| process.name)
                .collect();
            left.sort();
            left.dedup();

            if left.is_empty() {
                running = names_of(&apps);
                break;
            }

            if Instant::now() >= deadline {
                return Err(Error::new(
                    "wait-failed",
                    format!(
                        "{} did not quit within {} seconds",
                        left.join(", "),
                        QUIT_WAIT.as_secs()
                    ),
                ));
            }

            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    }

    // What was quit, and (for --all) what was left because its process had
    // already changed by the time it would have been signalled.
    let named = |signalled: bool| names_by_quit(&processes, &quit, signalled);

    let names = match closing {
        Closing::App(app) => vec![app.to_owned()],
        Closing::All => named(true),
    };

    // A copy is never removed while an app of its names runs: an app that
    // came back, or another one, may be running from it. An agent's close
    // keeps the copies another agent session made.
    let running: Vec<&str> = running.iter().map(String::as_str).collect();
    let session = client.host_session.as_ref().map(HostSession::key);
    let (mut removed, mut kept_others) = (Vec::new(), 0);
    for name in &names {
        let copies = handoff::remove_copies_of(name, &running, session);
        removed.extend(copies.folders);
        kept_others += copies.kept_others;
    }

    let mut reply = json!({
        "ok": true,
        "closed": !processes.is_empty(),
        "removed": removed,
    });
    if kept_others > 0 {
        reply["keptOthers"] = json!(kept_others);
    }

    match closing {
        Closing::App(app) => {
            reply["app"] = json!(app);
            if processes.is_empty() {
                reply["reason"] = json!(format!("{app} is not running"));
            }
        }
        Closing::All => {
            reply["closed"] = json!(!names.is_empty());
            reply["apps"] = json!(names);
            let skipped = named(false);
            if !skipped.is_empty() {
                reply["skipped"] = json!(skipped);
            }
        }
    }

    Ok(reply)
}

/// Poll the window list until a matching window appears (or, with `gone`,
/// until none remains). The timeout is a deadline for starting a look; the
/// last look counts even when it finishes after it, and a zero timeout
/// means one observation.
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
    let deadline = started
        .checked_add(timeout)
        .ok_or_else(|| Error::new("usage", "That --timeout is too long to wait"))?;

    loop {
        // Each observation gets what is left of the deadline (at least half
        // a second, at most what the helper lets a command take); a zero
        // timeout is exactly one observation.
        let remaining = deadline.saturating_duration_since(Instant::now());
        let budget = if timeout.is_zero() {
            PROBE_LIMIT
        } else {
            probe_budget(remaining)
        };

        let matching: Vec<_> = window_list(client, budget)
            .await?
            .into_iter()
            .filter(|window| filter.matches(window))
            .collect();

        if matching.is_empty() == gone {
            return Ok(json!({
                "ok": true,
                "waitedMs": u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
                "windows": matching,
            }));
        }

        let observed = Instant::now();
        if timeout.is_zero() || observed >= deadline {
            let error = if gone {
                "The window did not close in time".to_owned()
            } else {
                no_window(client, filter).await
            };

            return Ok(json!({
                "ok": false,
                "code": "wait-failed",
                "timedOut": true,
                "error": error,
                "uncertain": false,
            }));
        }

        tokio::time::sleep((deadline - observed).min(Duration::from_millis(250))).await;
    }
}

/// How long one look at the window list may take with `remaining` left to
/// wait: at least half a second, and no longer than the helper allows.
fn probe_budget(remaining: Duration) -> Duration {
    remaining
        .max(Duration::from_millis(500))
        .min(crate::client::RUN_TIMEOUT_MAX)
}

/// Why no window appeared: the app is not running, or it runs without a
/// window, as a menu bar app does until its menu bar item opens one.
async fn no_window(client: &Client, filter: &WindowFilter) -> String {
    let fallback = "No matching window appeared in time".to_owned();

    let Some(app) = filter.app.as_deref().filter(|_| filter.title.is_none()) else {
        return fallback;
    };

    let Ok(apps) = client.apps().await else {
        return fallback;
    };

    let running = apps["apps"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|running| {
            running["name"]
                .as_str()
                .is_some_and(|name| same_text(name, app))
        });
    match running {
        None => format!("{app} is not running"),
        Some(running) if running["policy"] == "accessory" => format!(
            "{app} is running but shows no window: a menu bar app opens its windows from its \
             menu bar item"
        ),
        Some(_) => format!("{app} is running but shows no window"),
    }
}

/// Which accessibility elements a command is about.
#[derive(Debug, Default, Clone)]
pub struct ElementFilter {
    /// AX role, for example `AXButton`, compared case-insensitively.
    pub role: Option<String>,
    /// Exact title, label text, description or identifier, compared
    /// case-insensitively. A label that joins several parts ("QA Eject,
    /// Option-Space, 6 Slices") is also named by its first part.
    pub name: Option<String>,
}

/// How an element's name matched a `--name`: only as the first part of a
/// label that joins several, or whole. A whole match is the better one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum NameMatch {
    Leading,
    Exact,
}

/// How `text` matches `name`: whole, or as the first of its ", " parts.
fn name_match(text: &str, name: &str) -> Option<NameMatch> {
    if same_text(text, name) {
        return Some(NameMatch::Exact);
    }

    let first = text.split_once(", ").map(|(first, _)| first)?;
    same_text(first, name).then_some(NameMatch::Leading)
}

impl ElementFilter {
    /// How `element` matches, treating the attributes in `unknown` as
    /// matching (they could not be read). Of an attribute shown only in
    /// part, only its whole first part is judged. An identifier is never a
    /// label, so it only matches whole.
    fn compare(&self, element: &Element, unknown: &dyn Fn(&str) -> bool) -> Option<NameMatch> {
        let cut = |attribute: &str| element.truncated.iter().any(|a| a == attribute);
        let role = self.role.as_deref().is_none_or(|role| {
            unknown("AXRole")
                || element
                    .role
                    .as_deref()
                    .is_some_and(|own| own.eq_ignore_ascii_case(role))
        });
        if !role {
            return None;
        }

        let Some(name) = self.name.as_deref() else {
            return Some(NameMatch::Exact);
        };

        let text = |value: &Option<String>, attribute: &str| {
            if cut(attribute) {
                return cut_match(value.as_deref(), name, unknown(attribute));
            }
            if unknown(attribute) {
                return Some(NameMatch::Exact);
            }
            value.as_deref().and_then(|text| name_match(text, name))
        };

        let identifier = if cut("AXIdentifier") {
            // What is shown of a cut identifier can still rule a name out.
            (unknown("AXIdentifier")
                && element
                    .identifier
                    .as_deref()
                    .is_some_and(|text| could_begin(text, name)))
            .then_some(NameMatch::Exact)
        } else if unknown("AXIdentifier") {
            Some(NameMatch::Exact)
        } else {
            element
                .identifier
                .as_deref()
                .filter(|identifier| same_text(identifier, name))
                .map(|_| NameMatch::Exact)
        };

        let label = if cut("AXValue") {
            cut_match(label(element), name, unknown("AXValue"))
        } else if unknown("AXValue") {
            // A static text whose value failed to read may be the label.
            (element.role.as_deref() == Some("AXStaticText") || unknown("AXRole"))
                .then_some(NameMatch::Exact)
        } else {
            label(element).and_then(|text| name_match(text, name))
        };

        [
            text(&element.title, "AXTitle"),
            text(&element.description, "AXDescription"),
            identifier,
            label,
        ]
        .into_iter()
        .flatten()
        .max()
    }

    /// How `element` matches, if it does.
    pub(crate) fn match_of(&self, element: &Element) -> Option<NameMatch> {
        self.compare(element, &|_| false)
    }

    pub fn matches(&self, element: &Element) -> bool {
        self.match_of(element).is_some()
    }

    /// Whether `element` could match once attributes it failed to read, or
    /// read only in part, are treated as unknown.
    pub(crate) fn might_match(&self, element: &Element) -> bool {
        let unknown = |attribute: &str| {
            element.unreadable.iter().any(|a| a == attribute)
                || element.truncated.iter().any(|a| a == attribute)
        };

        self.compare(element, &unknown).is_some()
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

/// Every name an element has: its title, description, label text and
/// identifier, in that order.
fn names(element: &Element) -> impl Iterator<Item = &str> {
    [
        element.title.as_deref(),
        element.description.as_deref(),
        label(element),
        element.identifier.as_deref(),
    ]
    .into_iter()
    .flatten()
}

/// What an element is called: the first of its names that is not empty.
fn shown_name(element: &Element) -> Option<&str> {
    names(element).find(|name| !name.is_empty())
}

/// Layers from the Dock's up hold the Dock, the menu bar and system overlays,
/// never an app's own windows.
const DOCK_LAYER: i64 = 20;

/// Whether `window` is one of an app's own visible windows: an ordinary one,
/// or a floating panel or alert below the Dock's layer. An open menu is not
/// one: `windows` and `wait` never mistake it for the window it opens.
fn app_layer(window: &Window) -> bool {
    window.layer == 0 || ((1..DOCK_LAYER).contains(&window.layer) && window.alpha > 0.0)
}

/// [`app_layer`], or a menu the app has open: what an element command may
/// act on.
fn app_or_menu_layer(window: &Window) -> bool {
    app_layer(window) || (window.layer == MENU_LAYER && window.alpha > 0.0)
}

/// The app an element command is about: a PID, or a name that must resolve
/// to exactly one process with a window of its own on screen.
#[derive(Debug, Clone)]
pub enum Target {
    Name(String),
    Pid(i64),
}

/// The processes named `name` that have windows on screen, by the kind of
/// window: ordinary ones first, then floating panels and alerts, then open
/// menus. An app showing only one of those is found by it.
fn app_pids(windows: &[Window], name: &str) -> Vec<i64> {
    let candidates: Vec<&Window> = windows
        .iter()
        .filter(|window| {
            app_or_menu_layer(window)
                && window
                    .owner
                    .as_deref()
                    .is_some_and(|owner| same_text(owner, name))
        })
        .collect();

    let tiers: [&dyn Fn(&Window) -> bool; 3] = [
        &|window| window.layer == 0,
        &|window| window.layer != MENU_LAYER,
        &|_| true,
    ];

    let mut pids: Vec<i64> = tiers
        .iter()
        .map(|tier| {
            candidates
                .iter()
                .filter(|window| tier(window))
                .map(|window| window.pid)
                .collect::<Vec<_>>()
        })
        .find(|pids| !pids.is_empty())
        .unwrap_or_default();
    pids.sort_unstable();
    pids.dedup();

    pids
}

/// The one process among `pids` a command about `name` is for.
fn one_pid(name: &str, pids: &[i64]) -> Result<i64> {
    match pids {
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

/// The process a command is about: the one with the app's windows, or,
/// when none of its windows is open, the active app of that name (its
/// menu bar is still on screen). Several apps of the name with windows
/// stay ambiguous. It is found in the helper `verified` describes, which
/// the caller verified just before, so it is not verified again for each
/// list it reads.
async fn target_pid(client: &Client, verified: &Session, target: &Target) -> Result<i64> {
    let name = match target {
        Target::Pid(pid) => {
            let apps = client.apps_in(verified).await?;
            let running = apps["apps"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|app| app["pid"] == *pid);
            if !running {
                return Err(Error::new(
                    "elements",
                    format!(
                        "No app in the helper has the process ID {pid}; run atlas apps to see them"
                    ),
                ));
            }

            return Ok(*pid);
        }
        Target::Name(name) => name,
    };

    let pids = app_pids(&verified_window_list(client, verified).await?, name);
    if !pids.is_empty() {
        return one_pid(name, &pids);
    }

    let apps = client.apps_in(verified).await?;
    let named: Vec<&Value> = apps["apps"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|app| app_named(app, name))
        .collect();
    if let Some(pid) = named
        .iter()
        .find(|app| app["active"] == true)
        .and_then(|app| app["pid"].as_i64())
    {
        return Ok(pid);
    }

    Err(Error::new(
        "elements",
        if named.is_empty() {
            format!("{name} is not running: run atlas open --app {name}")
        } else {
            format!(
                "No window of {name} is on screen: open one in it, or run atlas wait --app {name}"
            )
        },
    ))
}

/// Whether an `apps` row is the app called `name`: by the name it shows,
/// which may be localized, or by the file name of its executable or of its
/// own (innermost) bundle.
fn app_named(app: &Value, name: &str) -> bool {
    let shown = app["name"].as_str().is_some_and(|own| same_text(own, name));
    let executable = app["executable"].as_str().map(std::path::Path::new);
    let bundle = executable.and_then(|path| {
        path.ancestors()
            .find(|ancestor| ancestor.extension().is_some_and(|ext| ext == "app"))
    });

    shown
        || [executable, bundle]
            .into_iter()
            .flatten()
            .filter_map(|path| path.file_stem()?.to_str())
            .any(|own| same_text(own, name))
}

/// Most accessibility failures a scan's diagnostics name.
const SCAN_ERRORS: usize = 8;

/// Longest failure key a scan's diagnostics name, in bytes.
const SCAN_ERROR_KEY: usize = 64;

/// Most elements a refused name lists as what it may have meant.
const CANDIDATES: usize = 5;

/// One scan of an app, as the probe reported it.
struct Scanned {
    elements: Vec<Element>,
    incomplete: bool,
    /// Why it is incomplete, each reason once.
    reasons: Vec<Stop>,
    /// Accessibility failures other than a missing attribute, by count.
    errors: BTreeMap<String, usize>,
}

impl Scanned {
    /// The probe's reply, read. An older probe names no reasons.
    fn from_probe(mut value: Value) -> Result<Self> {
        if value["ok"] != true {
            return Err(Error::new(
                "elements",
                value["error"]
                    .as_str()
                    .unwrap_or("The accessibility probe failed")
                    .to_owned(),
            ));
        }

        let elements = serde_json::from_value(value["elements"].take())
            .map_err(|_| Error::new("elements", "Malformed accessibility elements"))?;

        let mut reasons: Vec<Stop> = value["reasons"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .filter_map(Stop::parse)
            .collect();
        reasons.sort_unstable();
        reasons.dedup();

        let mut errors: Vec<(String, usize)> = value["errors"]
            .as_object()
            .into_iter()
            .flatten()
            .filter(|(key, _)| key.len() <= SCAN_ERROR_KEY && ax::telling(key))
            .filter_map(|(key, count)| Some((key.clone(), usize::try_from(count.as_u64()?).ok()?)))
            .collect();
        errors.sort_by(|(a, first), (b, second)| second.cmp(first).then_with(|| a.cmp(b)));
        errors.truncate(SCAN_ERRORS);

        Ok(Self {
            elements,
            incomplete: value["truncated"] == true,
            reasons,
            errors: errors.into_iter().collect(),
        })
    }

    /// Why the scan stopped early and what failed in it, for programs:
    /// None when there is nothing to say.
    fn diagnostics(&self) -> Option<Value> {
        let mut said = serde_json::Map::new();
        if !self.reasons.is_empty() {
            said.insert("reasons".into(), json!(self.reasons));
        }
        if !self.errors.is_empty() {
            said.insert("errors".into(), json!(self.errors));
        }

        (!said.is_empty()).then_some(Value::Object(said))
    }

    /// A named click refused because the scan was incomplete.
    fn refusal(&self) -> Error {
        let why = ax::why(self.reasons.iter().copied())
            .map(|why| format!(" ({why})"))
            .unwrap_or_default();
        let mut error = Error::new(
            "elements",
            format!(
                "The accessibility scan was incomplete{why}, so a unique match cannot be \
                 established"
            ),
        );
        if let Some(diagnostics) = self.diagnostics() {
            error = error.with("scan", diagnostics);
        }

        error.with(
            "next",
            "Try again. If it stays incomplete, click the point from a screenshot instead.",
        )
    }
}

/// The accessibility elements of process `pid`, scanned in the helper
/// `verified` describes, which the caller verified just before.
async fn app_elements(client: &Client, verified: &Session, pid: i64) -> Result<Scanned> {
    let probe = client.config.identity_probe.to_string_lossy().into_owned();
    let argv = vec![probe, "elements-info".into(), pid.to_string()];
    let reply = client
        .inspect_in(verified, argv, crate::client::INSPECT_TIMEOUT)
        .await?;
    let value = run_output(&reply, 8 << 20)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .ok_or_else(|| Error::new("elements", "The accessibility probe returned no result"))?;

    Scanned::from_probe(value)
}

/// Accessibility elements of the target app's windows that match `filter`,
/// at most `max` of them, listed compactly unless `full`.
pub async fn elements(
    client: &Client,
    target: &Target,
    filter: &ElementFilter,
    max: usize,
    full: bool,
) -> Result<Value> {
    let verified = client.identity().await?;
    let pid = target_pid(client, &verified, target).await?;
    let scanned = app_elements(client, &verified, pid).await?;

    Ok(listing(pid, &scanned, filter, max, full))
}

/// Save a screenshot of the whole display, of `target`'s windows, or of
/// `region`, or name the earlier file when nothing changed (unless
/// `new_file`). The app is found as `elements` finds it.
pub async fn screenshot(
    client: &Client,
    target: Option<&Target>,
    region: Option<Region>,
    size: crate::client::Size,
    new_file: bool,
) -> Result<Value> {
    let crop = match (target, region) {
        (Some(_), Some(_)) => {
            return Err(Error::new(
                "usage",
                "Give an app or a region to capture, not both",
            ));
        }
        (Some(target), None) => Some(Crop::App {
            pid: target_pid(client, &client.identity().await?, target).await?,
        }),
        (None, region) => region.map(Crop::Region),
    };

    client
        .capture(size, crop.as_ref(), new_file)
        .await
        .map_err(|error| match target {
            Some(Target::Name(name)) if error.code == "no-window" => Error::new(
                "no-window",
                format!(
                    "{name} has no window on screen: open one, or take a full screenshot to see \
                     its menu bar"
                ),
            ),
            _ => error,
        })
}

/// The elements of one scan that match `filter`, as `elements` lists them:
/// each by its role, name, value, click point and size, its window listed
/// once, or with every attribute the scan read when `full`. A name that
/// matches nothing lists the closest names.
fn listing(pid: i64, scanned: &Scanned, filter: &ElementFilter, max: usize, full: bool) -> Value {
    let matching: Vec<&Element> = scanned
        .elements
        .iter()
        .filter(|element| filter.matches(element))
        .collect();
    let shown = &matching[..max.min(matching.len())];

    let mut listed = json!({
        "ok": true,
        "pid": pid,
        "scanned": scanned.elements.len(),
        "scanIncomplete": scanned.incomplete,
        "matching": matching.len(),
        "more": matching.len() - shown.len(),
    });

    if full {
        listed["elements"] = json!(shown);
    } else {
        let mut windows: Vec<&Frame> = Vec::new();
        listed["elements"] = shown
            .iter()
            .map(|element| compact(element, &mut windows))
            .collect();
        listed["windows"] = windows
            .iter()
            .map(|frame| {
                json!({
                    "x": whole(frame.x),
                    "y": whole(frame.y),
                    "width": whole(frame.width),
                    "height": whole(frame.height),
                })
            })
            .collect();
    }

    // What failed in a complete scan (an app's own getter answering
    // failure, say) changes nothing an agent does.
    if let Some(diagnostics) = scanned.diagnostics().filter(|_| scanned.incomplete) {
        listed["scan"] = diagnostics;
    }

    if filter.name.is_some() && matching.is_empty() {
        let candidates = candidates(&in_play(&scanned.elements), &[], filter);
        if !candidates.is_empty() {
            listed["candidates"] = json!(candidates);
        }
    }

    listed
}

/// An element as the default listing names it, with its window's place in
/// `windows` (added when new).
fn compact<'a>(element: &'a Element, windows: &mut Vec<&'a Frame>) -> Value {
    let mut listed = json!({"index": element.index});
    let name = shown_name(element);

    if let Some(role) = &element.role {
        listed["role"] = json!(role);
    }
    if let Some(name) = name {
        listed["name"] = json!(name);
    }
    if let Some(value) = element
        .value
        .as_deref()
        .filter(|value| !value.is_empty() && Some(*value) != name)
    {
        listed["value"] = json!(value);
    }
    if let Some(frame) = &element.frame {
        let (x, y) = click_point(element.role.as_deref(), frame);
        listed["at"] = json!([whole(x), whole(y)]);
        listed["size"] = json!([whole(frame.width), whole(frame.height)]);
    }
    if let Some(window) = &element.window {
        let place = windows
            .iter()
            .position(|seen| seen.matches(window))
            .unwrap_or_else(|| {
                windows.push(window);
                windows.len() - 1
            });
        listed["window"] = json!(place);
    }

    let flags = [
        ("disabled", element.enabled == Some(false)),
        ("focused", element.focused == Some(true)),
        ("selected", element.selected == Some(true)),
        ("menu", element.menu),
    ];
    for (flag, set) in flags {
        if set {
            listed[flag] = json!(true);
        }
    }

    if let Some(mark) = &element.mark {
        listed["mark"] = json!(mark);
    }
    if !element.truncated.is_empty() {
        listed["truncated"] = json!(element.truncated);
    }
    if !element.unreadable.is_empty() {
        listed["unreadable"] = json!(element.unreadable);
    }

    listed
}

/// The CoreGraphics window whose bounds are an element's AX window frame,
/// or None when no window or more than one window of the process matches.
fn owning_window(windows: &[Window], pid: i64, frame: &Frame) -> Option<i64> {
    let near = |a: f64, b: f64| (a - b).abs() <= 1.0;
    let matching: Vec<&Window> = windows
        .iter()
        .filter(|window| {
            window.pid == pid
                && app_or_menu_layer(window)
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

/// The menu bar where it shows the point, which is the Window Server's
/// window rather than the app's.
fn menu_bar_window(windows: &[Window], x: f64, y: f64) -> Option<i64> {
    windows
        .iter()
        .find(|window| window.is_menu_bar() && window.alpha > 0.0 && window.bounds.contains(x, y))
        .map(|window| window.number)
}

/// The process of the active application in the helper: the one whose
/// titles the menu bar shows.
async fn active_pid(client: &Client) -> Result<Option<i64>> {
    let apps = client.apps().await?;
    Ok(apps["apps"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|app| app["active"] == true)
        .and_then(|app| app["pid"].as_i64()))
}

/// Whether a frame has any area to click: a collapsed element is not on
/// screen, whatever lies under its point.
fn drawn(frame: &Frame) -> bool {
    frame.width > 0.0 && frame.height > 0.0
}

/// The one element a click by name or role is for. While a menu is open
/// only its items count (a click outside it just closes the menu). A whole
/// name wins over the first part of a longer label, judged over every match
/// before the one chosen must have a frame. Refused when no visible element
/// or several match, or when another element whose name could not be read
/// might match too.
fn choose<'a>(elements: &'a [Element], filter: &ElementFilter) -> Result<&'a Element> {
    let elements = in_play(elements);
    // What is in play is either an open menu's items or no menu item at all.
    let menu_open = elements.first().is_some_and(|element| element.menu);

    let matches: Vec<(&Element, NameMatch)> = elements
        .iter()
        .filter_map(|element| Some((*element, filter.match_of(element)?)))
        .collect();
    let best = matches.iter().map(|(_, kind)| *kind).max();
    let matching: Vec<&Element> = matches
        .iter()
        .filter(|(element, kind)| Some(*kind) == best && element.frame.as_ref().is_some_and(drawn))
        .map(|(element, _)| *element)
        .collect();

    // A menu bar title gives way to a window element of the same name
    // (a Help button beside a Help menu); --role AXMenuBarItem asks for it.
    let in_windows: Vec<&Element> = matching
        .iter()
        .copied()
        .filter(|element| element.role.as_deref() != Some("AXMenuBarItem"))
        .collect();
    let matching = if filter.role.is_none() && in_windows.len() == 1 {
        in_windows
    } else {
        matching
    };

    let [element] = matching.as_slice() else {
        let candidates = candidates(&elements, &matching, filter);
        let closest = if candidates.is_empty() {
            ""
        } else {
            " The closest names are in candidates."
        };
        let error = Error::new(
            "elements",
            match matching.len() {
                0 if menu_open => format!(
                    "No item of the open menu matches. Run atlas elements --app NAME to see it, \
                     or press escape to close the menu.{closest}"
                ),
                0 => format!(
                    "No visible element matches. Run atlas elements --app NAME to see what is \
                     there.{closest}"
                ),
                count => format!(
                    "{count} elements match. Run atlas elements with the same --name to \
                     tell them apart, then add --role, or click the one you mean at its point."
                ),
            },
        );

        return Err(if candidates.is_empty() {
            error
        } else {
            error.with("candidates", candidates)
        });
    };

    // An element whose selector attributes could not be read may be a
    // second, unseen match, unless it is collapsed and could never be chosen.
    let doubtful = elements.iter().any(|other| {
        other.index != element.index
            && (!other.unreadable.is_empty() || !other.truncated.is_empty())
            && other.frame.as_ref().is_none_or(drawn)
            && filter.might_match(other)
    });
    if doubtful {
        return Err(Error::new(
            "elements",
            "Another element's name could not be read whole, so this match may not be unique",
        ));
    }

    Ok(element)
}

/// The elements a name is judged against: while a menu is open, only its
/// items (a click outside it just closes the menu).
fn in_play(elements: &[Element]) -> Vec<&Element> {
    let menu_open = elements.iter().any(|element| element.menu);
    elements
        .iter()
        .filter(|element| element.menu || !menu_open)
        .collect()
}

/// Up to five elements a refused name may have meant, each with its role,
/// name and click point: on several matches, those matches; on none, the
/// visible elements whose names are closest to the name asked for.
fn candidates(seen: &[&Element], matching: &[&Element], filter: &ElementFilter) -> Vec<Value> {
    let at = |element: &Element| {
        element
            .frame
            .as_ref()
            .map(|frame| click_point(element.role.as_deref(), frame))
    };

    if !matching.is_empty() {
        return matching
            .iter()
            .filter_map(|element| Some(clicked(element, at(element)?)))
            .take(CANDIDATES)
            .collect();
    }

    let Some(name) = filter.name.as_deref() else {
        return Vec::new();
    };
    let query: String = name.chars().flat_map(char::to_lowercase).collect();

    // Each element's closest name: by how it is close, then how far off.
    let mut close: Vec<((u8, usize, usize), &Element, &str)> = seen
        .iter()
        .filter(|element| element.frame.as_ref().is_some_and(drawn))
        .filter_map(|element| {
            names(element)
                .filter_map(|text| Some((closeness(&query, text)?, text)))
                .min()
                .map(|((tier, distance), text)| ((tier, distance, element.index), *element, text))
        })
        .collect();
    close.sort_by_key(|(rank, ..)| *rank);

    let mut listed: Vec<Value> = Vec::new();
    for (_, element, text) in close {
        let Some(point) = at(element) else {
            continue;
        };
        let mut candidate = clicked(element, point);
        candidate["name"] = json!(text);

        if !listed.contains(&candidate) {
            listed.push(candidate);
        }
        if listed.len() == CANDIDATES {
            break;
        }
    }

    listed
}

/// How close `text` is to `query` (already lowercased), if at all: the same
/// text under another role (0), one holding the other (1), or a few
/// characters off (2), with how far off it is.
fn closeness(query: &str, text: &str) -> Option<(u8, usize)> {
    let text: String = text.chars().flat_map(char::to_lowercase).collect();
    let (wanted, length) = (query.chars().count(), text.chars().count());
    let apart = wanted.abs_diff(length);

    if text.is_empty() {
        return None;
    }

    if text == query {
        return Some((0, 0));
    }

    if wanted.min(length) >= 2 && (text.contains(query) || query.contains(text.as_str())) {
        return Some((1, apart));
    }

    // A short query is a few characters off most short names ("OK" from
    // "No" and "Go"), so only a longer one is matched that way.
    if wanted < 4 {
        return None;
    }

    let limit = (wanted / 3).max(1);
    if apart > limit {
        return None;
    }

    let distance = edits(query, &text);
    (distance <= limit).then_some((2, distance))
}

/// How many characters must be added, removed or changed to turn one text
/// into the other.
fn edits(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();

    for (i, x) in a.chars().enumerate() {
        let mut diagonal = row[0];
        row[0] = i + 1;
        for (j, y) in b.iter().enumerate() {
            let above = row[j + 1];
            row[j + 1] = (above + 1)
                .min(row[j] + 1)
                .min(diagonal + usize::from(x != *y));
            diagonal = above;
        }
    }

    row[b.len()]
}

/// Click exactly one matching element at its click point. Uniqueness needs a
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
    let verified = client.identity().await?;
    let pid = target_pid(client, &verified, target).await?;
    let scanned = app_elements(client, &verified, pid).await?;
    let epoch = verified.epoch;
    if scanned.incomplete {
        return Err(scanned.refusal());
    }

    let element = choose(&scanned.elements, filter)?;
    let (x, y) = element
        .frame
        .as_ref()
        .map(|frame| click_point(element.role.as_deref(), frame))
        .unwrap_or_default();

    // A fresh list, read after the scan, judged for the element's own window
    // as the helper judges a point: the Dock covers what is under it where
    // it takes clicks.
    let Shown { windows, dock } = pointer_view(client).await?;
    let display = Bounds::display(epoch.display.width, epoch.display.height);
    let screen = Screen::new(display, dock);
    let window = if element.role.as_deref() == Some("AXMenuBarItem") {
        // The menu bar shows the active app's titles, and only those.
        if active_pid(client).await? != Some(pid) {
            return Err(Error::new(
                "elements",
                "The app is no longer active, so the menu bar does not show its titles",
            ));
        }

        menu_bar_window(&windows, x, y)
    } else {
        element
            .window
            .as_ref()
            .and_then(|frame| owning_window(&windows, pid, frame))
    };

    if !window.is_some_and(|window| unobstructed(&windows, window, x, y, &screen)) {
        return Err(Error::new(
            "elements",
            hidden_element(&windows, x, y, &screen),
        ));
    }

    // The name it was found by is kept only as a control's caption: any
    // other element's name may be a document's text, a file or a title.
    let uncaptioned;
    let client = if crate::doing::captioned(element.role.as_deref()) {
        client
    } else {
        uncaptioned = client.clone().unlabelled();
        &uncaptioned
    };

    let actions = click(x, y, which, count, modifiers);
    // The helper repeats the visibility check under its own lock where it
    // can; an older one is not needed for it, since the check above ran.
    let check = Check::Offered(Expect::new(pid)?);
    let mut result = client
        .input_owned(&ownership, &actions, &epoch, Some(check))
        .await?;
    result["element"] = clicked(element, (x, y));

    Ok(result)
}

/// Why a named click's element at the point is not visible: the Dock in
/// front of it, which may be one Atlas could not read, or anything else.
fn hidden_element(windows: &[Window], x: f64, y: f64, screen: &Screen) -> &'static str {
    let docked = top_at(windows, x, y, screen).is_some_and(Window::is_dock);
    match (docked, screen.dock()) {
        (true, Dock::Unread) => {
            "Atlas could not read where the Dock takes clicks, so it cannot tell whether the Dock covers the element; try again"
        }
        (true, _) => "The Dock covers the element, so a click there would reach the Dock",
        (false, _) => "The element is not visible in its window: covered, moved or off screen",
    }
}

/// Where a click on an element lands: the middle of its frame, or for a
/// checkbox or radio button with a label beside it (a frame much wider than
/// tall) the middle of its box, at the leading end of a frame that may also
/// span a long empty stretch after a short label. A tab drawn as a radio
/// button is clicked in its middle.
fn click_point(role: Option<&str>, frame: &Frame) -> (f64, f64) {
    match role {
        Some("AXCheckBox" | "AXRadioButton") if frame.width > 2.0 * frame.height => (
            (frame.x + frame.height / 2.0).round(),
            (frame.y + frame.height / 2.0).round(),
        ),
        _ => frame.center(),
    }
}

/// A coordinate or length in whole points, as replies give them.
fn whole(points: f64) -> i64 {
    points.round() as i64
}

/// A clicked element as its reply names it: role, name and the point clicked.
fn clicked(element: &Element, (x, y): (f64, f64)) -> Value {
    let mut named = json!({"at": [whole(x), whole(y)]});
    if let Some(role) = &element.role {
        named["role"] = json!(role);
    }
    if let Some(name) = shown_name(element) {
        named["name"] = json!(name);
    }

    named
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{native::windows::MENU_BAR_LAYER, session::Display};

    /// An app found by name or process ID is looked for in the helper the
    /// command verified moments before: the lookup and the scan after it
    /// verify the helper once between them, not once for each list read.
    #[tokio::test]
    async fn finding_an_app_verifies_the_helper_once() {
        use crate::helper_test::{Helper, UID, completed};

        let directory = tempfile::tempdir().unwrap();
        let (client, helper) = Helper::client(directory.path(), UID);
        let window = json!({"number": 5, "pid": 812, "owner": "TextEdit", "title": null,
            "layer": 0, "alpha": 1.0, "bounds": {"x": 0, "y": 0, "width": 100, "height": 100}});
        let answer = |request: &Value| {
            if request["op"] == "apps" {
                return Some(json!({"ok": true, "apps": [{"pid": 812, "name": "TextEdit"}]}));
            }
            let output = match request["argv"][1].as_str() {
                Some("windows-info") => {
                    json!({"ok": true, "windows": [window.clone()]})
                }
                Some("elements-info") => json!({"ok": true, "elements": [], "truncated": false}),
                other => panic!("nothing else is read: {other:?}"),
            };
            Some(completed(&output.to_string()))
        };

        for target in [Target::Name("TextEdit".into()), Target::Pid(812)] {
            let before = helper.proofs();
            let listed = helper
                .serve(
                    elements(&client, &target, &ElementFilter::default(), 10, false),
                    answer,
                )
                .await
                .unwrap();
            assert_eq!(listed["pid"], 812, "{target:?}");
            assert_eq!(helper.proofs() - before, 1, "{target:?}");
        }
    }

    /// Only a check of a point pays for reading the Dock: listing windows
    /// and waiting for them do not ask for it.
    #[test]
    fn only_a_point_check_asks_the_window_probe_for_the_dock() {
        let directory = tempfile::tempdir().unwrap();
        let client = crate::transaction_test::client(directory.path());
        let skip = windows_probe(&client, DockRead::Skip);
        let read = windows_probe(&client, DockRead::Read);
        assert_eq!(skip[1..], ["windows-info"]);
        assert_eq!(read[1..], ["windows-info", "--dock"]);
    }

    /// A named click refused under the Dock says it is the Dock, and when
    /// the Dock could not be read, that Atlas could not tell.
    #[test]
    fn a_named_click_under_the_dock_says_it_is_the_dock() {
        let dock = Window {
            number: 1,
            pid: 2,
            owner: Some("Dock".into()),
            title: None,
            layer: 20,
            alpha: 1.0,
            bounds: Bounds::display(1000.0, 800.0),
        };
        let app = Window {
            number: 3,
            owner: Some("TextEdit".into()),
            layer: 0,
            pid: 4,
            ..dock.clone()
        };
        let windows = [dock, app];
        let display = Bounds::display(1000.0, 800.0);
        let icons = Dock::Icons(vec![Bounds {
            x: 200.0,
            y: 740.0,
            width: 600.0,
            height: 60.0,
        }]);
        let said =
            |dock: Dock, y| hidden_element(&windows, 300.0, y, &Screen::new(display.clone(), dock));

        assert!(said(icons.clone(), 770.0).starts_with("The Dock covers"));
        assert!(said(Dock::Other, 300.0).starts_with("The Dock covers"));
        assert!(said(Dock::Unread, 300.0).contains("could not read where the Dock takes clicks"));
        assert!(said(icons, 300.0).starts_with("The element is not visible"));
    }

    /// The process a name resolves to from the window list alone.
    fn app_pid(windows: &[Window], target: &Target) -> Result<i64> {
        match target {
            Target::Pid(pid) => Ok(*pid),
            Target::Name(name) => one_pid(name, &app_pids(windows, name)),
        }
    }

    /// An `apps` row is found by its shown name or its bundle's file name.
    #[test]
    fn an_app_row_is_named_by_its_shown_or_file_name() {
        let row = json!({
            "name": "Éditeur de texte",
            "executable": "/System/Applications/TextEdit.app/Contents/MacOS/TextEdit",
        });
        assert!(app_named(&row, "éditeur de texte"));
        assert!(app_named(&row, "TextEdit"));
        assert!(
            !app_named(&row, "Contents"),
            "a folder on the way is not a name"
        );
        assert!(!app_named(&json!({"name": "Finder"}), "TextEdit"));

        let nested = json!({
            "name": "Simulator",
            "executable": "/Applications/Xcode.app/Contents/Developer/Applications/Simulator.app/Contents/MacOS/Simulator",
        });

        assert!(
            !app_named(&nested, "Xcode"),
            "the bundle it lives in is not its name"
        );
    }

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

    /// A menu bar title is judged against the menu bar, the Window Server's
    /// window at the point to click, and only while that is shown; an app's
    /// own window at that layer is not the bar.
    #[test]
    fn the_menu_bar_is_the_window_servers() {
        let bar = |owner: &str, alpha: f64| {
            let mut bar = window(3, 433, owner, MENU_BAR_LAYER);
            bar.alpha = alpha;
            bar.bounds = Bounds {
                x: 0.0,
                y: 0.0,
                width: 1512.0,
                height: 30.0,
            };

            bar
        };
        let app = || window(9, 6, "Opie", 0);

        assert_eq!(
            menu_bar_window(&[bar("Window Server", 1.0), app()], 232.0, 12.0),
            Some(3)
        );
        assert_eq!(
            menu_bar_window(&[bar("Window Server", 1.0), app()], 232.0, 60.0),
            None
        );
        assert_eq!(
            menu_bar_window(&[bar("Window Server", 0.0), app()], 232.0, 12.0),
            None
        );
        assert_eq!(
            menu_bar_window(&[bar("Opie", 1.0), app()], 232.0, 12.0),
            None
        );
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

        for command in [
            "doctor",
            "windows",
            "screenshot",
            "elements",
            "click",
            "wait",
            "open",
            "run",
            "restart",
            "close",
            "environment rollback",
            "monitor",
        ] {
            assert!(CORE.contains(&format!("atlas {command}")), "{command}");
        }
    }

    /// The core with its lines joined, so a phrase may wrap anywhere.
    fn core_text() -> String {
        CORE.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    /// Where the skill's references live, beside its SKILL.md.
    fn references_folder() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("agents/atlas/skills/atlas/references")
    }

    #[test]
    fn the_skill_is_the_core_followed_by_its_references() {
        let skill = skill();
        let (_, body) = skill.split_once("\n---\n\n# Atlas\n\n").unwrap();
        let (_, core) = CORE.split_once('\n').unwrap();
        let rest = body.strip_prefix(core.trim_start()).unwrap();
        let mut expected = String::from("\n## References\n\nRead one when the task needs it:\n\n");
        for (file, about) in REFERENCES {
            expected.push_str(&format!(
                "- [references/{file}](references/{file}): {about}\n"
            ));
        }
        assert_eq!(rest, expected);
    }

    /// Every agent loads the whole skill at the start of each task. When it
    /// grows past this, move detail into a reference rather than raise it.
    #[test]
    fn the_skill_stays_near_two_thousand_tokens() {
        let size = skill().len();
        assert!(size <= 8 * 1024, "the skill is {size} bytes");
    }

    #[test]
    fn the_skill_core_keeps_every_rule_an_agent_needs_from_the_start() {
        let core = core_text();
        let actor = ["ATLAS", "ACTOR"].join("_");
        for rule in [
            "busy",
            "holder",
            "stopped",
            "\"uncertain\": true",
            "Do not run `atlas restart`",
            "on-console",
            "blocked",
            "session-changed",
            actor.as_str(),
            "privacy settings",
            "credentials",
            "atlas switch",
            "atlas logout",
            "Never repeat",
            "this task opened",
            "unsaved work",
            "without a screenshot that shows it",
            "show_screen",
            "&&",
            "unchanged: true",
        ] {
            assert!(core.contains(rule), "{rule}");
        }
    }

    #[test]
    fn no_step_quits_or_discards_what_the_task_did_not_open() {
        let core = core_text();
        for (index, _) in core.match_indices("close --all") {
            assert!(
                core[..index].ends_with("never run `atlas "),
                "close --all is taught: {}",
                &core[index.saturating_sub(60)..index]
            );
        }
        assert!(!core.to_lowercase().contains("cmd+backspace"));
    }

    #[test]
    fn the_full_guide_is_the_core_then_every_reference_written_for_it() {
        let mut expected = format!("{CORE}\n");
        for file in ["testing.md", "environment.md", "limits.md"] {
            assert!(
                REFERENCES.iter().any(|(listed, _)| *listed == file),
                "{file}"
            );
            let text = std::fs::read_to_string(references_folder().join(file)).unwrap();
            assert!(text.starts_with("## "), "{file} starts with its heading");
            expected.push_str(&text);
            expected.push('\n');
        }
        let rest = GUIDE.strip_prefix(&expected).unwrap();
        assert!(rest.starts_with("## References\n"), "{rest}");
        assert!(rest.contains("`atlas agent-guide --commands`"), "{rest}");
        assert!(rest.contains("`atlas agent-guide --errors`"), "{rest}");
    }

    #[test]
    fn every_reference_the_skill_lists_is_in_its_folder_and_nothing_else_is() {
        let mut present: Vec<String> = std::fs::read_dir(references_folder())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        present.sort();
        let mut listed: Vec<String> = REFERENCES
            .iter()
            .map(|(file, _)| (*file).to_owned())
            .collect();
        listed.sort();
        assert_eq!(present, listed);
    }

    /// The core sends an agent to these sections by name: each heads a part
    /// of the full guide and is how the skill's References list names it.
    #[test]
    fn every_section_the_core_points_to_exists() {
        let core = core_text();
        for section in ["Test your own app", "Give the app what it needs", "Limits"] {
            assert!(core.contains(&format!("read {section}")), "{section}");
            assert!(GUIDE.contains(&format!("\n## {section}\n")), "{section}");
            assert!(
                REFERENCES
                    .iter()
                    .any(|(_, about)| about.starts_with(section)),
                "{section}"
            );
        }
    }

    #[test]
    fn agents_are_told_to_read_both_output_streams_on_failure() {
        for text in [GUIDE.to_owned(), skill()] {
            let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
            assert!(text.contains("exit non-zero on failure; then read both stdout and stderr."));
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
        assert_eq!(click(1.0, 2.0, Button::Right, 1, &[])[0]["button"], "right");
        assert_eq!(
            drag((1.0, 2.0), (3.0, 4.0), Button::Middle)[0]["button"],
            "middle"
        );

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
        assert!(
            batches.iter().all(|t| t.chars().count() <= TYPE_BATCH),
            "each part is short enough to finish in time on a busy Mac"
        );
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

        let long_cluster = format!("a{}", "\u{301}".repeat(99));
        let batches = type_batches(&format!("xy{long_cluster}z")).unwrap();
        assert_eq!(
            batches,
            ["xy", long_cluster.as_str(), "z"],
            "a long cluster goes alone"
        );
    }

    #[test]
    fn text_is_typed_in_parts_of_at_most_sixteen_characters() {
        let plain = "x".repeat(64);
        let parts = type_parts(&plain);
        assert_eq!(parts.len(), 4);
        assert!(parts.iter().all(|part| part.chars().count() == TYPE_PART));

        let family = "👨‍👩‍👧‍👦";
        let text = format!("{}{family}{}", "a".repeat(14), "b".repeat(20));
        let parts = type_parts(&text);
        assert_eq!(parts.concat(), text);
        assert!(parts.iter().any(|part| part.contains(family)), "kept whole");
        assert!(parts.iter().all(|part| part.chars().count() <= TYPE_PART));

        let decomposed = "e\u{301}".repeat(20);
        let parts = type_parts(&decomposed);
        assert!(parts.iter().all(|part| !part.starts_with('\u{301}')));
        assert_eq!(parts.concat(), decomposed);

        let long_cluster = format!("a{}", "\u{301}".repeat(30));
        assert_eq!(
            type_parts(&format!("xy{long_cluster}z")),
            ["xy", long_cluster.as_str(), "z"],
            "a cluster longer than a part goes alone"
        );

        for batch in type_batches(&"y".repeat(200)).unwrap() {
            let parts: Vec<Value> = type_parts(&batch)
                .iter()
                .map(|part| json!({"type": "type", "text": part}))
                .collect();
            assert!(parts.len() <= TYPE_BATCH / TYPE_PART);
            crate::actions::validate(&Value::Array(parts), &display()).unwrap();
        }
    }

    #[test]
    fn a_stopped_type_counts_the_parts_the_helper_performed() {
        let parts = type_parts(&"z".repeat(64));
        let refused = json!({"ok": false, "code": "focus-changed", "reason": "app",
            "found": {"app": "Calculator", "pid": 900}, "error": "Calculator has keyboard focus",
            "performed": 2, "partial": false, "uncertain": false, "cleanupError": "kept out"});
        let reply = |value: &Value| Unconfirmed::from_reply(value);

        let report = stopped_typing(64, 3, 1, &parts, reply(&refused));
        assert_eq!(report["typed"], 64 + 32);
        assert_eq!(report["batches"], 3);
        assert_eq!(report["confirmedBatches"], 1);
        assert_eq!(report["partial"], true);
        assert_eq!(report["uncertain"], false);
        assert_eq!(report["code"], "focus-changed");
        assert_eq!(report["error"], "Calculator has keyboard focus");
        assert_eq!(report["reason"], "app");
        assert_eq!(report["found"], refused["found"]);
        assert!(
            report.get("cleanupError").is_none(),
            "only what agents act on"
        );

        let first = stopped_typing(
            0,
            1,
            0,
            &parts,
            reply(&json!({"performed": 0, "code": "focus-changed"})),
        );
        assert_eq!(
            (first["typed"].clone(), first["partial"].clone()),
            (json!(0), json!(false))
        );
        let broke = reply(&json!({"performed": 0, "partial": true, "uncertain": true}));
        assert_eq!(stopped_typing(0, 1, 0, &parts, broke)["partial"], true);

        let lost = Error::new("transport", "Native reply timed out").uncertain();
        let report = stopped_typing(64, 2, 1, &parts, lost.into());
        assert_eq!(report["typed"], 64, "no part of a failed batch counts");
        assert_eq!(report["code"], "transport");
        assert_eq!(report["uncertain"], true);
        assert!(report.get("reason").is_none() && report.get("found").is_none());

        let busy = Error::new("busy", "held").held_by(Holder::Agent);
        assert_eq!(
            stopped_typing(0, 1, 0, &parts, busy.into())["holder"],
            "agent"
        );
        let busy = reply(&json!({"code": "busy", "holder": "person"}));
        assert_eq!(stopped_typing(0, 1, 0, &parts, busy)["holder"], "person");

        let unknown = stopped_typing(0, 1, 0, &parts, reply(&json!({"code": "made-up"})));
        assert_eq!(unknown["code"], "input");
        assert_eq!(unknown["error"], "The helper did not confirm the text");
    }

    #[tokio::test]
    async fn typing_stops_between_parts_and_says_how_much_was_typed() {
        use nix::sys::signal::Signal;

        let interrupt = Interrupt::new(Duration::from_secs(10));
        let batches = type_batches(&"x".repeat(200)).unwrap();
        assert_eq!(batches.len(), 4);
        let mut sent = 0;

        let report = type_each(&batches, &interrupt, async |actions: &Value| {
            sent += 1;
            interrupt.ask(Signal::SIGTERM);
            Ok(json!({"ok": true, "performed": actions.as_array().unwrap().len()}))
        })
        .await;
        assert_eq!(sent, 1, "nothing more is sent once a stop was asked");
        assert_eq!(report["ok"], false);
        assert_eq!(report["code"], "interrupted");
        assert_eq!(report["uncertain"], false);
        assert_eq!(report["typed"], 64);
        assert_eq!(report["batches"], 4);
        assert_eq!(report["confirmedBatches"], 1);

        let interrupt = Interrupt::new(Duration::from_secs(10));
        let mut first = true;
        let report = type_each(&batches, &interrupt, async |actions: &Value| {
            if std::mem::take(&mut first) {
                Ok(json!({"ok": true, "performed": actions.as_array().unwrap().len()}))
            } else {
                Err(interrupt.cut_short(Signal::SIGTERM))
            }
        })
        .await;
        assert_eq!(report["code"], "interrupted");
        assert_eq!(report["uncertain"], true, "the part under way was cut");
        assert_eq!(report["typed"], 64, "only confirmed parts count");
    }

    /// A look never asks the helper for longer than it allows a command,
    /// however long the wait.
    #[test]
    fn a_look_stays_within_the_helpers_limit() {
        assert_eq!(probe_budget(Duration::ZERO), Duration::from_millis(500));
        assert_eq!(probe_budget(Duration::from_secs(9)), Duration::from_secs(9));
        assert_eq!(
            probe_budget(Duration::from_secs(86_400)),
            crate::client::RUN_TIMEOUT_MAX
        );

        let longest = crate::client::RUN_TIMEOUT_MAX.as_millis();
        let request = json!({"op": "run", "argv": ["x"], "timeoutMs": longest}).to_string();
        assert!(
            crate::daemon::protocol::parse(request.as_bytes()).is_ok(),
            "the helper accepts the longest look"
        );
    }

    /// Every command compares app names the same way, beyond ASCII too.
    #[test]
    fn app_names_match_in_any_case() {
        let windows = [window(1, 8, "Éditeur", 0)];
        assert_eq!(
            app_pid(&windows, &Target::Name("éditeur".into())).unwrap(),
            8
        );

        let filter = WindowFilter {
            app: Some("ÉDITEUR".into()),
            ..WindowFilter::default()
        };
        assert!(filter.matches(&windows[0]));
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
        assert!(!by_title.matches(&window(5, 6, "Preview", 0)));

        assert!(!WindowFilter::default().matches(&menu));
        assert!(
            WindowFilter {
                all: true,
                ..Default::default()
            }
            .matches(&menu)
        );

        let alert = window(7, 8, "CoreServicesUIAgent", 8);
        assert!(
            WindowFilter::default().matches(&alert),
            "an alert is listed"
        );
        assert!(
            !WindowFilter::default().matches(&window(9, 10, "Dock", 20)),
            "the Dock is not"
        );
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
            selected: None,
            frame: None,
            unreadable: Vec::new(),
            truncated: Vec::new(),
            menu: false,
            mark: None,
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
    fn a_named_click_reports_only_role_name_and_point() {
        let mut button = element("AXButton", Some("Save"), Some("Save the file"));
        button.identifier = Some("save".into());
        assert_eq!(
            clicked(&button, (25.0, 25.0)),
            json!({"at": [25, 25], "name": "Save", "role": "AXButton"})
        );

        let mut label = element("AXStaticText", Some(""), None);
        label.value = Some("Main".into());
        assert_eq!(
            clicked(&label, (3.0, 4.0)),
            json!({"at": [3, 4], "name": "Main", "role": "AXStaticText"})
        );

        let mut field = element("AXTextField", None, None);
        field.value = Some("typed".into());
        assert_eq!(
            clicked(&field, (3.0, 4.0)),
            json!({"at": [3, 4], "role": "AXTextField"})
        );
    }

    /// A label that joins several parts (SwiftUI's combined rows) is
    /// named by its first part too, but a whole name is the better match.
    #[test]
    fn a_combined_label_is_named_by_its_first_part() {
        let filter = ElementFilter {
            role: None,
            name: Some("qa eject".into()),
        };
        let row = element("AXButton", None, Some("QA Eject, Option-Space, 6 Slices"));
        assert_eq!(filter.match_of(&row), Some(NameMatch::Leading));
        let whole = element("AXButton", Some("QA Eject"), None);
        assert_eq!(filter.match_of(&whole), Some(NameMatch::Exact));
        assert!(NameMatch::Exact > NameMatch::Leading);

        for other in [
            "QA Ejected, 6 Slices",
            "Eject, QA Eject",
            "QA Eject 6 Slices",
        ] {
            assert!(
                !filter.matches(&element("AXButton", None, Some(other))),
                "{other}"
            );
        }

        let mut field = element("AXTextField", None, None);
        field.value = Some("QA Eject, typed".into());
        assert!(!filter.matches(&field), "typed text is still not a name");

        let mut identified = element("AXButton", None, None);
        identified.identifier = Some("QA Eject, more".into());
        assert!(!filter.matches(&identified), "identifiers match whole");
    }

    #[test]
    fn quit_output_names_each_signalled_app_once() {
        assert_eq!(signalled(b"12\n 34 \nnot a pid\n"), [12, 34]);

        let process = |pid, name: &str| Process {
            pid,
            executable: format!("/{pid}"),
            name: name.to_owned(),
            protected: false,
        };

        let processes = [
            process(1, "Notes"),
            process(2, "Notes"),
            process(3, "Safari"),
        ];
        assert_eq!(names_by_quit(&processes, &[1, 2], true), ["Notes"]);
        assert_eq!(names_by_quit(&processes, &[1, 2], false), ["Safari"]);
    }

    #[test]
    fn open_menus_are_an_apps_own_windows() {
        let name = Target::Name("Opie".into());
        let mut menu = window(9, 6, "Opie", MENU_LAYER);
        assert_eq!(
            app_pid(std::slice::from_ref(&menu), &name).unwrap(),
            6,
            "a menu bar app with only its menu open"
        );

        let frame = Frame {
            x: 10.0,
            y: 20.0,
            width: 300.0,
            height: 200.0,
        };
        assert_eq!(
            owning_window(std::slice::from_ref(&menu), 6, &frame),
            Some(9)
        );

        menu.alpha = 0.0;
        assert!(
            app_pid(&[menu], &name).is_err(),
            "a closed menu is no window"
        );

        // Two processes of one name: the one with a panel wins over one
        // showing only a menu, and a same-sized ordinary window over a menu.
        let panel = window(4, 7, "Opie", 3);
        assert_eq!(
            app_pid(&[window(9, 6, "Opie", MENU_LAYER), panel], &name).unwrap(),
            7
        );

        let ordinary = window(5, 6, "Opie", 0);
        assert_eq!(
            owning_window(&[window(9, 6, "Opie", MENU_LAYER), ordinary], 6, &frame),
            Some(5)
        );

        let menubar = window(1, 2, "Window Server", 24);
        assert!(
            !app_or_menu_layer(&menubar),
            "the menu bar is not the app's"
        );

        let open = window(9, 6, "Opie", MENU_LAYER);
        let filter = WindowFilter {
            app: Some("opie".into()),
            ..WindowFilter::default()
        };

        assert!(
            !filter.matches(&open),
            "wait and windows never take an open menu for a window"
        );
    }

    /// An element as the scan reports it, with a frame.
    fn shown(index: usize, role: &str, title: Option<&str>, description: Option<&str>) -> Element {
        let mut element = element(role, title, description);
        element.index = index;
        element.frame = Some(Frame {
            x: 10.0,
            y: 10.0,
            width: 20.0,
            height: 20.0,
        });

        element
    }

    fn named(name: &str) -> ElementFilter {
        ElementFilter {
            role: None,
            name: Some(name.into()),
        }
    }

    #[test]
    fn a_click_chooses_one_visible_element() {
        let whole = shown(0, "AXButton", Some("Delete"), None);
        let row = shown(1, "AXButton", None, Some("Delete, 3 files"));
        let both = [row.clone(), whole.clone()];
        let chosen = choose(&both, &named("delete")).unwrap();
        assert_eq!(chosen.index, 0, "a whole name wins over a first part");
        assert_eq!(
            choose(std::slice::from_ref(&row), &named("delete"))
                .unwrap()
                .index,
            1,
            "a first part alone is enough"
        );

        // A whole match without a frame does not hand the click to a row
        // named only in part.
        let mut hidden = whole.clone();
        hidden.frame = None;
        let error = choose(&[hidden, row.clone()], &named("delete")).unwrap_err();
        assert!(
            error.error.contains("No visible element"),
            "{}",
            error.error
        );

        let other = shown(2, "AXButton", None, Some("Delete, 9 files"));
        let error = choose(&[row.clone(), other], &named("delete")).unwrap_err();
        assert!(
            error.error.starts_with("2 elements match"),
            "{}",
            error.error
        );
        assert_eq!(error.details["candidates"].as_array().unwrap().len(), 2);

        let mut unread = shown(3, "AXButton", None, None);
        unread.unreadable = vec!["AXTitle".into()];
        let error = choose(&[whole.clone(), unread.clone()], &named("delete")).unwrap_err();
        assert!(error.error.contains("could not be read"), "{}", error.error);
        assert!(error.details.is_empty(), "a doubtful match lists none");
    }

    /// A scan as the probe reports it, complete.
    fn scanned(elements: Vec<Element>) -> Scanned {
        Scanned {
            elements,
            incomplete: false,
            reasons: Vec::new(),
            errors: BTreeMap::new(),
        }
    }

    fn frame(x: f64, y: f64, width: f64, height: f64) -> Frame {
        Frame {
            x,
            y,
            width,
            height,
        }
    }

    #[test]
    fn the_default_listing_names_each_element_by_its_click_point_and_size() {
        let window = frame(10.2, 38.0, 800.0, 600.0);
        let mut bold = shown(3, "AXCheckBox", Some("Bold"), None);
        bold.frame = Some(frame(388.0, 51.0, 24.0, 22.4));
        bold.window = Some(window.clone());
        bold.value = Some("0".into());
        bold.depth = 4;
        bold.identifier = Some("bold".into());
        bold.description = Some("Bold text".into());

        let mut text = shown(17, "AXTextArea", None, None);
        text.frame = Some(frame(20.0, 50.0, 780.0, 560.0));
        text.window = Some(frame(10.0, 38.5, 800.0, 600.0));
        text.value = Some("Hello".into());

        let mut label = shown(18, "AXStaticText", None, None);
        label.value = Some("Status".into());
        label.window = Some(frame(900.0, 0.0, 300.0, 200.0));

        let mut bare = element("AXGroup", None, None);
        bare.index = 19;

        let listed = listing(
            812,
            &scanned(vec![bold, text, label, bare]),
            &ElementFilter::default(),
            50,
            false,
        );
        assert_eq!(
            listed,
            json!({
                "ok": true, "pid": 812, "scanned": 4, "scanIncomplete": false,
                "matching": 4, "more": 0,
                "elements": [
                    {"index": 3, "role": "AXCheckBox", "name": "Bold", "value": "0",
                     "at": [400, 62], "size": [24, 22], "window": 0},
                    {"index": 17, "role": "AXTextArea", "value": "Hello",
                     "at": [410, 330], "size": [780, 560], "window": 0},
                    {"index": 18, "role": "AXStaticText", "name": "Status",
                     "at": [20, 20], "size": [20, 20], "window": 1},
                    {"index": 19, "role": "AXGroup"},
                ],
                "windows": [
                    {"x": 10, "y": 38, "width": 800, "height": 600},
                    {"x": 900, "y": 0, "width": 300, "height": 200},
                ],
            })
        );
    }

    #[test]
    fn flags_are_listed_only_when_they_are_set() {
        let mut off = shown(0, "AXButton", Some("Save"), None);
        off.enabled = Some(false);
        off.focused = Some(false);
        off.selected = Some(false);
        off.value = Some(String::new());

        let mut on = shown(1, "AXMenuItem", Some("Ruler"), None);
        on.enabled = Some(true);
        on.focused = Some(true);
        on.selected = Some(true);
        on.menu = true;
        on.mark = Some("✓".into());
        on.truncated = vec!["AXTitle".into()];
        on.unreadable = vec!["AXDescription".into()];

        let listed = listing(
            1,
            &scanned(vec![off, on]),
            &ElementFilter::default(),
            50,
            false,
        );
        let elements = listed["elements"].as_array().unwrap();
        assert_eq!(elements[0]["disabled"], true);
        for absent in [
            "focused",
            "selected",
            "menu",
            "mark",
            "value",
            "truncated",
            "unreadable",
        ] {
            assert!(elements[0].get(absent).is_none(), "{absent}");
        }

        assert!(elements[1].get("disabled").is_none());
        assert_eq!(elements[1]["focused"], true);
        assert_eq!(elements[1]["selected"], true);
        assert_eq!(elements[1]["menu"], true);
        assert_eq!(elements[1]["mark"], "✓");
        assert_eq!(elements[1]["truncated"], json!(["AXTitle"]));
        assert_eq!(elements[1]["unreadable"], json!(["AXDescription"]));
        for dropped in [
            "depth",
            "subrole",
            "title",
            "description",
            "identifier",
            "frame",
        ] {
            assert!(elements[1].get(dropped).is_none(), "{dropped}");
        }
    }

    #[test]
    fn elements_listed_in_full_are_as_the_scan_read_them() {
        let mut save = shown(0, "AXButton", Some("Save"), None);
        save.window = Some(frame(0.0, 0.0, 600.0, 400.0));
        save.identifier = Some("save".into());
        let scan = scanned(vec![save.clone(), shown(1, "AXButton", Some("Open"), None)]);

        let listed = listing(1, &scan, &named("save"), 50, true);
        assert_eq!(listed["elements"], json!([save]));
        assert!(listed.get("windows").is_none());
        assert_eq!(listed["matching"], 1);
        assert_eq!(listed["more"], 0);
    }

    #[test]
    fn a_short_listing_says_how_many_more_matched() {
        let five: Vec<Element> = (0..5)
            .map(|index| shown(index, "AXButton", Some("Go"), None))
            .collect();
        let scan = scanned(five);
        let all = ElementFilter::default();

        for (max, shown, more) in [(2, 2, 3), (0, 0, 5), (50, 5, 0)] {
            for full in [false, true] {
                let listed = listing(1, &scan, &all, max, full);
                assert_eq!(listed["elements"].as_array().unwrap().len(), shown);
                assert_eq!(listed["matching"], 5);
                assert_eq!(listed["more"], more, "--max {max}");
                assert_eq!(listed["scanIncomplete"], false);
            }
        }
    }

    #[test]
    fn a_scan_says_why_it_stopped_early() {
        let probe = |reasons: Value, errors: Value| {
            let mut reply =
                json!({"ok": true, "elements": [], "truncated": true, "errors": errors});
            if !reasons.is_null() {
                reply["reasons"] = reasons;
            }
            Scanned::from_probe(reply).unwrap()
        };

        let every = probe(
            json!(["depth", "count", "time", "unread", "later"]),
            json!({}),
        );
        assert_eq!(
            every.reasons,
            [Stop::Depth, Stop::Count, Stop::Time, Stop::Unread]
        );
        assert!(every.incomplete);
        assert_eq!(
            every.diagnostics(),
            Some(json!({"reasons": ["depth", "count", "time", "unread"]}))
        );

        let older = probe(Value::Null, json!({}));
        assert!(older.reasons.is_empty() && older.incomplete);
        assert_eq!(older.diagnostics(), None, "an older probe gives none");

        let mut errors = serde_json::Map::new();
        errors.insert("AXTitle:-25212".into(), json!(40));
        errors.insert("AXValue:-25205".into(), json!(40));
        errors.insert(format!("{}:-25200", "A".repeat(70)), json!(50));
        for attribute in 0..10 {
            errors.insert(
                format!("AXChildren{attribute}:-25204"),
                json!(attribute + 1),
            );
        }
        let failed = probe(json!(["unread"]), Value::Object(errors));
        let kept: Vec<(&str, usize)> = failed
            .errors
            .iter()
            .map(|(key, count)| (key.as_str(), *count))
            .collect();
        assert_eq!(kept.len(), 8, "at most eight");
        assert!(
            kept.iter().all(|(key, _)| key.starts_with("AXChildren")),
            "{kept:?}"
        );
        assert_eq!(
            kept.iter().map(|(_, count)| count).min(),
            Some(&3),
            "the most often"
        );
        assert_eq!(
            listing(1, &failed, &ElementFilter::default(), 50, false)["scan"]["reasons"],
            json!(["unread"])
        );

        let quiet = listing(
            1,
            &scanned(Vec::new()),
            &ElementFilter::default(),
            50,
            false,
        );
        assert!(quiet.get("scan").is_none());

        let mut complete = failed;
        complete.incomplete = false;
        let listed = listing(1, &complete, &ElementFilter::default(), 50, false);
        assert!(
            listed.get("scan").is_none(),
            "a complete scan has nothing to say"
        );
    }

    #[test]
    fn a_refused_click_on_an_incomplete_scan_points_to_its_diagnostics() {
        let mut scan = scanned(Vec::new());
        scan.incomplete = true;
        scan.reasons = vec![Stop::Time, Stop::Unread];
        scan.errors = BTreeMap::from([("AXChildren:-25204".to_owned(), 2)]);

        let error = scan.refusal();
        assert_eq!(error.code, "elements");
        assert_eq!(
            error.error,
            "The accessibility scan was incomplete (it ran out of time and part of the app could \
             not be read), so a unique match cannot be established"
        );
        assert_eq!(
            error.details["scan"],
            json!({"reasons": ["time", "unread"], "errors": {"AXChildren:-25204": 2}})
        );
        assert!(
            error.details["next"]
                .as_str()
                .unwrap()
                .starts_with("Try again.")
        );

        scan.reasons.clear();
        scan.errors.clear();
        let error = scan.refusal();
        assert_eq!(
            error.error,
            "The accessibility scan was incomplete, so a unique match cannot be established"
        );
        assert!(error.details.get("scan").is_none());
    }

    /// The names a refused click lists, in order.
    fn candidate_names(error: &Error) -> Vec<&str> {
        error.details["candidates"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|candidate| candidate["name"].as_str())
            .collect()
    }

    #[test]
    fn a_name_that_matches_nothing_lists_the_closest_names() {
        let at = |index, x: f64, role: &str, title: &str| {
            let mut element = shown(index, role, Some(title), None);
            element.frame = Some(frame(x, 0.0, 40.0, 22.0));
            element
        };
        let bar = [
            at(0, 10.0, "AXMenuBarItem", "Edit"),
            at(1, 50.0, "AXMenuBarItem", "Format"),
            at(2, 90.0, "AXMenuBarItem", "Window"),
            at(3, 130.0, "AXMenuBarItem", "Zoom"),
        ];
        let error = choose(&bar, &named("Formt")).unwrap_err();
        assert_eq!(
            candidate_names(&error),
            ["Format"],
            "unrelated names are left out"
        );
        assert!(
            error
                .error
                .ends_with(" The closest names are in candidates.")
        );
        assert_eq!(
            error.details["candidates"][0],
            json!({"role": "AXMenuBarItem", "name": "Format", "at": [70, 11]})
        );

        let error = choose(&bar, &named("Nothing like it")).unwrap_err();
        assert!(error.details.get("candidates").is_none());
        assert!(error.error.ends_with("what is there."), "{}", error.error);

        let close = [
            at(0, 10.0, "AXButton", "Sane"),
            at(1, 50.0, "AXButton", "Save As…"),
            at(2, 90.0, "AXButton", "Sav"),
            at(3, 130.0, "AXButton", "Cave"),
            at(4, 170.0, "AXButton", "Sage"),
            at(5, 210.0, "AXButton", "Wave"),
            at(6, 250.0, "AXButton", "Have"),
        ];
        let error = choose(&close, &named("save")).unwrap_err();
        let names = candidate_names(&error);
        assert_eq!(names.len(), 5, "at most five: {names:?}");
        assert_eq!(
            names[..2],
            ["Sav", "Save As…"],
            "a part, nearest first, before a few letters off"
        );

        let error = choose(&[at(0, 10.0, "AXButton", "Sofe")], &named("save")).unwrap_err();
        assert!(
            error.details.get("candidates").is_none(),
            "two letters off is too far for a short name"
        );

        let dialog = [
            at(0, 10.0, "AXButton", "No"),
            at(1, 50.0, "AXButton", "Go"),
            at(2, 90.0, "AXButton", "Okay"),
        ];
        let error = choose(&dialog, &named("OK")).unwrap_err();
        assert_eq!(
            candidate_names(&error),
            ["Okay"],
            "a short name is not matched a letter or two off"
        );

        let mut hidden = at(7, 0.0, "AXButton", "Safe");
        hidden.frame = None;
        let error = choose(&[hidden], &named("save")).unwrap_err();
        assert!(
            error.details.get("candidates").is_none(),
            "unframed is skipped"
        );

        let window = at(0, 10.0, "AXButton", "Copied");
        let mut item = at(1, 50.0, "AXMenuItem", "Copy Style");
        item.menu = true;
        let error = choose(&[window, item], &named("Copy")).unwrap_err();
        assert_eq!(
            candidate_names(&error),
            ["Copy Style"],
            "only the open menu"
        );

        let by_role = ElementFilter {
            role: Some("AXButton".into()),
            name: Some("format".into()),
        };
        let mut button = at(4, 170.0, "AXButton", "Formats");
        button.index = 0;
        let error = choose(&[button, bar[1].clone()], &by_role).unwrap_err();
        assert_eq!(
            candidate_names(&error),
            ["Format", "Formats"],
            "the exact name under another role first"
        );

        let listed = listing(1, &scanned(bar.to_vec()), &named("Formt"), 50, false);
        assert_eq!(listed["matching"], 0);
        assert_eq!(listed["candidates"][0]["name"], "Format");
        let listed = listing(1, &scanned(bar.to_vec()), &named("Format"), 50, false);
        assert!(listed.get("candidates").is_none());
    }

    #[test]
    fn a_checkbox_or_radio_button_is_clicked_on_its_box_not_the_middle_of_its_frame() {
        let wide = frame(100.0, 200.0, 237.0, 18.0);
        assert_eq!(click_point(Some("AXCheckBox"), &wide), (109.0, 209.0));
        assert_eq!(click_point(Some("AXRadioButton"), &wide), (109.0, 209.0));

        let tab = frame(100.0, 200.0, 80.0, 50.0);
        assert_eq!(click_point(Some("AXRadioButton"), &tab), (140.0, 225.0));

        let bare = frame(100.0, 200.0, 14.0, 30.0);
        assert_eq!(click_point(Some("AXCheckBox"), &bare), (107.0, 215.0));
    }

    #[test]
    fn an_ordinary_button_is_clicked_in_the_middle_of_its_frame() {
        let wide = frame(100.0, 200.0, 237.0, 18.0);
        assert_eq!(click_point(Some("AXButton"), &wide), (219.0, 209.0));
        assert_eq!(click_point(None, &wide), (219.0, 209.0));
    }

    #[test]
    fn a_click_point_always_lies_inside_its_elements_frame() {
        let frames = [
            frame(100.0, 200.0, 237.0, 16.0),
            frame(485.0, 344.0, 115.0, 24.0),
            frame(100.0, 200.0, 14.0, 30.0),
            frame(10.4, 20.6, 41.0, 20.0),
            frame(-50.6, -20.4, 300.0, 1.0),
            frame(0.0, 0.0, 1.0, 1.0),
        ];

        for role in [
            Some("AXCheckBox"),
            Some("AXRadioButton"),
            Some("AXButton"),
            None,
        ] {
            for frame in &frames {
                let (x, y) = click_point(role, frame);
                assert!(
                    (frame.x..=frame.x + frame.width).contains(&x)
                        && (frame.y..=frame.y + frame.height).contains(&y),
                    "{role:?} {frame:?} -> {x}, {y}"
                );
            }
        }
    }

    #[test]
    fn listed_and_candidate_points_aim_at_a_checkbox_box() {
        let detectors = |index| {
            let mut element = shown(index, "AXCheckBox", Some("Data detectors"), None);
            element.frame = Some(frame(100.0, 200.0 + 30.0 * index as f64, 237.0, 18.0));
            element
        };

        let listed = listing(
            1,
            &scanned(vec![detectors(0)]),
            &named("Data detectors"),
            50,
            false,
        );
        assert_eq!(listed["elements"][0]["at"], json!([109, 209]));

        let error = choose(&[detectors(0), detectors(1)], &named("Data detectors")).unwrap_err();
        assert_eq!(
            error.details["candidates"],
            json!([
                {"role": "AXCheckBox", "name": "Data detectors", "at": [109, 209]},
                {"role": "AXCheckBox", "name": "Data detectors", "at": [109, 239]},
            ])
        );
    }

    #[test]
    fn a_name_that_matches_several_lists_each_match() {
        let close = |index| {
            let mut element = shown(index, "AXButton", None, Some("close button"));
            element.frame = Some(frame(10.0 + 100.0 * index as f64, 0.0, 14.0, 14.0));
            element
        };

        let error = choose(&[close(0), close(1)], &named("close button")).unwrap_err();
        assert!(
            error.error.starts_with("2 elements match"),
            "{}",
            error.error
        );
        assert_eq!(
            error.details["candidates"],
            json!([
                {"role": "AXButton", "name": "close button", "at": [17, 7]},
                {"role": "AXButton", "name": "close button", "at": [117, 7]},
            ])
        );

        let seven: Vec<Element> = (0..7).map(close).collect();
        let error = choose(&seven, &named("close button")).unwrap_err();
        assert_eq!(error.details["candidates"].as_array().unwrap().len(), 5);
        assert_eq!(error.details["candidates"][4]["at"], json!([417, 7]));
    }

    /// Names fold case beyond ASCII; an attribute shown only in part is never
    /// an exact match but may hide one; a collapsed element is not on
    /// screen; a menu bar title gives way to a window element of its name.
    #[test]
    fn matching_is_honest_about_what_it_cannot_see() {
        let editer = shown(0, "AXButton", Some("Éditer"), None);
        assert_eq!(
            choose(std::slice::from_ref(&editer), &named("ÉDITER"))
                .unwrap()
                .index,
            0
        );

        let mut cut = shown(1, "AXButton", Some("Delete"), None);
        cut.truncated = vec!["AXTitle".into()];
        assert!(!named("delete").matches(&cut), "a cut title proves nothing");
        assert!(named("delete").might_match(&cut), "but it may hide a match");

        let whole = shown(2, "AXButton", Some("Delete"), None);
        let error = choose(&[whole.clone(), cut], &named("delete")).unwrap_err();
        assert!(error.error.contains("could not be read"), "{}", error.error);

        let mut collapsed = shown(3, "AXButton", Some("Delete"), None);
        collapsed.frame.as_mut().unwrap().height = 0.0;
        assert_eq!(
            choose(&[collapsed, whole.clone()], &named("delete"))
                .unwrap()
                .index,
            2,
            "a collapsed element is not on screen"
        );

        // A long message cut short cannot hide a short name; what is shown
        // of a cut name can.
        let mut message = shown(6, "AXStaticText", None, None);
        message.value = Some("This document explains what the button does".into());
        message.truncated = vec!["AXValue".into()];
        let ok = shown(7, "AXButton", Some("OK"), None);
        assert_eq!(
            choose(&[message, ok.clone()], &named("ok")).unwrap().index,
            7
        );

        let mut long = shown(8, "AXButton", Some("Delete every"), None);
        long.truncated = vec!["AXTitle".into()];
        assert!(named("delete everything").might_match(&long));
        assert!(!named("delete").might_match(&long));

        // The whole first part of a cut label is still judged.
        let mut parts = shown(
            9,
            "AXMenuItem",
            Some("QA Eject, Option-Space, 6 Slices"),
            None,
        );
        parts.truncated = vec!["AXTitle".into()];
        assert!(matches!(
            named("qa eject").match_of(&parts),
            Some(NameMatch::Leading)
        ));
        assert!(!named("eject").might_match(&parts));

        // A static text whose value failed to read may be the label, unless
        // it is collapsed.
        let mut unread = shown(10, "AXStaticText", None, None);
        unread.unreadable = vec!["AXValue".into()];
        assert!(named("ok").might_match(&unread));
        assert!(choose(&[unread.clone(), ok.clone()], &named("ok")).is_err());

        unread.frame.as_mut().unwrap().width = 0.0;
        assert_eq!(choose(&[unread, ok], &named("ok")).unwrap().index, 7);

        let help = shown(4, "AXButton", Some("Help"), None);
        let title = shown(5, "AXMenuBarItem", Some("Help"), None);
        assert_eq!(
            choose(&[title.clone(), help], &named("help"))
                .unwrap()
                .index,
            4,
            "the window's Help button, not the menu"
        );

        let by_role = ElementFilter {
            role: Some("AXMenuBarItem".into()),
            name: Some("help".into()),
        };
        assert_eq!(choose(&[title], &by_role).unwrap().index, 5);
    }

    #[test]
    fn with_a_menu_open_only_its_items_are_chosen() {
        let button = shown(0, "AXButton", Some("Copy"), None);
        let mut unread = shown(1, "AXTextArea", None, None);
        unread.unreadable = vec!["AXDescription".into()];
        let mut item = shown(2, "AXMenuItem", Some("Copy"), None);
        item.menu = true;

        let elements = [button.clone(), unread.clone(), item.clone()];
        assert_eq!(
            choose(&elements, &named("copy")).unwrap().index,
            2,
            "the window's button and its unreadable neighbour are out of play"
        );

        let error = choose(&[button, unread, item], &named("paste")).unwrap_err();
        assert!(error.error.contains("open menu"), "{}", error.error);
    }

    #[test]
    fn an_apps_visible_floating_windows_are_listed_and_hidden_ones_are_not() {
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
        assert!(
            WindowFilter::default().matches(&panel),
            "windows shows it too"
        );
    }

    #[test]
    fn a_launch_refused_for_a_finished_job_names_its_app() {
        let said = "The application /System/Applications/Calculator.app cannot be opened for an \
                    unexpected reason, error=Error Domain=RBSRequestErrorDomain Code=5 \"Launch \
                    failed.\" UserInfo={NSUnderlyingError=0x7482c10990 {Error \
                    Domain=OSLaunchdErrorDomain Code=37 \"Operation already in progress\"}}";
        assert_eq!(
            stale_launch(said),
            Some("/System/Applications/Calculator.app")
        );

        let other = said.replace("Code=37", "Code=3");
        assert_eq!(stale_launch(&other), None, "only error 37");
        assert_eq!(
            stale_launch("Unable to find application named 'Nope'"),
            None
        );
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

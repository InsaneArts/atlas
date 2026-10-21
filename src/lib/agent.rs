//! Agent-facing commands: small, stable verbs over the guarded input path,
//! the helper's window list, accessibility elements, app launching and
//! waiting for windows. Every action is an ordinary validated batch with the
//! usual ownership and epoch checks; nothing here bypasses them.
use crate::{
    actions::Button,
    client::Client,
    control::Ownership,
    error::{Error, Result},
    handoff,
    native::{
        ax::{Element, Frame},
        windows::{Bounds, MENU_BAR_LAYER, MENU_LAYER, Window, unobstructed},
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
a web app in Google Chrome or Firefox, take screenshots, and click or type in apps. Not for unit tests, builds \
or reading files.";

/// The rules every agent follows, set between the guide's introduction and
/// its first section in the plugin's skill.
const SKILL_RULES: &str = "## Rules

- Never act on the person's own desktop; everything happens in the helper.
- Check your on-screen work in the helper before saying it works, and show the
  screenshots.
- Do the work yourself: opening builds, copying files in, cleaning up. The
  person only set Atlas up.
- Never use sudo, type a password or change privacy settings. If `doctor` is
  not ready, run the Atlas commands it suggests; tell the person only what
  needs them (or use the setup skill).
- Never take control from the person: on `busy`, wait.
- Ask the person before `atlas switch`, and before anything that locks, sleeps
  or logs out. Run `atlas logout` only when the person asks for it: only they
  can log the helper back in.
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

/// How much text `atlas type` sends at once: 64 characters, about half a
/// second of typing, so a busy Mac typing several times slower than usual
/// still finishes each part well within the helper's deadline. Control is
/// held across the parts, so nothing lands between them.
const TYPE_BATCH: usize = 64;

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

/// Text as `type` batches of at most [`TYPE_BATCH`] characters, never
/// splitting a grapheme cluster (an emoji sequence or a letter with
/// combining marks); a longer cluster goes on its own, up to
/// [`TEXT_MAX`](crate::actions::TEXT_MAX).
pub(crate) fn type_batches(text: &str) -> Result<Vec<String>> {
    let mut batches = Vec::new();
    let mut chunk = String::new();
    let mut length = 0;

    for grapheme in text.graphemes(true) {
        let size = grapheme.chars().count();
        if size > crate::actions::TEXT_MAX {
            return Err(Error::new(
                "input",
                "A single character cluster is too long to type",
            ));
        }

        if length > 0 && length + size > TYPE_BATCH {
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
/// failure, whether the failed batch's delivery is uncertain. The whole
/// text is typed under one ownership into the helper as it was at the
/// start: no other input lands between batches, and a helper that restarts
/// midway is refused rather than typed into.
pub async fn type_text(client: &Client, text: &str) -> Result<Value> {
    let batches = type_batches(text)?;
    let (mut typed, mut confirmed) = (0, 0);

    let ownership = client.controller.acquire(false)?;
    let epoch = client.identity().await?.epoch;

    for batch in &batches {
        let outcome = client
            .input_owned(
                &ownership,
                &json!([{ "type": "type", "text": batch }]),
                &epoch,
            )
            .await;

        // Agents act on the code (busy, lease-lost, session-changed), so
        // it is kept whichever way the batch failed.
        let (code, failure, uncertain) = match outcome {
            Ok(reply) if reply["ok"] == true => {
                confirmed += 1;
                typed += batch.chars().count();
                continue;
            }
            Ok(reply) => (
                reply["code"]
                    .as_str()
                    .and_then(crate::error::code)
                    .unwrap_or("input"),
                reply["error"]
                    .as_str()
                    .unwrap_or("The helper did not confirm the text")
                    .to_owned(),
                reply["uncertain"] == true,
            ),
            Err(error) => (error.code, error.error, error.uncertain),
        };

        return Ok(json!({
            "ok": false,
            "typed": typed,
            "batches": batches.len(),
            "confirmedBatches": confirmed,
            "code": code,
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

/// Whether two app names name the same app: compared case-insensitively,
/// so `textedit` and `TextEdit` (or `ÉDITEUR` and `Éditeur`) agree.
pub(crate) fn same_text(a: &str, b: &str) -> bool {
    a == b
        || a.chars()
            .flat_map(char::to_lowercase)
            .eq(b.chars().flat_map(char::to_lowercase))
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
    windows_within(client, PROBE_LIMIT).await
}

/// The window list, with the helper probe bounded by `limit`.
async fn windows_within(client: &Client, limit: Duration) -> Result<Vec<Window>> {
    let probe = client.config.identity_probe.to_string_lossy().into_owned();
    let reply = client
        .inspect_within(vec![probe, "windows-info".into()], limit)
        .await?;

    crate::native::windows::from_probe(&reply)
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

    let mut reply = ran(client.run_owned(&ownership, argv.clone()).await?)?;

    if reply["exitCode"] != 0 && cleared_stale_launch(client, &ownership, &reply).await? {
        reply = ran(client.run_owned(&ownership, argv).await?)?;
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

/// A helper `run` reply whose command ran to its end. A command the helper
/// could not start, or stopped part way, is said with the helper's own code
/// and certainty: one stopped part way may have done some of its work.
fn ran(reply: Value) -> Result<Value> {
    if reply["ok"] != true {
        return Err(crate::transport::refusal(&reply));
    }

    Ok(reply)
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
    let cleared = ran(client.run_owned(ownership, argv).await?)?;

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

/// Whether a running app (as `apps` lists it) belongs to macOS or to Atlas
/// itself; neither is ever closed. Atlas is known by its bundle identifier
/// too, so a development build never closes the installed app.
fn protected(running: &Value, atlas: Option<&str>) -> bool {
    let executable = running["executable"].as_str().unwrap_or("");
    (["/System/Library/", "/usr/", "/Library/Apple/"]
        .iter()
        .any(|system| executable.starts_with(system))
        && !executable.starts_with("/usr/local/"))
        || atlas.is_some_and(|bundle| executable.starts_with(bundle))
        || running["bundleId"].as_str().is_some_and(|id| {
            id == crate::setup::IDENTIFIER
                || id
                    .strip_prefix(crate::setup::IDENTIFIER)
                    .is_some_and(|rest| rest.starts_with('.'))
        })
}

/// Whether `executable` is the main program of an app someone can open:
/// `/Applications/Notes.app/Contents/MacOS/Notes`, or an app shipped inside
/// another (Simulator in Xcode), but not a helper app, framework program or
/// XPC service inside an app.
fn app_program(executable: &str) -> bool {
    let Some((bundle, program)) = executable.rsplit_once(".app/Contents/MacOS/") else {
        return false;
    };

    executable.starts_with('/')
        && !program.is_empty()
        && !program.contains('/')
        && ![
            "/Contents/Frameworks/",
            "/Contents/XPCServices/",
            "/Contents/Library/",
            "/Contents/Helpers/",
            "/Contents/PlugIns/",
            ".xpc/",
        ]
        .iter()
        .any(|inside| bundle.contains(inside))
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

/// Atlas's own app bundle, whose processes `close` leaves alone.
fn atlas_bundle() -> Option<String> {
    std::env::current_exe()
        .ok()
        .and_then(|exe| crate::update::app_of(&exe))
        .map(|bundle| bundle.to_string_lossy().into_owned())
}

/// Which running apps `close` quits.
#[derive(Debug, Clone, Copy)]
pub enum Closing<'a> {
    /// Every process of the app with this name.
    App(&'a str),
    /// Every app that is not part of macOS or of Atlas: what earlier
    /// sessions left open.
    All,
}

/// A running app's process that `close` selected.
#[derive(Debug, Clone, PartialEq)]
struct Process {
    pid: u64,
    executable: String,
    name: String,
    /// Part of macOS or of Atlas, so never closed.
    protected: bool,
}

/// The running apps `closing` selects.
fn selected(apps: &Value, closing: Closing, atlas: Option<&str>) -> Vec<Process> {
    apps["apps"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|running| match closing {
            Closing::App(app) => running["name"]
                .as_str()
                .is_some_and(|name| same_text(name, app)),
            // An app someone opened: the main program of an app bundle (not
            // a helper or service inside one), with a Dock icon, or a menu
            // bar app that is not part of macOS (its own, such as Siri,
            // start with the session). Background-only processes are
            // services, not apps left open.
            Closing::All => {
                let executable = running["executable"].as_str().unwrap_or("");
                let apple = running["bundleId"]
                    .as_str()
                    .is_some_and(|id| id.starts_with("com.apple."));

                app_program(executable)
                    && !protected(running, atlas)
                    && match running["policy"].as_str() {
                        Some("regular") => true,
                        Some("accessory") => !apple && !executable.starts_with("/System/"),
                        _ => false,
                    }
            }
        })
        .filter_map(|running| {
            Some(Process {
                pid: running["pid"].as_u64()?,
                executable: running["executable"].as_str().unwrap_or("").to_owned(),
                name: running["name"].as_str().unwrap_or("").to_owned(),
                protected: protected(running, atlas),
            })
        })
        .collect()
}

/// The apps someone opened in the helper and left open, by name: what
/// `close --all` would quit.
pub(crate) fn left_open(apps: &Value) -> Vec<String> {
    selected(apps, Closing::All, atlas_bundle().as_deref())
        .into_iter()
        .filter(|process| !process.protected)
        .map(|process| process.name)
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

        let reply = ran(client.run_owned(&ownership, argv).await?)?;
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
    // came back, or another one, may be running from it.
    let running: Vec<&str> = running.iter().map(String::as_str).collect();
    let removed: Vec<String> = names
        .iter()
        .flat_map(|name| handoff::remove_copies_of(name, &running))
        .collect();

    let mut reply = json!({
        "ok": true,
        "closed": !processes.is_empty(),
        "removed": removed,
    });

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

        let matching: Vec<_> = windows_within(client, budget)
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
/// stay ambiguous.
async fn target_pid(client: &Client, target: &Target) -> Result<i64> {
    let name = match target {
        Target::Pid(pid) => {
            let apps = client.apps().await?;
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

    let pids = app_pids(&windows(client).await?, name);
    if !pids.is_empty() {
        return one_pid(name, &pids);
    }

    let apps = client.apps().await?;
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

async fn app_elements(client: &Client, pid: i64) -> Result<(Vec<Element>, bool)> {
    let probe = client.config.identity_probe.to_string_lossy().into_owned();
    let reply = client
        .inspect(vec![probe, "elements-info".into(), pid.to_string()])
        .await?;
    let mut value = run_output(&reply, 8 << 20)
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

    let elements = serde_json::from_value(value["elements"].take())
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
    let pid = target_pid(client, target).await?;
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
        .find(|window| {
            window.layer == MENU_BAR_LAYER
                && window.owner.as_deref() == Some("Window Server")
                && window.alpha > 0.0
                && window.bounds.contains(x, y)
        })
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
    let menu_open = elements.iter().any(|element| element.menu);
    let elements: Vec<&Element> = elements
        .iter()
        .filter(|element| element.menu || !menu_open)
        .collect();

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
        return Err(Error::new(
            "elements",
            match matching.len() {
                0 if menu_open => "No item of the open menu matches. Run atlas elements --app \
                                   NAME to see it, or press escape to close the menu."
                    .to_owned(),
                0 => "No visible element matches. Run atlas elements --app NAME to see \
                      what is there."
                    .to_owned(),
                count => format!(
                    "{count} elements match. Run atlas elements with the same --name to \
                     tell them apart, then add --role, or click the one you mean at its point."
                ),
            },
        ));
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
    let pid = target_pid(client, target).await?;
    let (elements, incomplete) = app_elements(client, pid).await?;
    if incomplete {
        return Err(Error::new(
            "elements",
            "The accessibility scan was incomplete, so a unique match cannot be established",
        ));
    }

    let element = choose(&elements, filter)?;
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
        assert!(same_text("TextEdit", "textedit"));
        assert!(same_text("ÉDITEUR", "éditeur"));
        assert!(!same_text("TextEdit", "Text"));

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

    /// `close --all` quits what sessions leave open, never macOS or Atlas.
    #[test]
    fn closing_everything_spares_macos_and_atlas() {
        let app = |name: &str, policy: &str, executable: &str| json!({"name": name, "pid": 10, "policy": policy, "executable": executable});
        let apps = json!({"apps": [
            app("Finder", "regular", "/System/Library/CoreServices/Finder.app/Contents/MacOS/Finder"),
            app("loginwindow", "accessory", "/System/Library/CoreServices/loginwindow.app/Contents/MacOS/loginwindow"),
            app("Siri AI", "accessory", "/System/Applications/Siri AI.app/Contents/MacOS/Siri AI"),
            app("universalaccessd", "prohibited", "/usr/sbin/universalaccessd"),
            app("Atlas", "accessory", "/Applications/Atlas.app/Contents/MacOS/atlas"),
            app("Helper", "prohibited", "/Applications/Other.app/Contents/MacOS/Helper"),
            app("Calendar", "regular", "/System/Applications/Calendar.app/Contents/MacOS/Calendar"),
            app("Safari", "regular", "/Applications/Safari.app/Contents/MacOS/Safari"),
            app("Opie QA", "accessory", "/Users/Shared/Opie QA.app/Contents/MacOS/Opie QA"),
        ]});

        let names = |closing| {
            selected(&apps, closing, Some("/Applications/Atlas.app"))
                .into_iter()
                .map(|process| process.name)
                .collect::<Vec<_>>()
        };

        assert_eq!(names(Closing::All), ["Calendar", "Safari", "Opie QA"]);
        let open = json!({"apps": [apps["apps"][0], apps["apps"][7], apps["apps"][8]]});
        assert_eq!(
            left_open(&open),
            ["Safari", "Opie QA"],
            "a held log-out names what close --all would quit"
        );

        let services = json!({"apps": [
            app("Raycast Web Content", "accessory", "/Users/me/com.apple.WebKit.WebContent"),
            app("Slack Helper", "accessory", "/Applications/Slack.app/Contents/Frameworks/Slack Helper.app/Contents/MacOS/Slack Helper"),
            app("Updater", "accessory", "/Applications/Opie.app/Contents/Frameworks/Sparkle.framework/Versions/B/Updater.app/Contents/MacOS/Updater"),
            app("Nameless", "regular", ""),
        ]});

        assert!(
            selected(&services, Closing::All, None).is_empty(),
            "helpers, services and apps without a program are not apps left open"
        );

        assert!(app_program("/Applications/Notes.app/Contents/MacOS/Notes"));
        assert!(app_program(
            "/Applications/Xcode.app/Contents/Developer/Applications/Simulator.app/Contents/MacOS/Simulator"
        ));
        assert!(!app_program("Notes.app/Contents/MacOS/Notes"));

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

        assert!(!protected(
            &json!({"executable": "/usr/local/Cellar/emacs/Emacs.app/Contents/MacOS/Emacs"}),
            None
        ));
        assert!(protected(
            &json!({"executable": "/usr/libexec/sharingd"}),
            None
        ));

        assert_eq!(names(Closing::App("Finder")), ["Finder"], "refused later");
        assert_eq!(names(Closing::App("opie qa")), ["Opie QA"], "any case");
        assert!(names(Closing::App("Opie")).is_empty(), "whole names only");
        assert!(selected(&apps, Closing::App("Finder"), None)[0].protected);

        // A development build, outside the app, still spares the installed
        // Atlas by its identifier.
        let installed = json!({"executable": "/Applications/Atlas.app/Contents/MacOS/atlas",
            "bundleId": "com.insanearts.atlas"});
        assert!(protected(&installed, None));
        let window = json!({"executable": "/Applications/Atlas.app/Contents/Helpers/Atlas.app/x",
            "bundleId": "com.insanearts.atlas.window"});
        assert!(protected(&window, None));
        let other = json!({"executable": "/Applications/Other.app/x",
            "bundleId": "com.insanearts.atlasish"});
        assert!(!protected(&other, None));
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

        let mut unread = shown(3, "AXButton", None, None);
        unread.unreadable = vec!["AXTitle".into()];
        let error = choose(&[whole.clone(), unread.clone()], &named("delete")).unwrap_err();
        assert!(error.error.contains("could not be read"), "{}", error.error);
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

    #[test]
    fn a_command_the_helper_stopped_part_way_stays_uncertain_with_its_code() {
        for code in ["on-console", "restart", "transport"] {
            let reply = json!({
                "ok": false,
                "code": code,
                "error": "stopped",
                "uncertain": true,
            });

            let error = ran(reply).unwrap_err();
            assert_eq!(error.code, code);
            assert!(error.uncertain, "{code}");
        }
    }

    #[test]
    fn a_command_the_helper_never_started_is_a_certain_refusal() {
        let reply = json!({"ok": false, "code": "spawn-failed", "error": "no such file"});

        let error = ran(reply).unwrap_err();
        assert_eq!(error.code, "spawn-failed");
        assert!(!error.uncertain);
    }

    #[test]
    fn a_command_that_ran_to_its_end_is_read_for_its_exit() {
        let reply = json!({"ok": true, "exitCode": 1, "timedOut": false});

        assert_eq!(ran(reply.clone()).unwrap(), reply);
    }
}

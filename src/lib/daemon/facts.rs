//! The `hello` reply: who the daemon runs as, in which session, on which
//! display, with which permissions.
use crate::native::{
    accessibility_trusted as accessibility, capture,
    cf::{self, CFBoolean, CFDictionary, CFString},
};
use objc2_application_services::AXIsProcessTrustedWithOptions;
use objc2_core_graphics::{
    CGDisplayBounds, CGDisplayCopyDisplayMode, CGDisplayMode, CGMainDisplayID,
};
use serde_json::{Value, json};
use std::time::Duration;

/// Ask for Accessibility with the system prompt, unless it is allowed.
#[allow(
    unsafe_code,
    reason = "FFI: the consent prompt takes an options dictionary"
)]
fn request_accessibility() {
    if accessibility() {
        return;
    }

    let key = CFString::from_static_str("AXTrustedCheckOptionPrompt");
    let options =
        CFDictionary::<CFString, CFBoolean>::from_slices(&[&key], &[CFBoolean::new(true)]);
    // SAFETY: the options dictionary holds the documented string key with a
    // boolean value.
    unsafe { AXIsProcessTrustedWithOptions(Some(options.as_opaque())) };
}

/// Ask for missing consent whenever the helper comes on screen (or starts
/// while on screen), never in the background, where prompts are lost.
/// macOS shows one prompt at a time and the Screen Recording prompt only
/// once per app, so the requests come one by one: Accessibility first,
/// waiting until it is on (macOS reports that at once), then Screen
/// Recording, with its System Settings page opened so its switch is there
/// even when macOS shows no prompt. While Atlas's setup window is open, it
/// guides the person instead and this only watches. Nothing is granted
/// here; the person decides, in the normal way.
///
/// With nothing to ask right after the person logged in (after a restart
/// or a log-out), they are told they can switch back to `controller`.
pub async fn request_consent_on_arrival(controller: u32, restart: impl Fn()) {
    let shown = || console_state() == Some(true);
    let mut present = false;

    loop {
        let now = shown();
        if now && !present {
            ask(&shown, &restart, controller).await;
        }

        // Leaving while being asked counts as never having arrived.
        present = now && shown();
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

/// A login younger than this has only just happened.
const FRESH_LOGIN: Duration = Duration::from_secs(90);

/// Whether this login has only just happened (asked of `ps`, off the
/// runtime).
async fn just_logged_in() -> bool {
    let uid = nix::unistd::getuid().as_raw();
    tokio::task::spawn_blocking(move || crate::session::login(uid))
        .await
        .ok()
        .flatten()
        .is_some_and(|login| login.age < FRESH_LOGIN)
}

/// What the daemon does on one pass about a permission still off.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Nudge {
    /// Nothing now: keep watching.
    Wait,
    /// Ask macOS for it: its prompt, which also lists Atlas in System
    /// Settings.
    Request,
    /// Open its System Settings page.
    OpenSettings,
}

/// Passes in a row with no setup window that the daemon waits for: it asks
/// on the third, so a window opening at login (by itself, or by the
/// opener) is in time to guide instead.
const QUIET_PASSES: u32 = 3;

/// How asking for one permission stands: passes in a row with no setup
/// window, whether macOS was asked, whether its page was opened, and how
/// many passes went by since the request.
#[derive(Debug, Default)]
struct Asking {
    quiet: u32,
    requested: bool,
    opened: bool,
    since_request: u32,
}

impl Asking {
    /// What to do on this pass, and count it. While Atlas's setup window
    /// guides the person (`guided`), only it does: macOS's prompt would
    /// cover it, and asking lists Atlas switched off, where dragging Atlas
    /// in may no longer turn it on. Otherwise, after [`QUIET_PASSES`]
    /// without it, ask once, then open the page once `settings_after`
    /// passes went by since asking.
    fn pass(&mut self, guided: bool, settings_after: u32) -> Nudge {
        self.quiet = if guided { 0 } else { self.quiet + 1 };

        let nudge = if self.quiet < QUIET_PASSES {
            Nudge::Wait
        } else if !self.requested {
            self.requested = true;
            Nudge::Request
        } else if !self.opened && self.since_request >= settings_after {
            self.opened = true;
            Nudge::OpenSettings
        } else {
            Nudge::Wait
        };

        if self.requested {
            self.since_request += 1;
        }

        nudge
    }
}

/// Whether Atlas's setup window guides the person through the permissions
/// in this account now: it is open, or the opener waits to open it (asked
/// off the runtime: it runs `ps`).
async fn guided() -> bool {
    tokio::task::spawn_blocking(|| {
        let uid = nix::unistd::getuid().as_raw();
        crate::config::home().is_ok_and(|home| {
            crate::onboarding::window_open(&home)
                || crate::setup::first_login::opener_waiting(&crate::setup::helper::Layout {
                    uid,
                    home,
                    guard: None,
                })
        })
    })
    .await
    .unwrap_or(false)
}

/// How often the daemon looks for a Screen Recording grant it cannot use
/// yet, while Atlas's setup window guides the person: every 2 s.
const RECORDING_WATCH: Duration = Duration::from_secs(2);

/// The least time between two restarts for Screen Recording: 60 s, so a
/// grant that this process and a new one see differently never restarts it
/// over and over.
const RECORDING_RESTART_GAP: Duration = Duration::from_secs(60);

/// While Atlas's setup window guides the person, restart this process as
/// soon as a new one may use Screen Recording and this one may not. Only a
/// new process gets the grant, and the window reads it from this one, so the
/// window sees the switch on at once, whichever permission the person turned
/// on first. It runs apart from the arrival's requests, which a moment off
/// screen ends, and ends once this process may record.
pub async fn restart_for_recording(restart: impl Fn()) {
    watch_recording(
        || tokio::time::sleep(RECORDING_WATCH),
        capture::permitted,
        || async { guided().await && screen_recording_now().await },
        restarted_lately,
        restart,
    )
    .await;
}

async fn watch_recording<Pause, Probe>(
    mut pause: impl FnMut() -> Pause,
    mut permitted: impl FnMut() -> bool,
    mut granted: impl FnMut() -> Probe,
    mut cooling_down: impl FnMut() -> bool,
    restart: impl Fn(),
) where
    Pause: std::future::Future<Output = ()>,
    Probe: std::future::Future<Output = bool>,
{
    loop {
        pause().await;
        if permitted() {
            return;
        }

        // The window first: a new process is started only while it guides.
        if granted().await && !cooling_down() {
            restart();
            return;
        }
    }
}

/// Whether this watcher restarted Atlas less than [`RECORDING_RESTART_GAP`]
/// ago; when not, now is noted as its latest restart. The note outlives the
/// restart, as the asked mark does.
fn restarted_lately() -> bool {
    let Some(home) = std::env::var_os("HOME").map(std::path::PathBuf::from) else {
        return false;
    };

    let path = home
        .join(crate::setup::helper::ROOT)
        .join("recording-restart");
    let age = std::fs::metadata(&path)
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|modified| modified.elapsed().ok());
    let lately = within(age, RECORDING_RESTART_GAP);
    if !lately {
        let _ = std::fs::write(&path, b"restarted\n");
    }

    lately
}

/// Whether something `age` old (none when it never happened) is younger
/// than `gap`.
fn within(age: Option<Duration>, gap: Duration) -> bool {
    age.is_some_and(|age| age < gap)
}

/// One arrival's requests, given up as soon as the helper leaves the screen.
/// When the person has turned on what was asked for, Atlas says so here, so
/// they know to switch back, and restarts this process if it cannot yet use
/// Screen Recording (macOS applies that grant to new processes only). While
/// the setup window is open, it guides the person and this only watches
/// ([`Asking::pass`]); closing it midway brings the requests back.
async fn ask(shown: &impl Fn() -> bool, restart: &impl Fn(), controller: u32) {
    // Let the desktop settle after the switch, and let macOS finish a new
    // account's setup screens: a prompt raised behind them is never seen.
    tokio::time::sleep(Duration::from_secs(2)).await;

    while shown() && setting_up().await {
        tokio::time::sleep(Duration::from_secs(1)).await;
    }

    if !shown() {
        return;
    }

    let recording = || async { capture::permitted() || screen_recording_now().await };

    // Asked on an earlier visit, or before a restart, and everything is on
    // now: say so (and pick up the grant).
    if accessibility() && recording().await {
        if asked(false) {
            done_notice(controller);
        } else if just_logged_in().await {
            back_notice(controller);
        }

        if !capture::permitted() {
            restart();
        }

        return;
    }

    asked(true);

    if !accessibility() {
        // As long as the person stays, however long they take. Should the
        // prompt not appear, its System Settings page opens instead.
        let mut asking = Asking::default();

        while !accessibility() {
            match asking.pass(guided().await, 15) {
                Nudge::Request => request_accessibility(),
                Nudge::OpenSettings => open_settings("Privacy_Accessibility"),
                Nudge::Wait => {}
            }

            tokio::time::sleep(Duration::from_secs(1)).await;
            if !shown() {
                return;
            }
        }

        tokio::time::sleep(Duration::from_secs(1)).await;
    }

    if !recording().await {
        let mut asking = Asking::default();

        // Just read as off: passes ask (once quiet) before reading it again.
        loop {
            if !shown() {
                return;
            }

            let nudge = asking.pass(guided().await, 1);
            match nudge {
                Nudge::Request => _ = capture::request(),
                Nudge::OpenSettings => open_settings("Privacy_ScreenCapture"),
                Nudge::Wait => {}
            }

            let pause = if nudge == Nudge::Request { 1 } else { 2 };
            tokio::time::sleep(Duration::from_secs(pause)).await;
            if screen_recording_now().await {
                break;
            }
        }
    }

    if shown() {
        asked(false);
        done_notice(controller);
        if !capture::permitted() {
            restart();
        }
    }
}

/// Whether Screen Recording is allowed now, as a fresh Atlas process sees it
/// (never waiting more than a few seconds for it).
async fn screen_recording_now() -> bool {
    let Ok(executable) = std::env::current_exe() else {
        return false;
    };

    let run = tokio::process::Command::new(executable)
        .arg("permissions-info")
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .output();
    match tokio::time::timeout(Duration::from_secs(5), run).await {
        Ok(Ok(output)) => serde_json::from_slice::<Value>(&output.stdout)
            .is_ok_and(|reply| reply["screenCapture"] == true),
        _ => false,
    }
}

/// This process's permissions now (`atlas permissions-info`).
pub fn permissions_now() -> Value {
    json!({
        "ok": true,
        "screenCapture": capture::permitted(),
        "accessibility": accessibility(),
    })
}

/// Remember (`true`) that Atlas asked the person for a permission, or forget
/// it (`false`), returning whether it was remembered. It outlives this
/// process, which macOS's Quit & Reopen restarts mid-way.
fn asked(remember: bool) -> bool {
    let Some(home) = std::env::var_os("HOME").map(std::path::PathBuf::from) else {
        return false;
    };

    let path = home.join(crate::setup::helper::ROOT).join("consent-asked");
    let was = path.exists();

    if remember {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        let _ = std::fs::write(&path, b"asked\n");
    } else {
        let _ = std::fs::remove_file(&path);
    }

    was
}

/// Tell the person, in the helper, that it is set up, with a button that
/// switches back to `controller` when that account is known.
fn done_notice(controller: u32) {
    let account = crate::setup::account_with_uid(controller).ok().flatten();
    let title = account.as_ref().map(|account| {
        crate::setup::real_name(&account.name).unwrap_or_else(|| account.name.clone())
    });
    notice(
        "Atlas is set up",
        &done_message(title.as_deref()),
        account.as_ref().map(|account| account.name.as_str()),
    );
}

/// What the set-up notice says: who to switch back to, by name when known.
fn done_message(title: Option<&str>) -> String {
    let back = title.unwrap_or("your own account");
    format!(
        "Screen Recording and Accessibility are on. {}",
        switch_back(back)
    )
}

/// The sentence that sends the person back to `title`'s account.
fn switch_back(title: &str) -> String {
    format!("Switch back to {title} — Atlas keeps running there.")
}

/// Tell the person, just logged in to the helper, that Atlas runs here
/// again, with a button that switches back to `controller`.
fn back_notice(controller: u32) {
    let Some(account) = crate::setup::account_with_uid(controller).ok().flatten() else {
        return;
    };

    let title = crate::setup::real_name(&account.name).unwrap_or_else(|| account.name.clone());
    notice(
        "Atlas is running here again",
        &switch_back(&title),
        Some(&account.name),
    );
}

/// Where the login a notice was last shown in is kept: a daemon restarted
/// to pick up Screen Recording never repeats what its predecessor showed.
fn notice_record(home: &std::path::Path) -> std::path::PathBuf {
    home.join(crate::setup::helper::ROOT).join("notice-shown")
}

/// Whether a notice was shown already in the login with this loginwindow.
fn shown_in(home: &std::path::Path, login: u32) -> bool {
    std::fs::read_to_string(notice_record(home)).is_ok_and(|shown| shown == login.to_string())
}

/// Tell the person something in the helper, at most once a login, with a
/// Switch Back button to `switch_back` when given. The notice runs on its
/// own; its process is collected when it closes.
fn notice(title: &str, message: &str, switch_back: Option<&str>) {
    use std::os::unix::process::CommandExt;

    let Some(home) = std::env::var_os("HOME").map(std::path::PathBuf::from) else {
        return;
    };

    let login = crate::session::login(nix::unistd::getuid().as_raw()).map(|login| login.pid);
    // The onboarding window, when open here, tells the person itself.
    if crate::onboarding::window_open(&home) || login.is_some_and(|pid| shown_in(&home, pid)) {
        return;
    }

    let Ok(app) = crate::menubar::executable() else {
        return;
    };

    let mut arguments = vec!["--notice", title, message];

    if let Some(account) = switch_back {
        arguments.extend(["--switch-back", account]);
    }

    // Its own process group: restarting this daemon (to pick up Screen
    // Recording) must not close the notice the person is reading.
    let shown = std::process::Command::new(app)
        .process_group(0)
        .args(arguments)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
    if let Ok(mut child) = shown {
        if let Some(pid) = login {
            let _ = std::fs::write(notice_record(&home), pid.to_string());
        }

        std::thread::spawn(move || {
            let _ = child.wait();
        });
    }
}

/// Whether macOS's setup screens for a new account are open in this session
/// (asked of `ps`, off the runtime).
async fn setting_up() -> bool {
    let uid = nix::unistd::getuid().as_raw().to_string();
    let listed = tokio::process::Command::new("/bin/ps")
        .args(["-axo", "uid=,comm="])
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .await;

    listed.is_ok_and(|output| {
        String::from_utf8_lossy(&output.stdout).lines().any(|line| {
            let line = line.trim_start();
            line.strip_prefix(&uid).is_some_and(|rest| {
                rest.starts_with(char::is_whitespace)
                    && rest
                        .trim()
                        .ends_with("/Setup Assistant.app/Contents/MacOS/Setup Assistant")
            })
        })
    })
}

/// Typing in the helper arrives exactly as sent: macOS's autocorrect,
/// automatic capitalisation, smart punctuation and predictions are turned
/// off in the helper's own preferences (apps read them when they start).
/// The person's account is never touched.
pub fn type_exactly() {
    for key in [
        "NSAutomaticSpellingCorrectionEnabled",
        "NSAutomaticCapitalizationEnabled",
        "NSAutomaticPeriodSubstitutionEnabled",
        "NSAutomaticDashSubstitutionEnabled",
        "NSAutomaticQuoteSubstitutionEnabled",
        "NSAutomaticTextReplacementEnabled",
        "NSAutomaticTextCompletionEnabled",
        "NSAutomaticInlinePredictionEnabled",
        "WebAutomaticSpellingCorrectionEnabled",
    ] {
        let _ = std::process::Command::new("/usr/bin/defaults")
            .args(["write", "-g", key, "-bool", "false"])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }
}

/// Open a Privacy & Security page of System Settings in this session.
fn open_settings(page: &str) {
    let _ = std::process::Command::new("/usr/bin/open")
        .arg(format!(
            "x-apple.systempreferences:com.apple.preference.security?{page}"
        ))
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
}

/// This process's account: name, home and UID.
fn user() -> Value {
    let uid = nix::unistd::getuid().as_raw();
    match crate::setup::account_with_uid(uid) {
        Ok(Some(account)) => json!({"uid": uid, "name": account.name, "home": account.home}),
        _ => json!({"uid": uid, "name": null, "home": null}),
    }
}

/// Whether this graphical session is on the console now, if known, from
/// the window server's own session record.
fn console_state() -> Option<bool> {
    let record = cf::session()?;
    cf::boolean(&record, "kCGSSessionOnConsoleKey")
        .or_else(|| cf::boolean(&record, "kCGSessionOnConsoleKey"))
}

/// Who owns this graphical session and whether their login has finished,
/// from the window server's own session record.
pub fn session_owner() -> Option<(i64, bool)> {
    let record = cf::session()?;
    let uid = cf::integer(&record, "kCGSSessionUserIDKey")?;
    let done = cf::boolean(&record, "kCGSessionLoginDoneKey")?;
    Some((uid, done))
}

/// Whether the session is on the console. Unknown counts as on the console:
/// nothing acts unless the session is known to be hidden.
pub fn on_console() -> bool {
    console_state() != Some(false)
}

/// The graphical session: whether it is on the console, and launchd's name
/// for the session type (`Aqua` for a logged-in desktop).
fn session(manager: &str) -> Value {
    json!({"managerName": manager, "onConsole": console_state()})
}

/// The main display's bounds in points: x, y, width, height.
pub fn display_bounds() -> (f64, f64, f64, f64) {
    let bounds = CGDisplayBounds(CGMainDisplayID());
    (
        bounds.origin.x,
        bounds.origin.y,
        bounds.size.width,
        bounds.size.height,
    )
}

/// The main display in points, and its backing pixels per point.
fn display() -> Value {
    let display = CGMainDisplayID();
    let bounds = CGDisplayBounds(display);
    let mode = CGDisplayCopyDisplayMode(display);
    let (pixels, points) = (
        CGDisplayMode::pixel_width(mode.as_deref()),
        CGDisplayMode::width(mode.as_deref()),
    );
    let scale = if points > 0 {
        pixels as f64 / points as f64
    } else {
        1.0
    };

    json!({"width": bounds.size.width, "height": bounds.size.height, "scale": scale})
}

/// launchd's session type for this process (`launchctl managername`).
pub fn manager_name() -> String {
    std::process::Command::new("/bin/launchctl")
        .arg("managername")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "Unknown".into())
}

/// The full `hello` reply.
/// The daemon's hello: who and where the helper is, what it may do, and,
/// when it keeps background jobs, how many may still run, are launching,
/// or have records it cannot read (`jobs`).
pub fn hello(manager: &str, jobs: Option<Value>) -> Value {
    use crate::daemon::protocol::{JOBS, READ, SESSION_PIN};

    let mut capabilities = vec![SESSION_PIN, READ];
    if jobs.is_some() {
        capabilities.push(JOBS);
    }
    let mut hello = json!({
        "ok": true,
        "op": "hello",
        "user": user(),
        "permissions": {
            "accessibility": accessibility(),
            "postEvents": objc2_core_graphics::CGPreflightPostEventAccess(),
            "screenCapture": capture::permitted(),
        },
        "session": session(manager),
        "display": display(),
        "daemon": {"protocol": 1, "pid": std::process::id(), "version": env!("CARGO_PKG_VERSION"),
            "capabilities": capabilities},
    });
    if let Some(jobs) = jobs {
        hello["daemon"]["jobs"] = jobs;
    }
    hello
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_recording_grant_is_picked_up_after_the_restart_cooldown_expires() {
        let elapsed = std::cell::Cell::new(Duration::ZERO);
        let restarted = std::cell::Cell::new(false);
        watch_recording(
            || {
                elapsed.set(elapsed.get() + RECORDING_WATCH);
                assert!(elapsed.get() <= RECORDING_RESTART_GAP);
                std::future::ready(())
            },
            || false,
            || std::future::ready(true),
            || within(Some(elapsed.get()), RECORDING_RESTART_GAP),
            || restarted.set(true),
        )
        .await;
        assert!(restarted.get());
        assert_eq!(elapsed.get(), RECORDING_RESTART_GAP);
    }

    /// The prompt option is passed by name; it is the exported constant.
    #[test]
    #[allow(
        unsafe_code,
        reason = "FFI: the framework's constants are extern statics"
    )]
    fn the_prompt_option_is_the_framework_constant() {
        // SAFETY: reading an immutable framework constant in a test.
        let key = unsafe { objc2_application_services::kAXTrustedCheckOptionPrompt };
        assert_eq!(key.to_string(), "AXTrustedCheckOptionPrompt");
    }

    /// `asking`'s nudges over `count` passes, the window guiding as given.
    fn passes(asking: &mut Asking, guided: bool, settings_after: u32, count: usize) -> Vec<Nudge> {
        (0..count)
            .map(|_| asking.pass(guided, settings_after))
            .collect()
    }

    #[test]
    fn a_restart_for_screen_recording_waits_a_minute_after_the_last_one() {
        let gap = RECORDING_RESTART_GAP;
        assert!(!within(None, gap), "never restarted");
        assert!(within(Some(Duration::from_secs(5)), gap), "just restarted");
        assert!(!within(Some(gap + Duration::from_secs(1)), gap));
    }

    #[test]
    fn while_the_setup_window_guides_the_daemon_only_watches() {
        let mut asking = Asking::default();
        assert!(
            passes(&mut asking, true, 15, 30)
                .iter()
                .all(|nudge| *nudge == Nudge::Wait)
        );
        assert_eq!(
            asking.since_request, 0,
            "passes count only once macOS was asked"
        );
    }

    #[test]
    fn without_the_window_the_daemon_waits_asks_once_then_opens_the_page_once() {
        let mut asking = Asking::default();
        let quiet = QUIET_PASSES as usize;
        let mut expected = vec![Nudge::Wait; quiet - 1];
        expected.push(Nudge::Request);
        expected.extend(vec![Nudge::Wait; 14]);
        expected.push(Nudge::OpenSettings);
        expected.push(Nudge::Wait);
        assert_eq!(passes(&mut asking, false, 15, expected.len()), expected);

        let mut recording = Asking::default();
        let nudges = passes(&mut recording, false, 1, quiet + 2);
        assert_eq!(
            nudges[quiet - 1..],
            [Nudge::Request, Nudge::OpenSettings, Nudge::Wait]
        );
    }

    #[test]
    fn a_window_opening_late_at_login_still_guides_before_any_prompt() {
        let mut asking = Asking::default();
        let early = passes(&mut asking, false, 15, QUIET_PASSES as usize - 1);
        assert!(early.iter().all(|nudge| *nudge == Nudge::Wait));
        assert_eq!(
            asking.pass(true, 15),
            Nudge::Wait,
            "the window came in time"
        );
        let again = passes(&mut asking, false, 15, QUIET_PASSES as usize);
        assert_eq!(
            again.last(),
            Some(&Nudge::Request),
            "closing it brings the request back"
        );
    }

    #[test]
    fn closing_the_window_midway_brings_the_page_back_once_its_time_went_by() {
        let mut asking = Asking::default();
        passes(&mut asking, false, 15, QUIET_PASSES as usize);
        assert!(asking.requested);
        assert!(
            passes(&mut asking, true, 15, 20)
                .iter()
                .all(|nudge| *nudge == Nudge::Wait)
        );
        let after = passes(&mut asking, false, 15, QUIET_PASSES as usize);
        assert_eq!(after.last(), Some(&Nudge::OpenSettings));
    }

    #[test]
    fn the_set_up_notice_names_the_account_to_switch_back_to() {
        assert_eq!(
            done_message(Some("Alex")),
            "Screen Recording and Accessibility are on. Switch back to Alex — Atlas keeps running there."
        );
        assert_eq!(
            done_message(None),
            "Screen Recording and Accessibility are on. Switch back to your own account — Atlas keeps \
             running there."
        );
        assert!(
            !switch_back("Alex").contains(';'),
            "one plain sentence, no semicolon"
        );
    }

    #[test]
    fn a_notice_is_remembered_for_its_login_only() {
        let home = tempfile::tempdir().unwrap();
        assert!(!shown_in(home.path(), 1158), "none shown yet");

        std::fs::create_dir_all(home.path().join(crate::setup::helper::ROOT)).unwrap();
        std::fs::write(notice_record(home.path()), "1158").unwrap();
        assert!(shown_in(home.path(), 1158));
        assert!(!shown_in(home.path(), 2001), "a later login shows its own");
    }

    #[test]
    fn hello_describes_this_process_in_its_session() {
        let reply = hello(&manager_name(), None);
        assert_eq!(reply["user"]["uid"], nix::unistd::getuid().as_raw());
        assert!(reply["user"]["name"].is_string());
        assert!(
            reply["user"]["home"]
                .as_str()
                .is_some_and(|home| home.starts_with('/'))
        );
        assert_eq!(reply["daemon"]["pid"], std::process::id());
        assert_eq!(reply["daemon"]["protocol"], 1);
        assert!(crate::session::has_capability(
            &reply["daemon"],
            crate::daemon::protocol::SESSION_PIN
        ));
        assert!(reply["permissions"]["screenCapture"].is_boolean());
        assert!(
            reply["display"]["scale"]
                .as_f64()
                .is_some_and(|scale| scale >= 1.0)
        );
    }
}

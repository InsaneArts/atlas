//! Asking the person for the helper's permissions when they arrive in it,
//! and restarting for a Screen Recording grant only a new process can use.
use super::facts::console_state;
use atlas_native::native::{accessibility_trusted as accessibility, capture};
use objc2_application_services::AXIsProcessTrustedWithOptions;
use objc2_core_foundation::{CFBoolean, CFDictionary, CFString};
use serde_json::Value;
use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::Duration,
};

/// `command` with nothing to read and nowhere to say anything.
fn silent(command: &mut Command) -> &mut Command {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
}

/// Where the note `name` is kept, which outlives this process.
fn note(name: &str) -> Option<PathBuf> {
    let home = PathBuf::from(std::env::var_os("HOME")?);
    Some(home.join(crate::helper_setup::ROOT).join(name))
}

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
pub(super) async fn request_consent_on_arrival(controller: u32, restart: impl Fn()) {
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
    tokio::task::spawn_blocking(move || atlas_core::session::login(uid))
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

/// How far asking for a permission got.
#[derive(Debug, Default)]
enum ConsentRequest {
    #[default]
    Unasked,
    /// macOS was asked, this many passes ago.
    Requested(u32),
    Opened,
}

/// Passes without a setup window, and the permission request's progress.
#[derive(Debug, Default)]
struct Asking {
    quiet: u32,
    request: ConsentRequest,
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
        } else {
            match self.request {
                ConsentRequest::Unasked => {
                    self.request = ConsentRequest::Requested(0);
                    Nudge::Request
                }
                ConsentRequest::Requested(since) if since >= settings_after => {
                    self.request = ConsentRequest::Opened;
                    Nudge::OpenSettings
                }
                ConsentRequest::Requested(_) | ConsentRequest::Opened => Nudge::Wait,
            }
        };

        if let ConsentRequest::Requested(since) = &mut self.request {
            *since += 1;
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
        atlas_core::config::home().is_ok_and(|home| {
            crate::helper_setup::pairing::window_open(&home)
                || crate::helper_setup::first_login::opener_waiting(&crate::helper_setup::Layout {
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
pub(super) async fn restart_for_recording(restart: impl Fn()) {
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
    let Some(path) = note("recording-restart") else {
        return false;
    };

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
            arrival_notice(controller, Arrival::SetUp);
        } else if just_logged_in().await {
            arrival_notice(controller, Arrival::Running);
        }

        if !capture::permitted() {
            restart();
        }

        return;
    }

    asked(true);

    if !accessibility_granted(shown).await {
        return;
    }

    if !recording().await && !recording_granted(shown).await {
        return;
    }

    if shown() {
        asked(false);
        arrival_notice(controller, Arrival::SetUp);
        if !capture::permitted() {
            restart();
        }
    }
}

/// Ask for Accessibility as long as the person stays, however long they
/// take. Should the prompt not appear, its System Settings page opens
/// instead. Whether it is on before they left.
async fn accessibility_granted(shown: &impl Fn() -> bool) -> bool {
    if accessibility() {
        return true;
    }

    let mut asking = Asking::default();

    while !accessibility() {
        match asking.pass(guided().await, 15) {
            Nudge::Request => request_accessibility(),
            Nudge::OpenSettings => open_settings("Privacy_Accessibility"),
            Nudge::Wait => {}
        }

        tokio::time::sleep(Duration::from_secs(1)).await;
        if !shown() {
            return false;
        }
    }

    tokio::time::sleep(Duration::from_secs(1)).await;
    true
}

/// Ask for Screen Recording, just read as off: passes ask (once quiet)
/// before reading it again. Whether it is on before the person left.
async fn recording_granted(shown: &impl Fn() -> bool) -> bool {
    let mut asking = Asking::default();

    loop {
        if !shown() {
            return false;
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
            return true;
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
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .output();
    match tokio::time::timeout(Duration::from_secs(5), run).await {
        Ok(Ok(output)) => serde_json::from_slice::<Value>(&output.stdout)
            .is_ok_and(|reply| reply["screenCapture"] == true),
        _ => false,
    }
}

/// Remember (`true`) that Atlas asked the person for a permission, or forget
/// it (`false`), returning whether it was remembered. It outlives this
/// process, which macOS's Quit & Reopen restarts mid-way.
fn asked(remember: bool) -> bool {
    let Some(path) = note("consent-asked") else {
        return false;
    };

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

/// What the person, just arrived in the helper, is told.
enum Arrival {
    SetUp,
    Running,
}

/// Tell the person `arrival`, with a button that switches back to
/// `controller` when that account is known. Without it, that Atlas runs here
/// again goes unsaid.
fn arrival_notice(controller: u32, arrival: Arrival) {
    let account = atlas_core::account::account_with_uid(controller)
        .ok()
        .flatten();
    let title = account.as_ref().map(|account| {
        atlas_core::account::real_name(&account.name).unwrap_or_else(|| account.name.clone())
    });
    let (heading, message) = match arrival {
        Arrival::SetUp => ("Atlas is set up", done_message(title.as_deref())),
        Arrival::Running => {
            let Some(title) = title else {
                return;
            };
            ("Atlas is running here again", switch_back(&title))
        }
    };
    notice(
        heading,
        &message,
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

/// Where the login a notice was last shown in is kept: a daemon restarted
/// to pick up Screen Recording never repeats what its predecessor showed.
fn notice_record(home: &Path) -> PathBuf {
    home.join(crate::helper_setup::ROOT).join("notice-shown")
}

/// Whether a notice was shown already in the login with this loginwindow.
fn shown_in(home: &Path, login: u32) -> bool {
    std::fs::read_to_string(notice_record(home)).is_ok_and(|shown| shown == login.to_string())
}

/// Tell the person something in the helper, at most once a login, with a
/// Switch Back button to `switch_back` when given. The notice runs on its
/// own; its process is collected when it closes.
fn notice(title: &str, message: &str, switch_back: Option<&str>) {
    use std::os::unix::process::CommandExt;

    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
        return;
    };

    let login = atlas_core::session::login(nix::unistd::getuid().as_raw()).map(|login| login.pid);
    // The onboarding window, when open here, tells the person itself.
    if crate::helper_setup::pairing::window_open(&home)
        || login.is_some_and(|pid| shown_in(&home, pid))
    {
        return;
    }

    let Ok(Some(app)) = atlas_core::app_bundle::shipped_app() else {
        return;
    };

    let mut arguments = vec!["--notice", title, message];

    if let Some(account) = switch_back {
        arguments.extend(["--switch-back", account]);
    }

    // Its own process group: restarting this daemon (to pick up Screen
    // Recording) must not close the notice the person is reading.
    let shown = silent(Command::new(app).process_group(0).args(arguments)).spawn();
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
        .stdin(Stdio::null())
        .stderr(Stdio::null())
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

/// Open a Privacy & Security page of System Settings in this session.
fn open_settings(page: &str) {
    let mut command = Command::new("/usr/bin/open");
    command.arg(format!(
        "x-apple.systempreferences:com.apple.preference.security?{page}"
    ));
    let _ = silent(&mut command).status();
}

#[cfg(test)]
mod tests;

//! Getting a helper account ready for its first login, run as that account
//! (`setup-helper`): macOS's per-user setup questions marked as answered, so
//! the login lands on the desktop, and a one-time job that opens Atlas
//! there once the desktop is up. Nothing here replaces a file the account
//! already has, follows a link, or writes preferences while the account runs
//! anything (cfprefsd would hold the old ones).
use super::{File, Layout, document, escape, file, pairing, problem, text_sha256};
use atlas_core::error::{Error, Result};
use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, Instant},
};

/// Label of the one-time job that opens Atlas at the helper's next login.
const OPENER: &str = "com.insanearts.atlas.first-login";
const SETUP_ASSISTANT: &str = "Library/Preferences/com.apple.SetupAssistant.plist";
const PRIVACY: &str = "Library/Preferences/com.apple.setupassistant.privacypane.plist";
/// The file whose presence tells MiniLauncher not to start Setup Assistant.
const SKIP: &str = ".skipbuddy";

/// Flags set true: each question macOS 27 asks a new account marked as seen,
/// and the two first-login optimisations skipped.
const SEEN: &[&str] = &[
    "DidSeeAccessibility",
    "DidSeeActivationLock",
    "DidSeeAppStore",
    "DidSeeAppearanceSetup",
    "DidSeeApplePaySetup",
    "DidSeeCloudSetup",
    "DidSeeiCloudLoginForStorageServices",
    "DidSeeLockdownMode",
    "DidSeePrivacy",
    "DidSeeScreenTime",
    "DidSeeSiriSetup",
    "DidSeeSyncSetup",
    "DidSeeSyncSetup2",
    "DidSeeTermsOfAddress",
    "DidSeeTouchIDSetup",
    "SkipFirstLoginOptimization",
    "SkipExpressSettingsUpdating",
];

/// Keys Setup Assistant compares with the running macOS version.
const VERSION_STAMPED: &[&str] = &[
    "LastSeenCloudProductVersion",
    "LastSeenDiagnosticsProductVersion",
    "LastSeenSiriProductVersion",
    "LastSeenSyncProductVersion",
    "LastSeenIntelligenceProductVersion",
    "LastSeeniCloudStorageServicesProductVersion",
    "LastSeenAgeRangeSelectionProductVersion",
    "LastSeenGlassTintUpsellProductVersion",
    "LastPreLoginTasksPerformedVersion",
    "LastLaunchedMiniBuddyForStorageServicesVersion",
    "PreviousSystemVersion",
];

/// Keys Setup Assistant compares with the running macOS build.
const BUILD_STAMPED: &[&str] = &[
    "LastSeenBuddyBuildVersion",
    "LastPreLoginTasksPerformedBuild",
    "PreviousBuildVersion",
];

/// Why the file at `path` is left alone: it is a link, a folder or the like.
fn not_a_file(path: &Path) -> Error {
    problem(format!(
        "{} is not a file; it was not changed",
        path.display()
    ))
}

fn string(key: &str, value: &str) -> String {
    format!("\t<key>{key}</key>\n\t<string>{}</string>\n", escape(value))
}

fn boolean(key: &str, value: bool) -> String {
    format!("\t<key>{key}</key>\n\t<{value}/>\n")
}

fn array(key: &str, values: &[&str]) -> String {
    let items: String = values
        .iter()
        .map(|value| format!("\t\t<string>{}</string>\n", escape(value)))
        .collect();
    format!("\t<key>{key}</key>\n\t<array>\n{items}\t</array>\n")
}

fn integer(key: &str, value: i64) -> String {
    format!("\t<key>{key}</key>\n\t<integer>{value}</integer>\n")
}

/// Setup Assistant's preferences with every question answered for macOS
/// `version` (build `build`), as an account that went through it has them.
fn setup_assistant_plist(version: &str, build: &str) -> String {
    let mut body = String::new();
    for key in SEEN {
        body.push_str(&boolean(key, true));
    }
    for key in VERSION_STAMPED {
        body.push_str(&string(key, version));
    }
    for key in BUILD_STAMPED {
        body.push_str(&string(key, build));
    }
    body.push_str(&integer("LastPrivacyBundleVersion", 2));
    body.push_str(&integer("MiniBuddyLaunchReason", 0));
    body.push_str(&boolean("MiniBuddyShouldLaunchToResumeSetup", false));
    body.push_str(&boolean("MiniBuddyLaunchedPostMigration", false));

    document(&body)
}

/// The Data & Privacy pane's preferences once it has been seen, as the
/// helper made on macOS 27.0.1 has them.
fn privacy_plist() -> String {
    document(
        &[
            boolean("HasSeenPrivacy", true),
            integer("LastSeenPrivacyVersion", 2),
            integer("MigrationVersion", 1),
        ]
        .concat(),
    )
}

/// The one-time job that runs Atlas's `program` at the next login, as if
/// the person had opened Atlas; Atlas, seeing its label, waits for the
/// desktop, opens itself and removes the job.
fn opener_plist(program: &Path) -> String {
    document(
        &[
            string("Label", OPENER),
            array("ProgramArguments", &[&program.to_string_lossy()]),
            boolean("RunAtLoad", true),
            boolean("KeepAlive", false),
            string("LimitLoadToSessionType", "Aqua"),
            string("ProcessType", "Interactive"),
            // Named so Background Items lists the job as Atlas.
            array(
                "AssociatedBundleIdentifiers",
                &[atlas_core::app_bundle::IDENTIFIER],
            ),
            string("StandardOutPath", "/dev/null"),
            string("StandardErrorPath", "/dev/null"),
        ]
        .concat(),
    )
}

/// macOS's version and build, as Setup Assistant records them.
fn macos() -> Result<(String, String)> {
    let read = |flag: &str| -> Result<String> {
        let output = atlas_core::spawn::output(Command::new("/usr/bin/sw_vers").arg(flag))?;
        let text = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        if !output.status.success() || text.is_empty() {
            return Err(problem(format!("sw_vers {flag} gave no answer")));
        }
        Ok(text)
    };

    Ok((read("-productVersion")?, read("-buildVersion")?))
}

/// Whether `path` is a real directory the account owns that no one else may
/// write to (another account could plant a login item in it otherwise).
fn private_enough(path: &Path, uid: u32) -> Result<bool> {
    let metadata = fs::symlink_metadata(path)?;
    Ok(metadata.is_dir()
        && metadata.uid() == uid
        && metadata.mode() & 0o022 == 0
        && !atlas_core::acl::allows_write(path)?)
}

/// The folder `relative` in the home, a real directory the account owns
/// that no one else may write to, made private (0700) with its missing
/// parents when absent.
fn owned_folder(layout: &Layout, relative: &str) -> Result<PathBuf> {
    let mut path = layout.home.clone();
    if !private_enough(&path, layout.uid)? {
        return Err(problem(
            "The home is not a folder only this account can change",
        ));
    }
    for part in Path::new(relative) {
        path.push(part);
        layout.check()?;
        match fs::DirBuilder::new().mode(0o700).create(&path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.into()),
        }

        if !private_enough(&path, layout.uid)? {
            return Err(problem(format!(
                "{} is not a folder only this account can change; nothing more was written",
                path.display()
            )));
        }
    }

    Ok(path)
}

/// Write `bytes` to `path` with mode 0600 unless something is there
/// already: the file is linked into place, so an existing one, or a link,
/// is kept as it is. Whether it was written.
fn write_if_absent(path: &Path, bytes: &[u8]) -> Result<bool> {
    if fs::symlink_metadata(path).is_ok() {
        return Ok(false);
    }

    let mut written = true;
    atlas_core::files::write_beside(
        path,
        0o600,
        |file| {
            file.set_permissions(fs::Permissions::from_mode(0o600))?;
            Ok(file.write_all(bytes)?)
        },
        |temporary| match fs::hard_link(temporary, path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                written = false;
                Ok(())
            }
            Err(error) => Err(error.into()),
        },
    )?;

    Ok(written)
}

/// What [`ready_with`] found and did.
#[derive(Debug, PartialEq, Eq)]
struct Readied {
    /// The files written now; empty when they were all there.
    answered: Vec<PathBuf>,
    /// The opener's plist, when one was asked for.
    opener: Option<PathBuf>,
    /// Whether the opener's plist was written or replaced now.
    opener_written: bool,
}

/// Get the account ready: macOS's setup questions answered (with the skip
/// file) only in an account that was never set up and has no desktop login
/// (`logged_in`), so a person's own account keeps every question macOS asks
/// it. A new account may already run macOS's own background services
/// (distnoted, cfprefsd), which say nothing of a person using it;
/// and, given `program`, the opener in place. Run as the account itself;
/// `version` and `build` are macOS's. An account with Atlas settings of its
/// own is no helper, so it gets no opener, and a home others may write to
/// gets nothing.
fn ready_with(
    layout: &Layout,
    logged_in: bool,
    program: Option<&Path>,
    (version, build): (&str, &str),
    mut write: impl FnMut(&Path, &[u8]) -> Result<bool>,
) -> Result<Readied> {
    if program.is_some()
        && fs::symlink_metadata(layout.home.join(atlas_core::config::SETTINGS)).is_ok()
    {
        return Err(problem(
            "This account has Atlas settings of its own, so it cannot be a helper",
        ));
    }

    if !private_enough(&layout.home, layout.uid)? {
        return Err(problem(format!(
            "{} is not a home only this account can change; nothing was written",
            layout.home.display()
        )));
    }

    // Setup Assistant's own preferences mark an account that was set up.
    let never_set_up = fs::symlink_metadata(layout.home.join(SETUP_ASSISTANT)).is_err();
    let mut answered = Vec::new();
    if never_set_up && !logged_in {
        owned_folder(layout, "Library/Preferences")?;
        let files = [
            (layout.home.join(PRIVACY), privacy_plist()),
            (layout.home.join(SKIP), String::new()),
            (
                layout.home.join(SETUP_ASSISTANT),
                setup_assistant_plist(version, build),
            ),
        ];
        for (path, text) in files {
            layout.check()?;
            if write(&path, text.as_bytes())? {
                answered.push(path);
            }
        }
    }

    let (opener, opener_written) = match program {
        None => (None, false),
        Some(program) => {
            owned_folder(layout, "Library/LaunchAgents")?;
            let path = layout.agent_plist(OPENER);
            let text = opener_plist(program);
            let written = match file(&path)? {
                File::Regular(sha256) if sha256 == text_sha256(&text) => false,
                File::Absent | File::Regular(_) => {
                    layout.check()?;
                    atlas_core::files::write_atomic(&path, text.as_bytes(), 0o644)?;
                    true
                }
                File::Other => return Err(not_a_file(&path)),
            };
            (Some(path), written)
        }
    };

    Ok(Readied {
        answered,
        opener,
        opener_written,
    })
}

/// `setup-helper`'s first-login action: [`ready_with`] with this Mac's facts, and
/// the opener started now when the account has a desktop login, which
/// loaded its LaunchAgents when it began. Run through `launchctl asuser`
/// for that, so launchctl reaches the login.
pub(super) fn first_login(layout: &Layout, opener: bool) -> Result<Value> {
    let logged_in = atlas_core::session::login(layout.uid).is_some();
    let program = if opener {
        Some(atlas_core::app_bundle::signed_executable()?)
    } else {
        None
    };
    let (version, build) = macos()?;
    let readied = ready_with(
        layout,
        logged_in,
        program.as_deref(),
        (&version, &build),
        write_if_absent,
    )?;

    // Without an opener asked for (atlas init sets the account up itself),
    // one a window left would only race it.
    if !opener {
        forget_opener(layout)?;
    }

    // The opener runs once, so a loaded one that has run is loaded again.
    let started = match &readied.opener {
        Some(path) if logged_in => {
            if readied.opener_written || super::job_pid(layout, OPENER).is_none() {
                super::unload(layout, OPENER)?;
            }
            super::loaded(layout, OPENER, path)?;
            true
        }
        _ => false,
    };

    Ok(json!({
        "ok": true,
        "answered": readied.answered,
        "loggedIn": logged_in,
        "opener": readied.opener,
        "started": started,
    }))
}

/// `setup-helper`'s action that takes a waiting opener away: its plist
/// removed first (a plist left behind would run at the next login), then
/// the job unloaded if launchd has it. Only a regular file is removed.
pub(crate) fn forget_opener(layout: &Layout) -> Result<Value> {
    let path = layout.agent_plist(OPENER);
    let removed = match file(&path)? {
        File::Absent => false,
        File::Regular(_) => {
            layout.check()?;
            fs::remove_file(&path)?;
            true
        }
        File::Other => return Err(not_a_file(&path)),
    };

    super::unload(layout, OPENER)?;
    Ok(json!({ "ok": true, "removed": removed, "plist": path }))
}

/// How long the opener waits for the desktop: Setup Assistant may be up
/// for as long as the person takes; after this it tries at the next login.
const DESKTOP_PATIENCE: Duration = Duration::from_secs(60 * 60);
/// How long the opener waits for the request to name this account: when
/// Atlas readies a helper that is logged in, the opener starts at once, a
/// moment before the asking account writes its request.
const REQUEST_PATIENCE: Duration = Duration::from_secs(120);
/// How long the opener waits for Atlas's window once it asked for it: a
/// first login is busy, and waiting longer costs nothing.
const WINDOW_PATIENCE: Duration = Duration::from_secs(120);

/// Whether this account's opener still waits to open Atlas's setup window:
/// until it has, the window is on its way to guide the person. An opener
/// that gave up (its job no longer running) leaves its plist for the next
/// login, and does not count.
pub(crate) fn opener_waiting(layout: &Layout) -> bool {
    fs::symlink_metadata(layout.agent_plist(OPENER)).is_ok_and(|metadata| metadata.is_file())
        && super::job_pid(layout, OPENER).is_some()
}

/// Whether launchd started this process as the helper's opener: it names
/// the job in `XPC_SERVICE_NAME`, and gives an app LaunchServices opens
/// another name.
pub fn started_as_opener() -> bool {
    std::env::var("XPC_SERVICE_NAME").as_deref() == Ok(OPENER)
}

/// Whether the opener should open Atlas: the account is the helper another
/// account asked for (`requested`) and has no Atlas settings of its own.
fn wanted(requested: bool, own_settings: bool) -> bool {
    requested && !own_settings
}

fn wanted_here(layout: &Layout) -> bool {
    let requested = pairing::request().is_some_and(|pairing| pairing.helper_uid == layout.uid);
    wanted(
        requested,
        fs::symlink_metadata(layout.home.join(atlas_core::config::SETTINGS)).is_ok(),
    )
}

/// The opener, run by launchd at login: once the desktop is up, open a new
/// Atlas as the person would (through LaunchServices, so the app is not
/// this job's process, and a new one, so an Atlas notice already up is not
/// just brought forward), and once its window is open, take the opener
/// away, which ends this process. An account that is no longer the helper
/// asked for loses the opener without anything opening; a desktop or window
/// that never comes leaves it for the next login.
///
/// # Errors
///
/// Returns failures locating or opening Atlas in the current account's login session.
pub fn open_at_login() -> Result<()> {
    use atlas_core::login_log::{mark, seconds};

    let uid = nix::unistd::getuid().as_raw();
    let layout = Layout::new(uid, atlas_core::config::home()?);
    mark(&format!("opener.start uid={uid}"));

    let deadline = Instant::now() + REQUEST_PATIENCE;
    while !wanted_here(&layout) {
        if Instant::now() >= deadline {
            forget_opener(&layout)?;
            return Ok(());
        }
        std::thread::sleep(Duration::from_secs(1));
    }

    let waiting = Instant::now();
    let deadline = waiting + DESKTOP_PATIENCE;
    let mut assistant = false;
    while wanted_here(&layout) && atlas_core::session::desktop_up(uid) != Some(true) {
        assistant |= atlas_core::session::setup_assistant(uid) == Some(true);
        if Instant::now() >= deadline {
            return Err(problem(
                "The desktop never came up; Atlas opens at the next login",
            ));
        }
        std::thread::sleep(Duration::from_secs(2));
    }
    mark(&format!(
        "opener.desktop waited={} setupAssistant={assistant}",
        seconds(waiting.elapsed())
    ));

    // Asked again once the desktop is up: the choice may have changed while
    // Setup Assistant waited for the person.
    if !wanted_here(&layout) || pairing::window_open(&layout.home) {
        forget_opener(&layout)?;
        return Ok(());
    }

    // The Dock and the Finder are up; give the login a moment to settle.
    std::thread::sleep(Duration::from_secs(2));
    let app = atlas_core::app_bundle::installed_app()?;
    mark("opener.open");
    let status =
        atlas_core::spawn::status(Command::new("/usr/bin/open").args(["-n", "-a"]).arg(&app))?;
    if !status.success() {
        return Err(problem(format!("{} could not be opened", app.display())));
    }

    let opening = Instant::now();
    let deadline = opening + WINDOW_PATIENCE;
    while !pairing::window_open(&layout.home) {
        if Instant::now() >= deadline {
            return Err(problem(
                "Atlas's window did not open; Atlas opens at the next login",
            ));
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    mark(&format!(
        "opener.window waited={}",
        seconds(opening.elapsed())
    ));

    forget_opener(&layout)?;
    Ok(())
}

#[cfg(test)]
mod tests;

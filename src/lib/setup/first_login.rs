//! Getting a helper account ready for its first login, run as that account
//! (`setup-helper`): macOS's per-user setup questions marked as answered, so
//! the login lands on the desktop, and a one-time job that opens Atlas
//! there once the desktop is up. Nothing here replaces a file the account
//! already has, follows a link, or writes preferences while the account runs
//! anything (cfprefsd would hold the old ones).
use super::helper::{File, Layout, escape, file, text_sha256};
use crate::error::{Error, Result};
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
/// Setup Assistant's per-user preferences.
const SETUP_ASSISTANT: &str = "Library/Preferences/com.apple.SetupAssistant.plist";
/// The Data & Privacy pane's own preferences.
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

fn problem(message: impl Into<String>) -> Error {
    Error::new("setup", message)
}

/// A plist document around `body`, a dictionary's entries.
fn document(body: &str) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \
         \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
         <plist version=\"1.0\">\n<dict>\n{body}</dict>\n</plist>\n"
    )
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
            array("AssociatedBundleIdentifiers", &[super::IDENTIFIER]),
            string("StandardOutPath", "/dev/null"),
            string("StandardErrorPath", "/dev/null"),
        ]
        .concat(),
    )
}

/// macOS's version and build, as Setup Assistant records them.
fn macos() -> Result<(String, String)> {
    let read = |flag: &str| -> Result<String> {
        let output = Command::new("/usr/bin/sw_vers").arg(flag).output()?;
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
    Ok(metadata.is_dir() && metadata.uid() == uid && metadata.mode() & 0o022 == 0)
}

/// The folder `relative` in the home, a real directory the account owns
/// that no one else may write to, made private (0700) with its missing
/// parents when absent.
fn owned_folder(layout: &Layout, relative: &str) -> Result<PathBuf> {
    let mut path = layout.home.clone();
    for part in Path::new(relative) {
        path.push(part);
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
    crate::config::write_beside(
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

/// What [`ready`] found and did.
#[derive(Debug, PartialEq, Eq)]
struct Readied {
    /// The files written now; empty when they were all there.
    answered: Vec<PathBuf>,
    /// Whether the account has a desktop login, so its preferences were left.
    logged_in: bool,
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
fn ready(
    layout: &Layout,
    logged_in: bool,
    program: Option<&Path>,
    (version, build): (&str, &str),
) -> Result<Readied> {
    if program.is_some() && fs::symlink_metadata(layout.home.join(crate::config::SETTINGS)).is_ok()
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
            (
                layout.home.join(SETUP_ASSISTANT),
                setup_assistant_plist(version, build),
            ),
            (layout.home.join(PRIVACY), privacy_plist()),
            (layout.home.join(SKIP), String::new()),
        ];
        for (path, text) in files {
            if write_if_absent(&path, text.as_bytes())? {
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
                    crate::config::write_atomic(&path, text.as_bytes(), 0o644)?;
                    true
                }
                File::Other => {
                    return Err(problem(format!(
                        "{} is not a file; it was not changed",
                        path.display()
                    )));
                }
            };
            (Some(path), written)
        }
    };

    Ok(Readied {
        answered,
        logged_in,
        opener,
        opener_written,
    })
}

/// `setup-helper`'s first-login action: [`ready`] with this Mac's facts, and
/// the opener started now when the account has a desktop login, which
/// loaded its LaunchAgents when it began. Run through `launchctl asuser`
/// for that, so launchctl reaches the login.
pub(super) fn first_login(layout: &Layout, opener: bool) -> Result<Value> {
    let logged_in = crate::session::login(layout.uid).is_some();
    let program = if opener {
        Some(crate::init::app_executable()?)
    } else {
        None
    };
    let (version, build) = macos()?;
    let readied = ready(layout, logged_in, program.as_deref(), (&version, &build))?;

    // Without an opener asked for (atlas init sets the account up itself),
    // one a window left would only race it.
    if !opener {
        forget_opener(layout)?;
    }

    // The opener runs once, so a loaded one that has run is loaded again.
    let started = match &readied.opener {
        Some(path) if logged_in => {
            if readied.opener_written || super::helper::job_pid(layout, OPENER).is_none() {
                super::helper::unload(layout, OPENER)?;
            }
            super::helper::loaded(layout, OPENER, path)?;
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
            fs::remove_file(&path)?;
            true
        }
        File::Other => {
            return Err(problem(format!(
                "{} is not a file; it was not changed",
                path.display()
            )));
        }
    };

    super::helper::unload(layout, OPENER)?;
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
        && super::helper::job_pid(layout, OPENER).is_some()
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

/// [`wanted`] for this account now.
fn wanted_here(layout: &Layout) -> bool {
    let requested =
        crate::onboarding::request().is_some_and(|pairing| pairing.helper_uid == layout.uid);
    wanted(
        requested,
        fs::symlink_metadata(layout.home.join(crate::config::SETTINGS)).is_ok(),
    )
}

/// The opener, run by launchd at login: once the desktop is up, open a new
/// Atlas as the person would (through LaunchServices, so the app is not
/// this job's process, and a new one, so an Atlas notice already up is not
/// just brought forward), and once its window is open, take the opener
/// away, which ends this process. An account that is no longer the helper
/// asked for loses the opener without anything opening; a desktop or window
/// that never comes leaves it for the next login.
pub fn open_at_login() -> Result<()> {
    let uid = nix::unistd::getuid().as_raw();
    let layout = Layout {
        uid,
        home: crate::config::home()?,
    };

    let deadline = Instant::now() + REQUEST_PATIENCE;
    while !wanted_here(&layout) {
        if Instant::now() >= deadline {
            forget_opener(&layout)?;
            return Ok(());
        }
        std::thread::sleep(Duration::from_secs(1));
    }

    let deadline = Instant::now() + DESKTOP_PATIENCE;
    while wanted_here(&layout) && crate::session::desktop_up(uid) != Some(true) {
        if Instant::now() >= deadline {
            return Err(problem(
                "The desktop never came up; Atlas opens at the next login",
            ));
        }
        std::thread::sleep(Duration::from_secs(2));
    }

    // Asked again once the desktop is up: the choice may have changed while
    // Setup Assistant waited for the person.
    if !wanted_here(&layout) || crate::onboarding::window_open(&layout.home) {
        forget_opener(&layout)?;
        return Ok(());
    }

    // The Dock and the Finder are up; give the login a moment to settle.
    std::thread::sleep(Duration::from_secs(2));
    let app = crate::update::installed_app()?;
    let status = Command::new("/usr/bin/open")
        .args(["-n", "-a"])
        .arg(&app)
        .status()?;
    if !status.success() {
        return Err(problem(format!("{} could not be opened", app.display())));
    }

    let deadline = Instant::now() + WINDOW_PATIENCE;
    while !crate::onboarding::window_open(&layout.home) {
        if Instant::now() >= deadline {
            return Err(problem(
                "Atlas's window did not open; Atlas opens at the next login",
            ));
        }
        std::thread::sleep(Duration::from_millis(500));
    }

    forget_opener(&layout)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const MACOS: (&str, &str) = ("27.0.1", "26A434");

    fn lint(text: &str) -> Value {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.plist");
        fs::write(&path, text).unwrap();
        let lint = Command::new("/usr/bin/plutil")
            .arg("-lint")
            .arg(&path)
            .output()
            .unwrap();
        assert!(lint.status.success(), "{text}");
        let json = Command::new("/usr/bin/plutil")
            .args(["-convert", "json", "-o", "-"])
            .arg(&path)
            .output()
            .unwrap();
        serde_json::from_slice(&json.stdout).unwrap()
    }

    fn layout() -> (tempfile::TempDir, Layout) {
        let dir = tempfile::tempdir().unwrap();
        let layout = Layout {
            uid: nix::unistd::getuid().as_raw(),
            home: dir.path().to_path_buf(),
        };
        (dir, layout)
    }

    fn names(folder: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(folder)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn setup_assistant_answers_every_question_for_this_macos() {
        let value = lint(&setup_assistant_plist("27.9", "26Z999"));
        for key in SEEN {
            assert_eq!(value[key], true, "{key}");
        }
        for key in VERSION_STAMPED {
            assert_eq!(value[key], "27.9", "{key}");
        }
        for key in BUILD_STAMPED {
            assert_eq!(value[key], "26Z999", "{key}");
        }
        assert_eq!(value["LastPrivacyBundleVersion"], 2);
        assert_eq!(value["MiniBuddyLaunchReason"], 0);
        assert_eq!(value["MiniBuddyShouldLaunchToResumeSetup"], false);
        assert_eq!(value["MiniBuddyLaunchedPostMigration"], false);
        assert!(value.get("MiniBuddyLaunch").is_none());

        let privacy = lint(&privacy_plist());
        assert_eq!(privacy["HasSeenPrivacy"], true);
        assert_eq!(privacy["LastSeenPrivacyVersion"], 2);
        assert_eq!(privacy["MigrationVersion"], 1);
    }

    #[test]
    fn the_opener_runs_atlas_once_at_login_and_is_named_atlas() {
        let value = lint(&opener_plist(Path::new(
            "/Applications/A&B.app/Contents/MacOS/atlas",
        )));
        assert_eq!(value["Label"], OPENER);
        assert_eq!(
            value["ProgramArguments"],
            json!(["/Applications/A&B.app/Contents/MacOS/atlas"])
        );
        assert_eq!(value["RunAtLoad"], true);
        assert_eq!(value["KeepAlive"], false);
        assert_eq!(value["LimitLoadToSessionType"], "Aqua");
        assert_eq!(
            value["AssociatedBundleIdentifiers"],
            json!([super::super::IDENTIFIER])
        );
    }

    #[test]
    fn a_new_account_gets_its_answers_and_the_opener_and_nothing_else() {
        let (_dir, layout) = layout();
        let program = Path::new("/Applications/Atlas.app/Contents/MacOS/atlas");
        let readied = ready(&layout, false, Some(program), MACOS).unwrap();

        assert_eq!(readied.answered.len(), 3);
        assert!(readied.opener_written);
        assert!(layout.home.join(SKIP).is_file());
        assert_eq!(
            names(&layout.home.join("Library/Preferences")),
            [
                "com.apple.SetupAssistant.plist",
                "com.apple.setupassistant.privacypane.plist"
            ],
            "no temporary file is left behind"
        );
        let prefs = fs::metadata(layout.home.join(SETUP_ASSISTANT)).unwrap();
        assert_eq!(prefs.mode() & 0o777, 0o600);
        let folder = fs::metadata(layout.home.join("Library/LaunchAgents")).unwrap();
        assert_eq!(folder.mode() & 0o777, 0o700);
        let opener = readied.opener.unwrap();
        assert_eq!(fs::metadata(&opener).unwrap().mode() & 0o777, 0o644);
        assert_eq!(fs::read_to_string(&opener).unwrap(), opener_plist(program));

        let again = ready(&layout, false, Some(program), MACOS).unwrap();
        assert!(again.answered.is_empty());
        assert!(!again.opener_written, "the same opener is left as it is");
    }

    #[test]
    fn an_account_that_was_set_up_or_runs_something_keeps_every_question_macos_asks() {
        let (_dir, layout) = layout();
        fs::create_dir_all(layout.home.join("Library/Preferences")).unwrap();
        fs::write(layout.home.join(SETUP_ASSISTANT), "theirs").unwrap();
        let program = Path::new("/Applications/Atlas.app/Contents/MacOS/atlas");

        let readied = ready(&layout, false, Some(program), MACOS).unwrap();
        assert!(readied.answered.is_empty());
        assert!(!layout.home.join(SKIP).exists());
        assert!(!layout.home.join(PRIVACY).exists());
        assert_eq!(
            fs::read_to_string(layout.home.join(SETUP_ASSISTANT)).unwrap(),
            "theirs"
        );
        assert!(readied.opener.is_some(), "the opener still goes in");

        let (_dir, logged_in) = self::layout();
        let readied = ready(&logged_in, true, None, MACOS).unwrap();
        assert!(readied.answered.is_empty());
        assert!(!logged_in.home.join("Library").exists());
        assert!(!logged_in.home.join(SKIP).exists());
    }

    #[test]
    fn a_home_or_folder_others_may_write_to_gets_nothing() {
        use std::os::unix::fs::PermissionsExt;

        let (_dir, layout) = layout();
        fs::set_permissions(&layout.home, fs::Permissions::from_mode(0o777)).unwrap();
        let program = Path::new("/Applications/Atlas.app/Contents/MacOS/atlas");
        assert!(ready(&layout, false, Some(program), MACOS).is_err());
        assert_eq!(names(&layout.home), Vec::<String>::new());

        let (_dir, layout) = self::layout();
        fs::create_dir_all(layout.home.join("Library/LaunchAgents")).unwrap();
        fs::set_permissions(
            layout.home.join("Library/LaunchAgents"),
            fs::Permissions::from_mode(0o777),
        )
        .unwrap();
        assert!(ready(&layout, true, Some(program), MACOS).is_err());
        assert!(!layout.agent_plist(OPENER).exists());
    }

    #[test]
    fn a_file_that_appears_meanwhile_is_kept() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("prefs.plist");
        fs::write(&path, "theirs").unwrap();
        assert!(!write_if_absent(&path, b"ours").unwrap());
        assert_eq!(fs::read_to_string(&path).unwrap(), "theirs");
        assert_eq!(names(dir.path()), ["prefs.plist"]);
    }

    #[test]
    fn links_in_the_home_are_never_followed() {
        let (_dir, layout) = layout();
        let elsewhere = tempfile::tempdir().unwrap();
        fs::create_dir_all(layout.home.join("Library")).unwrap();
        std::os::unix::fs::symlink(elsewhere.path(), layout.home.join("Library/Preferences"))
            .unwrap();
        assert!(ready(&layout, false, None, MACOS).is_err());
        assert_eq!(fs::read_dir(elsewhere.path()).unwrap().count(), 0);

        let (_dir, layout) = self::layout();
        fs::create_dir_all(layout.home.join("Library/Preferences")).unwrap();
        let target = elsewhere.path().join("target");
        std::os::unix::fs::symlink(&target, layout.home.join(SETUP_ASSISTANT)).unwrap();
        ready(&layout, false, None, MACOS).unwrap();
        assert!(
            !target.exists(),
            "an existing link is kept, not written through"
        );
    }

    #[test]
    fn an_opener_of_another_version_is_replaced_and_a_folder_in_its_place_refused() {
        let (_dir, layout) = layout();
        let program = Path::new("/Applications/Atlas.app/Contents/MacOS/atlas");
        let path = layout.agent_plist(OPENER);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "older").unwrap();
        let readied = ready(&layout, true, Some(program), MACOS).unwrap();
        assert!(readied.opener_written);
        assert_eq!(fs::read_to_string(&path).unwrap(), opener_plist(program));

        fs::remove_file(&path).unwrap();
        fs::create_dir(&path).unwrap();
        assert!(ready(&layout, true, Some(program), MACOS).is_err());
    }

    #[test]
    fn the_opener_opens_atlas_only_for_the_helper_asked_for() {
        assert!(wanted(true, false), "asked for");
        assert!(!wanted(false, false), "another account was chosen since");
        assert!(
            !wanted(true, true),
            "this account has Atlas settings of its own"
        );
    }

    #[test]
    fn an_account_with_its_own_atlas_settings_gets_no_opener() {
        let (_dir, layout) = layout();
        fs::create_dir_all(layout.home.join(".config")).unwrap();
        fs::write(layout.home.join(crate::config::SETTINGS), "").unwrap();
        let program = Path::new("/Applications/Atlas.app/Contents/MacOS/atlas");
        assert!(ready(&layout, false, Some(program), MACOS).is_err());
        assert!(!layout.agent_plist(OPENER).exists());
        assert!(
            !layout.home.join(SKIP).exists(),
            "refused before writing anything"
        );
    }
}

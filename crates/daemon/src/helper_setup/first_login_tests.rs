use super::*;

const MACOS: (&str, &str) = ("27.0.1", "26A434");

fn ready(
    layout: &Layout,
    logged_in: bool,
    program: Option<&Path>,
    macos: (&str, &str),
) -> Result<Readied> {
    ready_with(layout, logged_in, program, macos, write_if_absent)
}

fn lint(text: &str) -> Value {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("x.plist");
    fs::write(&path, text).unwrap();
    let lint =
        atlas_core::spawn::output(Command::new("/usr/bin/plutil").arg("-lint").arg(&path)).unwrap();
    assert!(lint.status.success(), "{text}");
    let json = atlas_core::spawn::output(
        Command::new("/usr/bin/plutil")
            .args(["-convert", "json", "-o", "-"])
            .arg(&path),
    )
    .unwrap();
    serde_json::from_slice(&json.stdout).unwrap()
}

fn layout() -> (tempfile::TempDir, Layout) {
    let dir = tempfile::tempdir().unwrap();
    let layout = Layout::new(nix::unistd::getuid().as_raw(), dir.path().to_path_buf());
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
        json!([atlas_core::app_bundle::IDENTIFIER])
    );
}

#[test]
fn a_staged_worker_records_the_installed_opener_and_passes_the_allow_identity_check() {
    const TEST: &str = "helper_setup::first_login::tests::a_staged_worker_records_the_installed_opener_and_passes_the_allow_identity_check";
    const DIRECTORY: &str = "ATLAS_FIRST_LOGIN_TEST_DIRECTORY";
    let Some(directory) = std::env::var_os(DIRECTORY).map(PathBuf::from) else {
        let dir = tempfile::tempdir().unwrap();
        let worker = dir
            .path()
            .join("atlas-approved.test/Atlas.app/Contents/MacOS/atlas");
        fs::create_dir_all(worker.parent().unwrap()).unwrap();
        fs::copy(std::env::current_exe().unwrap(), &worker).unwrap();
        let output = atlas_core::spawn::output(
            Command::new(worker)
                .args(["--exact", TEST, "--nocapture"])
                .env(DIRECTORY, dir.path()),
        )
        .unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    };
    let running = std::env::current_exe().unwrap();
    assert!(running.starts_with(directory.join("atlas-approved.test")));
    let installed = Path::new("/Applications/Atlas.app/Contents/MacOS/atlas");
    // Stand in only for disk reachability and codesign. Resolve paths,
    // write the real plist and apply Allow's actual identity checks.
    let program = atlas_core::test_support::app_executable_using(|path| {
        assert_eq!(path, installed);
        true
    })
    .unwrap();
    let program = atlas_core::test_support::signed_executable_using(program, |path| {
        assert_eq!(path, installed);
        true
    })
    .unwrap();
    let (_home, layout) = layout();
    let readied = ready(&layout, false, Some(&program), MACOS).unwrap();
    let opener = lint(&fs::read_to_string(readied.opener.unwrap()).unwrap());
    let opened = PathBuf::from(opener["ProgramArguments"][0].as_str().unwrap());
    assert_eq!(opened, installed);

    let (allowed, approvals) = atlas_native::approval::without_authorization(|| {
        atlas_core::test_support::signed_executable_using(opened, |path| path == installed)
    });
    assert_eq!(allowed.unwrap(), installed);
    assert_eq!(approvals, 0);
    assert!(
        atlas_core::test_support::signed_executable_using(program, |_| false).is_err(),
        "the installed app must still pass signature verification"
    );
    assert!(
        atlas_core::test_support::signed_executable_using(running, |_| true).is_err(),
        "a staged copy is never accepted as the installed identity"
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
fn a_failed_first_login_write_leaves_preparation_retryable() {
    for relative in [PRIVACY, SKIP, SETUP_ASSISTANT] {
        let (_dir, layout) = layout();
        let failed = layout.home.join(relative);
        let mut injected = false;
        let result = ready_with(&layout, false, None, MACOS, |path, bytes| {
            if path == failed {
                injected = true;
                return Err(std::io::Error::from_raw_os_error(libc::ENOSPC).into());
            }
            write_if_absent(path, bytes)
        });
        assert!(injected);
        assert!(result.is_err());
        assert!(!failed.exists());
        assert!(!layout.home.join(SETUP_ASSISTANT).exists());

        let readied = ready(&layout, false, None, MACOS).unwrap();
        assert!(readied.answered.contains(&failed));
        for (relative, expected) in [
            (PRIVACY, privacy_plist()),
            (SKIP, String::new()),
            (SETUP_ASSISTANT, setup_assistant_plist(MACOS.0, MACOS.1)),
        ] {
            assert_eq!(
                fs::read_to_string(layout.home.join(relative)).unwrap(),
                expected
            );
        }
        assert!(
            ready(&layout, false, None, MACOS)
                .unwrap()
                .answered
                .is_empty()
        );
    }
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
fn writable_acls_on_the_home_and_each_login_folder_are_refused() {
    let user = std::env::var("USER").unwrap();
    let program = Path::new("/Applications/Atlas.app/Contents/MacOS/atlas");
    for relative in ["", "Library", "Library/LaunchAgents"] {
        let (_dir, layout) = layout();
        let folder = layout.home.join(relative);
        fs::create_dir_all(&folder).unwrap();
        assert!(
            atlas_core::spawn::status(
                Command::new("/bin/chmod")
                    .args([
                        "+a",
                        &format!("user:{user} allow add_file,add_subdirectory")
                    ])
                    .arg(&folder)
            )
            .unwrap()
            .success()
        );
        assert!(ready(&layout, true, Some(program), MACOS).is_err());
        assert!(!layout.agent_plist(OPENER).exists());
        assert!(owned_folder(&layout, "Library/LaunchAgents").is_err());
    }
}

#[test]
fn a_new_login_folder_with_an_inherited_writable_acl_is_refused() {
    let (_dir, layout) = layout();
    let user = std::env::var("USER").unwrap();
    assert!(
        atlas_core::spawn::status(
            Command::new("/bin/chmod")
                .args([
                    "+a",
                    &format!(
                        "user:{user} allow add_file,add_subdirectory,directory_inherit,only_inherit"
                    ),
                ])
                .arg(&layout.home)
        )
        .unwrap()
        .success()
    );
    // The home itself is rejected conservatively, even for inherit-only
    // entries. A child created under it must also be rejected on its own.
    let child = layout.home.join("Library");
    fs::create_dir(&child).unwrap();
    assert!(!private_enough(&child, layout.uid).unwrap());
    assert!(owned_folder(&layout, "Library/LaunchAgents").is_err());
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
    std::os::unix::fs::symlink(elsewhere.path(), layout.home.join("Library/Preferences")).unwrap();
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
    fs::write(layout.home.join(atlas_core::config::SETTINGS), "").unwrap();
    let program = Path::new("/Applications/Atlas.app/Contents/MacOS/atlas");
    assert!(ready(&layout, false, Some(program), MACOS).is_err());
    assert!(!layout.agent_plist(OPENER).exists());
    assert!(
        !layout.home.join(SKIP).exists(),
        "refused before writing anything"
    );
}

use super::*;

fn temporary_client(config: Config) -> Result<Client> {
    let controller = atlas_control::Controller::at(
        config.helper_uid,
        &config.output_dir.parent().unwrap().join("state"),
    );
    let mut client = Client::new(config)?;
    client.controller = controller;
    Ok(client)
}

#[tokio::test]
async fn a_waiter_arriving_during_download_refuses_the_swap_under_ownership() {
    use atlas_core::host::Host;

    let directory = tempfile::tempdir().unwrap();
    let settings = directory.path().join("config.toml");
    std::fs::write(
        &settings,
        "helper_uid = 60001\nsocket_path = \"helper/missing.sock\"\n\
             identity_probe = \"atlas\"\ninput_socket = \"input.sock\"\noutput_dir = \"runs\"\n",
    )
    .unwrap();
    let mut client = temporary_client(Config::load(&settings).unwrap()).unwrap();
    client.host_session = Some(HostSession::new(Host::Named, "updater").unwrap());
    client.controller = client.controller.labelled(client.host_session.as_ref());
    let ownership = client.service_change(false).await.unwrap();
    preflight(
        &settings,
        &directory.path().join("new-atlas"),
        Some(&client),
    )
    .await
    .unwrap();

    let worker = HostSession::new(Host::Named, "worker").unwrap();
    atlas_client::test_support::waiting_note(&directory.path().join("state"), Some(&worker));

    let error = tokio::time::timeout(
        std::time::Duration::from_secs(1),
        preflight(
            &settings,
            &directory.path().join("new-atlas"),
            Some(&client),
        ),
    )
    .await
    .unwrap()
    .unwrap_err();
    assert_eq!(error.code, Code::Busy);
    assert_eq!(error.error, "worker is active in the helper.");
    assert_eq!(
        client.controller.acquire(false).unwrap_err().code,
        Code::Busy
    );
    assert!(
        client
            .controller
            .holder_note()
            .unwrap()
            .process
            .is_this_one()
    );
    drop(ownership);
}

#[tokio::test]
async fn updates_and_handover_refuse_active_work_before_replacing_anything() {
    use atlas_core::host::Host;
    use std::{fs, os::unix::fs::PermissionsExt, time::Duration};

    let directory = tempfile::tempdir().unwrap();
    let settings = directory.path().join("config.toml");
    fs::write(
        &settings,
        "helper_uid = 60001\nsocket_path = \"missing.sock\"\n\
            identity_probe = \"atlas\"\ninput_socket = \"input.sock\"\noutput_dir = \"runs\"\n",
    )
    .unwrap();
    let output = directory.path().join("runs");
    fs::create_dir(&output).unwrap();
    fs::set_permissions(&output, fs::Permissions::from_mode(0o700)).unwrap();
    let sessions = output.join("sessions");
    fs::create_dir(&sessions).unwrap();
    let host = HostSession::new(Host::Named, "worker").unwrap();
    let now = atlas_core::time::timestamp() / 1_000_000;
    let record = sessions.join(format!("{}.json", host.key()));
    let bytes = serde_json::to_vec(&json!({"version": 1, "key": host.key(),
        "host": "named", "hostSession": "worker", "started": now, "lastSeen": now}))
    .unwrap();
    fs::write(&record, &bytes).unwrap();

    for force in [false, true] {
        let error = tokio::time::timeout(
            Duration::from_secs(1),
            update_using(&settings, false, force, None, None, false, temporary_client),
        )
        .await
        .unwrap()
        .unwrap_err();
        assert_eq!(error.code, Code::Busy);
        assert_eq!(error.error, "worker is active in the helper.");
    }
    let error = tokio::time::timeout(
        Duration::from_secs(1),
        update_finish_using(&settings, None, false, temporary_client),
    )
    .await
    .unwrap()
    .unwrap_err();
    assert_eq!(error.code, Code::Busy);
    assert_eq!(error.error, "worker is active in the helper.");
    assert_eq!(fs::read(&record).unwrap(), bytes);
}

#[test]
fn setup_detection_propagates_broken_configuration_paths() {
    use std::{fs, os::unix::fs::symlink};

    let dir = tempfile::tempdir().unwrap();
    for missing in ["missing.toml", "missing/settings.toml"] {
        assert!(set_up(&dir.path().join(missing)).unwrap().is_none());
    }
    let valid = dir.path().join("valid.toml");
    fs::write(
        &valid,
        include_str!("../../../../../../examples/config.toml"),
    )
    .unwrap();
    assert_eq!(
        set_up(&valid).unwrap().unwrap(),
        Config::load(&valid).unwrap()
    );

    let malformed = dir.path().join("malformed.toml");
    fs::write(&malformed, "helper_uid = [").unwrap();
    let dangling = dir.path().join("dangling.toml");
    symlink(dir.path().join("missing.toml"), &dangling).unwrap();
    let looping = dir.path().join("loop");
    symlink(&looping, &looping).unwrap();
    assert_eq!(fs::read_link(&looping).unwrap(), looping);
    for broken in [malformed, dangling, looping.join("settings.toml")] {
        assert_eq!(set_up(&broken).unwrap_err().code, "config-invalid");
    }
}

/// Only missing settings mean "not set up"; a broken file is an error.
#[tokio::test]
async fn finishing_an_update_needs_settings_that_load() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("config.toml");
    let finished = update_finish(&missing, None, false).await.unwrap();
    assert_eq!(finished, json!({"ok": true, "next": "atlas init"}));

    let broken = dir.path().join("broken.toml");
    std::fs::write(&broken, "helper_uid = [").unwrap();
    let error = update_finish(&broken, None, false).await.unwrap_err();
    assert_eq!(error.code, "config-invalid", "{}", error.error);
}

/// Settings written before setup installed the helper's job leave no
/// daemon to move: the update finishes, and atlas init does the rest.
#[tokio::test]
async fn finishing_an_update_skips_a_helper_with_no_job() {
    let dir = tempfile::tempdir().unwrap();
    let settings = dir.path().join("config.toml");
    std::fs::write(
        &settings,
        "helper_uid = 60001\nsocket_path = \"helper/missing.sock\"\n\
             identity_probe = \"atlas\"\ninput_socket = \"input.sock\"\n\
             output_dir = \"runs\"\n",
    )
    .unwrap();

    let finished = update_finish_using(&settings, None, false, temporary_client)
        .await
        .unwrap();
    assert_eq!(
        finished,
        json!({"ok": true, "completedSteps": [], "next": "atlas init"})
    );

    assert!(has_helper_job(true, false), "a recorded setup");
    assert!(has_helper_job(false, true), "a daemon that answers");
    assert!(!has_helper_job(false, false));
}

/// What a child that exited with `code` wrote.
fn exited(code: i32, stdout: &str, stderr: &str) -> std::process::Output {
    use std::os::unix::process::ExitStatusExt;
    std::process::Output {
        status: std::process::ExitStatus::from_raw(code << 8),
        stdout: stdout.into(),
        stderr: stderr.into(),
    }
}

#[test]
fn an_update_stops_when_a_child_does_not_acknowledge_success() {
    const OK: &str = r#"{"ok":true}"#;
    let unacknowledged = || {
        [
            exited(0, "not json", ""),
            exited(0, "", ""),
            exited(0, "null", ""),
            exited(0, "true", ""),
            exited(0, "[true]", ""),
            exited(0, "{}", ""),
            exited(0, r#"{"ok":"true"}"#, ""),
            exited(0, r#"{"ok":1}"#, ""),
            exited(0, r#"{"ok":false,"error":"refused"}"#, ""),
            exited(1, OK, ""),
            exited(1, "", OK),
        ]
    };
    let steps: [&[&str]; 3] = [&["install"], &["restart"], &["start-input"]];
    for second in unacknowledged() {
        let shown = format!("{second:?}");
        let mut ran = Vec::new();
        let mut replies = [exited(0, OK, ""), second].into_iter();
        let failed = run_steps(&steps, |step| {
            ran.push(step.join(" "));
            Ok(replies.next().unwrap_or_else(|| exited(0, OK, "")))
        })
        .unwrap()
        .expect_err(&shown);
        assert_eq!(ran, ["install", "restart"], "no next step: {shown}");
        assert_eq!(failed["ok"], false, "{shown}");
        assert_eq!(failed["failedStep"], "restart", "{shown}");
        assert_eq!(failed["completedSteps"], json!(["install"]), "{shown}");
        assert_eq!(failed["failure"]["ok"], false, "{shown}");
        assert!(failed["failure"]["error"].is_string(), "{shown}");
    }
    let refused = run_steps(&steps, |_| {
        Ok(exited(3, r#"{"ok":false,"error":"refused"}"#, ""))
    })
    .unwrap()
    .unwrap_err();
    assert_eq!(
        refused["failure"],
        json!({"ok": false, "error": "refused"}),
        "a child's own failure is kept"
    );
    let done = run_steps(&steps, |_| Ok(exited(0, OK, ""))).unwrap();
    assert_eq!(done.unwrap(), ["install", "restart", "start-input"]);

    for output in unacknowledged() {
        let (moved, report) = helper_moved(&output);
        assert!(!moved, "{output:?}");
        assert_eq!(report["ok"], false, "{output:?}");
    }
    assert!(helper_moved(&exited(0, OK, "")).0);

    // Replies go to stdout, and errors to stderr when stdout says nothing.
    let both = exited(0, OK, r#"{"ok":false}"#);
    assert_eq!(child_reply(&both).unwrap(), json!({"ok": true}));
    let stderr = exited(1, "", r#"{"ok":false,"error":"refused"}"#);
    assert_eq!(child_reply(&stderr).unwrap()["error"], "refused");
}

#[test]
fn an_update_retains_the_previous_app_while_menu_bar_shutdown_is_unconfirmed() {
    let mut report = json!({"ok": true});
    let error = retire_previous(
        &mut report,
        || {
            Err(Error::new(
                Code::Menubar,
                "The menu bar did not stop within a minute",
            ))
        },
        || panic!("the previous app was removed"),
    )
    .unwrap_err();
    assert_eq!(error.code, Code::Menubar);
    assert_eq!(
        error.error,
        "The menu bar did not stop within a minute, so the previous Atlas.app was kept. Quit the \
         menu bar, then run atlas update again."
    );
    assert!(report.get("previous").is_none(), "{report}");

    for running in [Some(8123), None] {
        let mut report = json!({"ok": true});
        let port =
            retire_previous(&mut report, || Ok(running), || Ok(json!({"removed": true}))).unwrap();
        assert_eq!(port, running, "restarted only where it ran");
        assert_eq!(report["previous"], json!({"removed": true}));
    }
}

#[test]
fn an_update_does_not_take_unloadable_settings_for_no_menu_bar() {
    use atlas_core::test_support::{TestChild, message, permitted};
    if std::env::var_os("ATLAS_ANOTHER_ATLAS").is_some() {
        message("running");
        permitted("done");
        return;
    }

    let dir = tempfile::tempdir().unwrap();
    let broken = dir.path().join("broken.toml");
    std::fs::write(&broken, "helper_uid = [").unwrap();

    // Another process of this executable may be the menu bar.
    let mut other = TestChild::spawn(
        std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "update::tests::an_update_does_not_take_unloadable_settings_for_no_menu_bar",
                "--nocapture",
            ])
            .env("ATLAS_ANOTHER_ATLAS", "1"),
    );
    other.expect("running");
    let running = stop_menubar(&broken);
    other.permit("done");
    assert!(other.wait().success());
    let error = running.expect_err("where the menu bar keeps its records is unknown");
    assert_eq!(error.code, Code::Menubar);
    assert_eq!(
        error.error,
        format!(
            "Atlas can't find the menu bar's record while Atlas runs as process {}",
            other.process.id()
        )
    );

    assert_eq!(
        stop_menubar(&broken).unwrap(),
        None,
        "no other Atlas process runs"
    );
}

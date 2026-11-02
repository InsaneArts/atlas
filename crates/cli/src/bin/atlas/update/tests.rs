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
    use atlas_core::{host::Host, process::Instance};

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

    let holder = std::fs::read_dir(directory.path().join("state"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| path.to_string_lossy().ends_with(".holder.json"))
        .unwrap();
    let waiting = holder.with_file_name(
        holder
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .replace(".holder.json", ".waiting"),
    );
    std::fs::create_dir(&waiting).unwrap();
    let worker = HostSession::new(Host::Named, "worker").unwrap();
    let note = atlas_control::Note {
        session: Some(worker.key().clone()),
        host: Some(Host::Named),
        folder: None,
        named: Some(worker.id().clone()),
        process: Instance::current().unwrap(),
        since: u64::try_from(atlas_core::time::timestamp() / 1_000_000).unwrap(),
    };
    std::fs::write(
        waiting.join("0000000000000001.json"),
        serde_json::to_vec(&note).unwrap(),
    )
    .unwrap();

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

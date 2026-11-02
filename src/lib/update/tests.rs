use super::*;
use crate::app_bundle::app_of;

async fn swap(
    new: &Path,
    installed: &Path,
    name: &str,
    record: &Path,
    preflight: impl AsyncFnOnce(&Path) -> Result<()>,
) -> Result<()> {
    swap_using(new, installed, name, record, preflight, stage).await
}

/// An owned, reachable stand-in bundle and private update paths. No test
/// runs the stand-in executable or resolves the account's real paths.
fn installation(directory: &Path) -> (PathBuf, PathBuf, PathBuf) {
    use std::os::unix::fs::PermissionsExt;

    let installed = directory.join("Applications/Atlas.app");
    let executable = installed.join(EXECUTABLE);
    fs::create_dir_all(executable.parent().unwrap()).unwrap();
    fs::write(&executable, b"installed app stays").unwrap();
    for path in executable
        .ancestors()
        .take_while(|path| path.starts_with(directory))
    {
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }
    assert!(reachable_by_others(&executable));
    (
        installed,
        directory.join("cache"),
        directory.join("update.json"),
    )
}

#[tokio::test]
async fn unchanged_updates_need_no_archive_and_clean_their_scratch() {
    for available in [crate::VERSION, "0.0.0"] {
        let dir = tempfile::Builder::new().tempdir_in("/tmp").unwrap();
        let (installed, cache, record) = installation(dir.path());
        let _exclusive = exclusive_in(&cache).unwrap();
        let source = Source {
            base: format!("file://{}", dir.path().display()),
            latest: false,
        };
        let name = format!("Atlas-{available}-macos.zip");
        fs::write(
            dir.path().join("SHA256SUMS"),
            format!("{}  {name}\n", "0".repeat(64)),
        )
        .unwrap();
        fs::create_dir_all(cache.join("update")).unwrap();
        fs::write(cache.join("update/stale"), "previous attempt").unwrap();

        let result = install_in(
            &source,
            false,
            false,
            &installed,
            &cache,
            &record,
            async |_| panic!("a no-op must not preflight or swap"),
        )
        .await
        .unwrap();

        assert_eq!(
            result,
            json!({"ok": true, "updated": false, "version": crate::VERSION, "available": name})
        );
        assert_eq!(
            fs::read(installed.join(EXECUTABLE)).unwrap(),
            b"installed app stays"
        );
        assert!(!cache.join("update").exists());
        assert!(!record.exists());
        assert!(
            !dir.path().join(name).exists(),
            "the unused archive is absent"
        );
    }
}

#[tokio::test]
async fn newer_and_forced_updates_still_download_and_verify_the_archive() {
    let newer = format!("{}.0.0", version(crate::VERSION)[0] + 1);
    for (available, force) in [
        (newer.as_str(), false),
        (crate::VERSION, true),
        ("0.0.0", true),
    ] {
        let dir = tempfile::Builder::new().tempdir_in("/tmp").unwrap();
        let (installed, cache, record) = installation(dir.path());
        let _exclusive = exclusive_in(&cache).unwrap();
        let source = Source {
            base: format!("file://{}", dir.path().display()),
            latest: false,
        };
        let name = format!("Atlas-{available}-macos.zip");
        let archive = dir.path().join(&name);
        let sums = dir.path().join("SHA256SUMS");
        fs::write(&sums, format!("{}  {name}\n", "0".repeat(64))).unwrap();

        for expected in [
            "Could not download",
            "Checksum mismatch",
            "could not be unpacked",
        ] {
            let error = install_in(
                &source,
                false,
                force,
                &installed,
                &cache,
                &record,
                async |_| panic!("unverified archives must not preflight or swap"),
            )
            .await
            .unwrap_err();
            assert!(error.error.contains(expected), "{}", error.error);
            assert!(!cache.join("update").exists());
            assert!(!record.exists());
            assert_eq!(
                fs::read(installed.join(EXECUTABLE)).unwrap(),
                b"installed app stays"
            );

            // First supply an archive, then its correct checksum: neither
            // missing bytes nor corrupt bytes can reach installation.
            if !archive.exists() {
                fs::write(&archive, b"not a zip archive").unwrap();
            } else {
                let sum = crate::digest::file_sha256(&archive).unwrap();
                fs::write(&sums, format!("{sum}  {name}\n")).unwrap();
            }
        }
    }
}

#[tokio::test]
async fn invalid_release_metadata_never_reaches_installation() {
    let dir = tempfile::Builder::new().tempdir_in("/tmp").unwrap();
    let (installed, cache, record) = installation(dir.path());
    let _exclusive = exclusive_in(&cache).unwrap();
    let source = Source {
        base: format!("file://{}", dir.path().display()),
        latest: false,
    };
    for metadata in [
        "",
        "00  ../Atlas-0.0.0-macos.zip\n",
        "00  Atlas-bad-macos.zip\n",
    ] {
        fs::write(dir.path().join("SHA256SUMS"), metadata).unwrap();
        let error = install_in(
            &source,
            false,
            false,
            &installed,
            &cache,
            &record,
            async |_| panic!("invalid metadata must not preflight or swap"),
        )
        .await
        .unwrap_err();
        assert_eq!(error.error, "SHA256SUMS does not name an Atlas archive");
        assert!(!cache.join("update").exists());
        assert!(!record.exists());
        assert_eq!(
            fs::read(installed.join(EXECUTABLE)).unwrap(),
            b"installed app stays"
        );
    }
}

#[tokio::test]
async fn an_unfinished_handover_precedes_downloads_and_scratch_cleanup() {
    let dir = tempfile::Builder::new().tempdir_in("/tmp").unwrap();
    let (installed, cache, record_path) = installation(dir.path());
    let _exclusive = exclusive_in(&cache).unwrap();
    let source = Source {
        base: format!("file://{}/absent", dir.path().display()),
        latest: false,
    };
    let record = json!({"installed": installed, "to": "Atlas-9.9.9-macos.zip"});
    fs::write(&record_path, record.to_string()).unwrap();
    fs::create_dir_all(cache.join("update")).unwrap();
    fs::write(cache.join("update/keep"), "unfinished").unwrap();
    let mut checked = false;
    let result = install_in(
        &source,
        false,
        false,
        &installed,
        &cache,
        &record_path,
        async |program| {
            assert_eq!(program, installed.join(EXECUTABLE));
            checked = true;
            Ok(())
        },
    )
    .await
    .unwrap();
    assert!(checked);
    assert_eq!(
        result,
        json!({"ok": true, "updated": true, "resumed": true, "record": record})
    );
    assert_eq!(
        fs::read_to_string(cache.join("update/keep")).unwrap(),
        "unfinished"
    );
    assert_eq!(
        read_record(&record_path).unwrap().map(|kept| kept.written),
        Some(record)
    );
    assert_eq!(
        fs::read(installed.join(EXECUTABLE)).unwrap(),
        b"installed app stays"
    );
}

#[tokio::test]
async fn a_waiter_arriving_during_staging_prevents_recording_and_exchange_under_ownership() {
    use crate::host_session::{Host, HostSession};
    use std::time::Duration;

    for waiter in [None, Some("worker"), Some("updater")] {
        let directory = tempfile::tempdir().unwrap();
        let installed = directory.path().join("Applications/Atlas.app");
        let new = directory.path().join("unpacked/Atlas.app");
        let record = directory.path().join("Support/update.json");
        for (app, version) in [(&installed, "old"), (&new, "new")] {
            fs::create_dir_all(app.join("Contents/MacOS")).unwrap();
            fs::write(app.join(EXECUTABLE), version).unwrap();
        }
        let mut client = crate::test_support::client(directory.path());
        client.host_session = Some(HostSession::new(Host::Named, "updater").unwrap());
        client.controller = client.controller.labelled(client.host_session.as_ref());
        let ownership = client.service_change(false).await.unwrap();
        let worker = waiter.map(|name| HostSession::new(Host::Named, name).unwrap());
        let mut checked = false;
        let result = tokio::time::timeout(
            Duration::from_secs(1),
            swap_using(
                &new,
                &installed,
                "Atlas-9.9.9-macos.zip",
                &record,
                async |program| {
                    checked = true;
                    assert_eq!(fs::read_to_string(program).unwrap(), "new");
                    assert_eq!(program, copies(&installed, update_copy)[0].join(EXECUTABLE));
                    assert!(!record.exists());
                    assert_eq!(
                        client.controller.acquire(false).unwrap_err().code,
                        Code::Busy
                    );
                    client.require_service_idle().await
                },
                |new, staged| {
                    stage(new, staged)?;
                    crate::test_support::waiting_note(
                        &directory.path().join("state"),
                        worker.as_ref(),
                    );
                    Ok(())
                },
            ),
        )
        .await
        .unwrap();
        let error = result.unwrap_err();
        assert!(checked);
        assert_eq!(error.code, Code::Busy);
        assert_eq!(
            error.error,
            format!("{} is active in the helper.", waiter.unwrap_or("Terminal"))
        );
        assert_eq!(
            fs::read_to_string(installed.join(EXECUTABLE)).unwrap(),
            "old"
        );
        assert!(!record.exists());
        assert!(copies(&installed, update_copy).is_empty());
        assert!(
            client
                .controller
                .holder_note()
                .unwrap()
                .process
                .is_this_one()
        );
        assert_eq!(
            client.controller.acquire(false).unwrap_err().code,
            Code::Busy
        );
        drop(ownership);
    }
}

#[tokio::test]
async fn setup_refusal_prevents_the_bundle_exchange_and_update_record() {
    let dir = tempfile::tempdir().unwrap();
    let installed = dir.path().join("Applications/Atlas.app");
    let new = dir.path().join("unpacked/Atlas.app");
    let record = dir.path().join("Support/update.json");
    for (app, version) in [(&installed, "old"), (&new, "new")] {
        fs::create_dir_all(app.join("Contents/MacOS")).unwrap();
        fs::write(app.join(EXECUTABLE), version).unwrap();
    }
    let result = swap(
        &new,
        &installed,
        "Atlas-9.9.9-macos.zip",
        &record,
        async |program| {
            assert_ne!(program, new.join(EXECUTABLE));
            assert_eq!(fs::read_to_string(program).unwrap(), "new");
            assert_eq!(program, copies(&installed, update_copy)[0].join(EXECUTABLE));
            assert_eq!(
                fs::read_to_string(installed.join(EXECUTABLE)).unwrap(),
                "old"
            );
            Err(problem("setup refuses"))
        },
    )
    .await;
    assert_eq!(result.unwrap_err().error, "setup refuses");
    assert_eq!(
        fs::read_to_string(installed.join(EXECUTABLE)).unwrap(),
        "old"
    );
    assert_eq!(fs::read_to_string(new.join(EXECUTABLE)).unwrap(), "new");
    assert!(!record.exists());
    assert!(copies(&installed, update_copy).is_empty());
}

#[tokio::test]
async fn private_staged_apps_are_refused_before_recording_or_replacing_the_installation() {
    use std::os::unix::fs::PermissionsExt;

    for (relative, mode) in [
        ("", 0o700),
        ("Contents", 0o700),
        ("Contents/MacOS", 0o700),
        (EXECUTABLE, 0o700),
        (EXECUTABLE, 0o744),
        (EXECUTABLE, 0o711),
    ] {
        let directory = tempfile::Builder::new().tempdir_in("/tmp").unwrap();
        let (installed, _, record) = installation(directory.path());
        let original = fs::read(installed.join(EXECUTABLE)).unwrap();
        let new = directory.path().join("unpacked/Atlas.app");
        let executable = new.join(EXECUTABLE);
        fs::create_dir_all(executable.parent().unwrap()).unwrap();
        fs::write(&executable, b"new candidate").unwrap();
        for path in executable
            .ancestors()
            .take_while(|path| path.starts_with(&new))
        {
            fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
        }
        fs::set_permissions(new.join(relative), fs::Permissions::from_mode(mode)).unwrap();
        let mut checked = false;
        let error = swap(
            &new,
            &installed,
            "Atlas-9.9.9-macos.zip",
            &record,
            async |program| {
                checked = true;
                assert_eq!(fs::read(program).unwrap(), b"new candidate");
                assert!(!record.exists());
                Ok(())
            },
        )
        .await
        .unwrap_err();
        assert!(checked, "existing preflight checks still run first");
        assert_eq!(error.code, Code::Update);
        assert_eq!(fs::read(installed.join(EXECUTABLE)).unwrap(), original);
        assert!(!record.exists());
        assert!(copies(&installed, update_copy).is_empty());
        assert_eq!(fs::read(executable).unwrap(), b"new candidate");
    }
}

#[tokio::test]
async fn a_recorded_private_candidate_is_retained_without_exchange_on_resume() {
    let directory = tempfile::Builder::new().tempdir_in("/tmp").unwrap();
    let (installed, _, path) = installation(directory.path());
    let previous = installed
        .parent()
        .unwrap()
        .join(format!("{STAGED}{UPDATE}{}", crate::files::token()));
    fs::create_dir_all(previous.join("Contents/MacOS")).unwrap();
    fs::write(previous.join(EXECUTABLE), b"private candidate").unwrap();
    let record = json!({
        "installed": installed,
        "previous": previous,
        "executable": crate::digest::file_sha256(&previous.join(EXECUTABLE)).unwrap(),
    });
    let bytes = serde_json::to_vec(&record).unwrap();
    crate::files::save_private(&path, &bytes).unwrap();
    let mut checked = false;
    let error = resume(&record.into(), &installed, async |program| {
        checked = true;
        assert_eq!(program, previous.join(EXECUTABLE));
        Ok(())
    })
    .await
    .unwrap_err();
    assert!(checked);
    assert_eq!(error.code, Code::Update);
    assert!(!error.uncertain);
    assert_eq!(fs::read(&path).unwrap(), bytes);
    assert_eq!(
        fs::read(installed.join(EXECUTABLE)).unwrap(),
        b"installed app stays"
    );
    assert_eq!(
        fs::read(previous.join(EXECUTABLE)).unwrap(),
        b"private candidate"
    );
}

#[tokio::test]
async fn a_missing_staged_executable_keeps_its_private_io_cause_without_replacement() {
    use std::error::Error as _;

    let directory = tempfile::Builder::new().tempdir_in("/tmp").unwrap();
    let (installed, _, record) = installation(directory.path());
    let new = directory.path().join("unpacked/Atlas.app");
    fs::create_dir_all(new.join("Contents/MacOS")).unwrap();
    fs::write(new.join(EXECUTABLE), b"candidate").unwrap();
    let error = swap_using(
        &new,
        &installed,
        "Atlas-9.9.9-macos.zip",
        &record,
        async |_| Ok(()),
        |new, staged| {
            stage(new, staged)?;
            fs::remove_file(staged.join(EXECUTABLE))?;
            Ok(())
        },
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, Code::Update);
    assert!(!error.uncertain);
    assert_eq!(
        error
            .source()
            .unwrap()
            .downcast_ref::<std::io::Error>()
            .unwrap()
            .kind(),
        std::io::ErrorKind::NotFound
    );
    assert!(
        !serde_json::to_string(&error)
            .unwrap()
            .contains(directory.path().to_str().unwrap())
    );
    assert!(!record.exists());
    assert!(copies(&installed, update_copy).is_empty());
    assert_eq!(
        fs::read(installed.join(EXECUTABLE)).unwrap(),
        b"installed app stays"
    );
}

#[test]
fn a_check_reads_the_newest_release_and_installs_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let next = format!("Atlas-{}.0.0-macos.zip", version(crate::VERSION)[0] + 1);
    fs::write(
        dir.path().join("SHA256SUMS"),
        format!("{} {next}\n", "0".repeat(64)),
    )
    .unwrap();
    let source = Source {
        base: format!("file://{}", dir.path().display()),
        latest: true,
    };

    let caches = tempfile::tempdir().unwrap();
    let report = newer_in(&source, caches.path()).unwrap();
    assert_eq!(report["newer"], true);
    assert_eq!(report["version"], crate::VERSION);
    assert_eq!(
        fs::read_dir(dir.path()).unwrap().count(),
        1,
        "only the sums were there to read"
    );

    let current = format!("Atlas-{}-macos.zip", crate::VERSION);
    fs::write(
        dir.path().join("SHA256SUMS"),
        format!("{} {current}\n", "0".repeat(64)),
    )
    .unwrap();
    assert_eq!(newer_in(&source, caches.path()).unwrap()["newer"], false);
}

#[test]
fn a_check_stopped_midway_leaves_nothing_for_long() {
    let caches = tempfile::tempdir().unwrap();
    let stale = caches.path().join("check-999999");
    let other = caches.path().join("update");
    fs::create_dir_all(&stale).unwrap();
    fs::create_dir_all(&other).unwrap();

    sweep_checks(caches.path());

    assert!(!stale.exists(), "no process 999999 runs");
    assert!(other.exists(), "an update's scratch is not a check's");
}

#[test]
fn archives_name_their_version() {
    assert_eq!(
        archive_version("Atlas-0.10.2-macos.zip"),
        Some(vec![0, 10, 2])
    );
    assert_eq!(archive_version("Atlas-x-macos.zip"), None);
    assert_eq!(archive_version("SHA256SUMS"), None);
    assert!(archive_version("Atlas-0.10.0-macos.zip").unwrap() > version("0.9.9"));
}

#[test]
fn an_app_must_be_the_version_its_archive_is_named_for() {
    let dir = tempfile::tempdir().unwrap();
    let app = dir.path().join("Atlas.app");
    fs::create_dir_all(app.join("Contents")).unwrap();
    let name = "Atlas-1.2.3-macos.zip";
    assert!(require_named_version(&app, name).is_err(), "no Info.plist");

    for (version, matches) in [("1.2.3", true), ("1.2.4", false), ("1.2.3-beta", false)] {
        fs::write(
            app.join("Contents/Info.plist"),
            format!(
                "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<plist version=\"1.0\"><dict>\
                     <key>CFBundleShortVersionString</key><string>{version}</string>\
                     </dict></plist>\n"
            ),
        )
        .unwrap();
        assert_eq!(
            require_named_version(&app, name).is_ok(),
            matches,
            "{version}"
        );
    }
}

#[test]
fn releases_come_from_the_website() {
    assert_eq!(
        Source::website(None),
        Source {
            base: "https://atlas.insanearts.io/releases/latest".into(),
            latest: true,
        }
    );
    assert_eq!(
        Source::website(Some("0.2.0")),
        Source {
            base: "https://atlas.insanearts.io/releases/0.2.0".into(),
            latest: false,
        }
    );
    assert!(INSTALL_COMMAND.contains("https://atlas.insanearts.io/install.sh"));
}

#[test]
fn a_missing_newest_release_is_not_yet_published() {
    // curl says (22) over HTTP/1.1 and (56) over HTTP/2; the words are the same.
    for code in ["22", "56"] {
        let missing = || {
            problem(format!(
                "Could not download x: curl: ({code}) The requested URL returned error: 404"
            ))
        };
        assert_eq!(unpublished(missing(), true).error, NO_RELEASE);
        assert_ne!(unpublished(missing(), false).error, NO_RELEASE);
    }

    let offline = problem("Could not download x: curl: (6) Could not resolve host");
    assert_ne!(unpublished(offline, true).error, NO_RELEASE);
}

#[test]
fn only_an_app_executable_belongs_to_an_app() {
    assert_eq!(
        app_of(Path::new("/Applications/Atlas.app/Contents/MacOS/atlas")),
        Some(PathBuf::from("/Applications/Atlas.app"))
    );
    assert_eq!(app_of(Path::new("/usr/local/bin/atlas")), None);
    assert_eq!(app_of(Path::new("/x/Contents/MacOS/atlas")), None);
}

#[test]
fn an_exchange_swaps_both_apps_in_one_step() {
    let dir = tempfile::tempdir().unwrap();
    let installed = dir.path().join("Atlas.app");
    let new = dir.path().join(".Atlas.app.new");

    fs::create_dir_all(&installed).unwrap();
    fs::write(installed.join("version"), "old").unwrap();
    fs::create_dir_all(&new).unwrap();
    fs::write(new.join("version"), "new").unwrap();

    exchange(&new, &installed).unwrap();

    assert_eq!(
        fs::read_to_string(installed.join("version")).unwrap(),
        "new"
    );
    assert_eq!(
        fs::read_to_string(new.join("version")).unwrap(),
        "old",
        "kept"
    );

    assert!(exchange(&dir.path().join("missing"), &installed).is_err());
    assert_eq!(
        fs::read_to_string(installed.join("version")).unwrap(),
        "new"
    );
}

#[tokio::test]
async fn a_swap_is_recorded_first_and_finishing_removes_only_the_staged_copy() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::Builder::new().tempdir_in("/tmp").unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    let installed = root.join("Applications/Atlas.app");
    let new = root.join("unpacked/Atlas.app");
    for (app, version) in [(&installed, "old"), (&new, "new")] {
        fs::create_dir_all(app.join("Contents/MacOS")).unwrap();
        fs::write(app.join("version"), version).unwrap();
        fs::write(app.join(EXECUTABLE), version).unwrap();
        for path in app
            .join(EXECUTABLE)
            .ancestors()
            .take_while(|path| path.starts_with(&root))
        {
            fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
        }
    }

    // A staging copy a stopped update left behind is swept first; a
    // person's own folders beside the app, and install.sh's, stay.
    let stale = root.join(format!(
        "Applications/.Atlas.app.update-{}",
        crate::files::token()
    ));
    let kept = [
        ".Atlas.app.feature",
        ".Atlas.app.previous",
        ".Atlas.app.install-42",
    ]
    .map(|name| root.join("Applications").join(name));
    for folder in std::iter::once(&stale).chain(&kept) {
        fs::create_dir_all(folder).unwrap();
    }

    let record = root.join("Support/update.json");
    swap(
        &new,
        &installed,
        "Atlas-9.9.9-macos.zip",
        &record,
        async |_| Ok(()),
    )
    .await
    .unwrap();
    assert!(!stale.exists());
    for folder in &kept {
        assert!(folder.exists(), "{}", folder.display());
    }
    assert_eq!(
        fs::read_to_string(installed.join("version")).unwrap(),
        "new"
    );

    let plan = read_record(&record).unwrap().unwrap();
    let previous = plan.previous.clone().unwrap();
    assert_eq!(previous.parent(), installed.parent());
    assert!(
        previous
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with(".Atlas.app.update-")),
        "{}",
        previous.display()
    );
    assert_eq!(copies(&installed, update_copy), vec![previous.clone()]);
    assert_eq!(fs::read_to_string(previous.join("version")).unwrap(), "old");
    assert_eq!(
        plan.written["executable"],
        crate::digest::file_sha256(&installed.join(EXECUTABLE)).unwrap()
    );

    // The exchange happened: resuming changes nothing.
    resume(&plan, &installed, async |_| Ok(())).await.unwrap();
    assert_eq!(
        fs::read_to_string(installed.join("version")).unwrap(),
        "new"
    );

    // Stopped between the record and the exchange: resuming exchanges.
    exchange(&previous, &installed).unwrap();
    assert_eq!(
        fs::read_to_string(installed.join("version")).unwrap(),
        "old"
    );

    let refused = resume(&plan, &installed, async |program| {
        assert_eq!(program, previous.join(EXECUTABLE));
        Err(problem("setup refuses"))
    })
    .await;
    assert_eq!(refused.unwrap_err().error, "setup refuses");
    assert_eq!(
        fs::read_to_string(installed.join("version")).unwrap(),
        "old"
    );
    assert_eq!(fs::read_to_string(previous.join("version")).unwrap(), "new");
    resume(&plan, &installed, async |_| Ok(())).await.unwrap();
    assert_eq!(
        fs::read_to_string(installed.join("version")).unwrap(),
        "new"
    );
    assert_eq!(fs::read_to_string(previous.join("version")).unwrap(), "old");

    let report = finish_record(&record).unwrap();
    assert_eq!(report["removed"], true);
    assert!(!previous.exists() && !record.exists());
    assert_eq!(finish_record(&record).unwrap()["previous"], Value::Null);

    // A swap that cannot exchange leaves no record and no staged copy.
    let missing = root.join("Elsewhere/Atlas.app");
    fs::create_dir_all(missing.parent().unwrap()).unwrap();
    fs::set_permissions(missing.parent().unwrap(), fs::Permissions::from_mode(0o755)).unwrap();
    assert!(
        swap(&new, &missing, "x", &record, async |_| Ok(()))
            .await
            .is_err()
    );
    assert!(!record.exists());
    assert_eq!(fs::read_dir(missing.parent().unwrap()).unwrap().count(), 0);
}

#[test]
fn finishing_never_removes_what_is_not_a_staged_copy_beside_the_app() {
    let dir = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    let installed = root.join("Applications/Atlas.app");
    let record = root.join("update.json");

    let token = crate::files::token();
    for previous in [
        root.join("Documents"),
        root.join("Applications/Other.app"),
        root.join("Applications/.Atlas.app.previous"),
        root.join("Applications/.Atlas.app.feature"),
        root.join("Applications/.Atlas.app.install-42"),
        root.join(format!("Elsewhere/.Atlas.app.update-{token}")),
    ] {
        fs::create_dir_all(&previous).unwrap();
        let plan = json!({"previous": previous, "installed": installed, "to": "x"});
        fs::write(&record, plan.to_string()).unwrap();
        assert_eq!(finish_record(&record).unwrap()["removed"], false);
        assert!(previous.exists(), "{}", previous.display());
        assert!(!record.exists());
    }

    let staged = root.join(format!("Applications/.Atlas.app.update-{token}"));
    assert!(beside(&staged, Some(&installed), update_copy));
    assert!(!beside(&staged, None, update_copy));

    for name in [
        ".Atlas.app.",
        &format!(".Atlas.app.{token}"),
        ".Atlas.app.update-",
        ".Atlas.app.update-1",
        ".Atlas.app.1",
        ".Atlas.app.previous",
        &format!(".Atlas.app.update-{}", token.to_uppercase()),
        &format!(".Atlas.app.update-{token}0"),
    ] {
        let path = root.join("Applications").join(name);
        assert!(!beside(&path, Some(&installed), update_copy), "{name}");
    }
}

/// Uninstall clears every copy Atlas staged, by update or by
/// install.sh, and nothing else beside the app.
#[test]
fn only_copies_atlas_staged_are_its_own() {
    let dir = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    let installed = root.join("Atlas.app");
    let token = crate::files::token();

    let ours = [
        format!(".Atlas.app.update-{token}"),
        ".Atlas.app.install-123".to_owned(),
        ".Atlas.app.install-previous-123".to_owned(),
    ];
    let theirs = [
        ".Atlas.app.feature",
        ".Atlas.app.previous",
        ".Atlas.app.previous.123",
        ".Atlas.app.123",
        ".Atlas.app.install-",
        ".Atlas.app.install-12x",
        ".Atlas.app.install-previous-",
        "Atlas.app.update-1",
    ];
    for name in ours.iter().map(String::as_str).chain(theirs) {
        fs::create_dir_all(root.join(name)).unwrap();
    }

    let mut found = atlas_copies(&installed);
    found.sort();
    let mut expected: Vec<PathBuf> = ours.iter().map(|name| root.join(name)).collect();
    expected.sort();
    assert_eq!(found, expected);
    assert_eq!(
        copies(&installed, update_copy).len(),
        1,
        "update sweeps only its own"
    );
}

/// Two updates never run at once on one account.
#[test]
fn one_update_runs_at_a_time() {
    let dir = tempfile::tempdir().unwrap();
    let first = exclusive_in(dir.path()).unwrap();
    let error = exclusive_in(dir.path()).unwrap_err();
    assert!(
        error.error.contains("Another atlas update"),
        "{}",
        error.error
    );

    drop(first);
    // A process another test starts at this moment may hold a copy of
    // the lock's file for an instant before it runs its program.
    let again = (0..50).find_map(|_| {
        exclusive_in(dir.path()).ok().or_else(|| {
            std::thread::sleep(std::time::Duration::from_millis(20));
            None
        })
    });
    assert!(
        again.is_some(),
        "the lock is free once the first is dropped"
    );
}

/// Releases come over https or from a folder; plain http only from
/// this Mac, as a rehearsal serves them.
#[test]
fn releases_come_only_over_https_a_folder_or_this_mac() {
    for (url, allowed) in [
        (
            "https://atlas.insanearts.io/releases/latest/SHA256SUMS",
            Some("=https"),
        ),
        ("file:///tmp/releases/latest/SHA256SUMS", Some("=file")),
        (
            "http://127.0.0.1:8787/releases/latest/SHA256SUMS",
            Some("=http"),
        ),
        ("http://localhost:8787/releases/latest", Some("=http")),
        ("http://localhost/releases", Some("=http")),
        ("http://127.0.0.1", Some("=http")),
        ("http://example.com/releases", None),
        ("http://127.0.0.1.example.com/releases", None),
        ("http://127.0.0.1:8787@example.com/releases", None),
        ("http://localhost@example.com/releases", None),
        ("http://127.0.0.1:/releases", None),
        ("ftp://127.0.0.1/releases", None),
    ] {
        assert_eq!(protocol(url).ok(), allowed, "{url}");
    }
}

#[test]
fn an_unreadable_record_asks_to_be_inspected() {
    let dir = tempfile::tempdir().unwrap();
    let record = dir.path().join("update.json");
    assert!(read_record(&record).unwrap().is_none());

    for corrupt in ["{\"previous\": ", "[]", ""] {
        fs::write(&record, corrupt).unwrap();
        let error = read_record(&record).unwrap_err();
        assert_eq!(error.code, "update");
        assert!(error.error.contains("Inspect it"), "{}", error.error);
        assert!(finish_record(&record).is_err());
        assert!(record.exists(), "an unreadable record is kept");
    }
}

#[test]
fn archives_must_match_their_listed_checksum() {
    let dir = tempfile::tempdir().unwrap();
    let name = "Atlas-1.0.0-macos.zip";
    fs::write(dir.path().join(name), b"archive").unwrap();
    fs::write(dir.path().join("SHA256SUMS"), "00  Other.zip\n").unwrap();
    let unlisted = verify(dir.path(), name, false).unwrap_err();
    assert!(
        unlisted.error.contains("does not list"),
        "{}",
        unlisted.error
    );

    fs::write(
        dir.path().join("SHA256SUMS"),
        format!("{}  {name}\n", "0".repeat(64)),
    )
    .unwrap();
    let mismatch = verify(dir.path(), name, false).unwrap_err();
    assert!(
        mismatch.error.contains("Checksum mismatch"),
        "{}",
        mismatch.error
    );

    let sum = crate::digest::file_sha256(&dir.path().join(name)).unwrap();
    fs::write(dir.path().join("SHA256SUMS"), format!("{sum}  {name}\n")).unwrap();
    // The right bytes get past the checksum to unpacking, which fails here.
    let unpacked = verify(dir.path(), name, false).unwrap_err();
    assert!(unpacked.error.contains("unpacked"), "{}", unpacked.error);
}

#[test]
fn downloads_report_why_they_failed() {
    let dir = tempfile::tempdir().unwrap();
    let missing = format!("file://{}/missing", dir.path().display());
    let error = curl(&missing, &dir.path().join("out")).unwrap_err();
    assert!(error.error.starts_with("Could not download"));
    assert!(error.error.contains("curl"), "{}", error.error);

    let plain = curl("http://127.0.0.1:9/x", &dir.path().join("out")).unwrap_err();
    assert!(
        plain.error.contains("Could not download"),
        "{}",
        plain.error
    );

    let source = dir.path().join("source");
    fs::write(&source, b"bytes").unwrap();
    let local = format!("file://{}", source.display());
    curl(&local, &dir.path().join("copy")).unwrap();
    assert_eq!(fs::read(dir.path().join("copy")).unwrap(), b"bytes");
}

#[test]
fn only_paths_others_can_reach_are_runnable_by_the_helper() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    let private = dir.path().join("home");
    let file = private.join("Applications/atlas");
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    fs::write(&file, b"").unwrap();
    fs::set_permissions(&file, fs::Permissions::from_mode(0o755)).unwrap();
    fs::set_permissions(&private, fs::Permissions::from_mode(0o700)).unwrap();

    assert!(
        !reachable_by_others(&file),
        "a private home blocks the helper"
    );
    assert!(reachable_by_others(Path::new("/bin/ls")));
}

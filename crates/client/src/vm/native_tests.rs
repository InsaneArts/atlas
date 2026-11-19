use super::*;
use std::os::unix::fs::PermissionsExt;

/// The session the scripted guest's proof pins, as a scan carries it back.
const EPOCH: &str = "42-7";

/// A canary folder made as the fixture's launcher makes one in the guest,
/// gone with its files when dropped.
struct Canary(PathBuf);

impl Canary {
    fn new() -> Self {
        let path = PathBuf::from(format!(
            "/private/tmp/atlas-canary.{}",
            &files::token()[..12]
        ));
        std::fs::create_dir(&path).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        Self(path)
    }

    fn fixture(&self) -> Fixture {
        Fixture {
            pid: 4242,
            program: PathBuf::from("/Users/agent/Library/Atlas/copies/fixture/DesktopFixture"),
            directory: self.0.clone(),
        }
    }
}

impl Drop for Canary {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn scan(attempt: &str, challenge: &str, epoch: &str, trusted: bool) -> Value {
    json!({"ok": true, "op": "native", "scan": {"attemptID": attempt, "challenge": challenge,
        "epoch": epoch, "trusted": trusted, "parentPID": 4242, "scanComplete": true,
        "panelExists": true, "focusInPanel": true}})
}

fn asked<'a>(challenge: &'a str) -> Asked<'a> {
    Asked {
        attempt: "8E3F2A10-4B7C-4D2E-9A61-3C5B7D9E0F12",
        challenge,
        epoch: EPOCH,
        pid: 4242,
    }
}

/// A Guest of `client`'s VM, proved as `verified`, under `ownership`.
fn guest<'a>(client: &'a Client, ownership: &'a Ownership, verified: &'a Verified) -> Guest<'a> {
    Guest {
        client,
        ownership,
        verified,
        generation: verified.machine().unwrap().generation,
        display: verified.session().epoch.display.clone(),
        epoch: atlas_core::native::epoch(&verified.pin().unwrap()),
        program: PathBuf::from("/Users/Shared/AtlasTests/DesktopFixture"),
        late: Cell::new(false),
        stopped: Cell::new(false),
    }
}

#[test]
fn a_scan_arriving_after_its_bound_on_the_stations_clock_is_not_fresh() {
    let challenge = files::token();
    let reply = scan(
        "8E3F2A10-4B7C-4D2E-9A61-3C5B7D9E0F12",
        &challenge,
        EPOCH,
        true,
    );

    let in_time = judged(&reply, &asked(&challenge), Duration::from_millis(2900));
    let late = judged(&reply, &asked(&challenge), Duration::from_millis(3100));

    assert!(matches!(in_time, Judged::Fresh(_)), "{in_time:?}");
    assert!(matches!(late, Judged::Late), "{late:?}");
}

#[test]
fn a_scan_answering_another_challenge_attempt_or_session_never_passes_for_this_one() {
    let (earlier, challenge) = (files::token(), files::token());

    for reply in [
        scan(
            "8E3F2A10-4B7C-4D2E-9A61-3C5B7D9E0F12",
            &earlier,
            EPOCH,
            true,
        ),
        scan(
            "1D4C7B2A-6E5F-4A3B-8C9D-0E1F2A3B4C5D",
            &challenge,
            EPOCH,
            true,
        ),
        scan(
            "8E3F2A10-4B7C-4D2E-9A61-3C5B7D9E0F12",
            &challenge,
            "43-7",
            true,
        ),
        json!({"ok": false, "code": "native", "late": true, "error": "late"}),
    ] {
        let judged = judged(&reply, &asked(&challenge), Duration::ZERO);
        assert!(matches!(judged, Judged::Late), "{reply}: {judged:?}");
    }
    let untrusted = judged(
        &scan(
            "8E3F2A10-4B7C-4D2E-9A61-3C5B7D9E0F12",
            &challenge,
            EPOCH,
            false,
        ),
        &asked(&challenge),
        Duration::ZERO,
    );
    assert!(matches!(untrusted, Judged::Untrusted), "{untrusted:?}");
}

fn record(epoch: MachineEpoch, observed_at: u64, passed: bool) -> Record {
    let check = |kind: &str| Check {
        kind: kind.to_owned(),
        passed,
        stage: (!passed).then(|| "save panel appears".to_owned()),
        error: (!passed).then(|| "Timed out: save panel appears".to_owned()),
    };
    Record {
        observed_at,
        passed,
        checks: vec![check("save"), check("open")],
        epoch,
        evidence: Vec::new(),
    }
}

fn epoch(boot: &str) -> MachineEpoch {
    MachineEpoch {
        uid: 502,
        boot: boot.to_owned(),
        capture_pid: 123,
        display: Display {
            width: 1920.0,
            height: 1200.0,
            scale: 1.0,
        },
        security_session_id: 42,
        display_id: 7,
        input_pid: 456,
    }
}

/// A proven VM's status, its fresh proof pinning `epoch`.
fn proven(epoch: &MachineEpoch) -> Value {
    json!({"ok": true, "proven": true, "proof": {"epoch": epoch, "capture": true,
        "input": true, "generation": 7}})
}

#[test]
fn a_vms_record_is_kept_in_its_own_records_folder_with_its_machine_epoch() {
    use std::os::unix::fs::MetadataExt;
    let records = tempfile::tempdir().unwrap();
    let kept = record(epoch("boot-1"), now(), true);

    let path = keep(records.path(), &kept).unwrap();

    assert_eq!(path, records.path().join("native.json"));
    assert_eq!(std::fs::metadata(&path).unwrap().mode() & 0o777, 0o600);
    let read: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(read["epoch"], json!(epoch("boot-1")), "{read}");
    assert_eq!(read["epoch"]["boot"], "boot-1");
    assert!(matches!(read_record(records.path()), Evidence::Present(back) if back == kept));
}

#[test]
fn doctor_reports_a_vms_native_check_passed_only_in_the_epoch_it_was_observed_and_not_checked_before_any()
 {
    let records = tempfile::tempdir().unwrap();
    let (first, restarted) = (epoch("boot-1"), epoch("boot-2"));
    let at = now();

    let before = assessment("work", records.path(), &proven(&first), at);
    keep(records.path(), &record(first.clone(), at - 60, true)).unwrap();
    let passed = assessment("work", records.path(), &proven(&first), at);
    let after_restart = assessment("work", records.path(), &proven(&restarted), at);
    let older = assessment("work", records.path(), &proven(&first), at + 31 * 60);
    let unproven = assessment(
        "work",
        records.path(),
        &json!({"ok": true, "proven": false}),
        at,
    );
    keep(records.path(), &record(first.clone(), at - 60, false)).unwrap();
    let failed = assessment("work", records.path(), &proven(&first), at);

    assert_eq!(before.0, NativeState::NotChecked);
    assert_eq!(
        before.1["reason"],
        "Native Save/Open has not been checked on work; use atlas run or atlas open instead, or \
         check it with atlas --desktop work verify-native."
    );
    assert_eq!(passed.0, NativeState::Passed, "{}", passed.1);
    assert_eq!(
        passed.1["reason"],
        "Native Save and Open completed with exact bytes in work at the recorded time, as the \
         check's files in work report it; this is not a connection check."
    );
    assert_eq!(after_restart.0, NativeState::Stale);
    assert_eq!(
        after_restart.1["reason"],
        "work's session, display, capture or input process changed since it was checked."
    );
    assert_eq!(older.0, NativeState::Stale);
    assert_eq!(
        older.1["reason"],
        "The observation is older than 30 minutes."
    );
    assert_eq!(unproven.0, NativeState::NotChecked);
    assert_eq!(
        unproven.1["reason"],
        "Whether native Save/Open works on work is judged once it is proven again."
    );
    assert_eq!(failed.0, NativeState::Failed);
    assert_eq!(
        failed.1["reason"],
        "Native Save/Open did not complete in work's session."
    );
    for said in [&before.1, &passed.1, &after_restart.1, &failed.1] {
        assert!(!said.to_string().contains("helper"), "{said}");
    }
}

#[test]
fn a_second_logins_record_never_counts_for_a_vm() {
    let records = tempfile::tempdir().unwrap();
    let at = now();
    // What Second Login keeps: tied to this Mac's boot and the helper's
    // processes, under its own name.
    let second_login = json!({"observedAt": at - 60, "passed": true,
        "checks": [{"kind": "save", "passed": true, "stage": null, "error": null},
            {"kind": "open", "passed": true, "stage": null, "error": null}],
        "generation": {"capturePid": 123, "inputPid": 456, "securitySessionId": 42,
            "displayId": 7, "display": {"width": 1920.0, "height": 1200.0, "scale": 1.0},
            "boot": 1_700_000_000},
        "evidence": []});
    let bytes = serde_json::to_vec(&second_login).unwrap();
    std::fs::write(records.path().join("native-502.json"), &bytes).unwrap();

    let elsewhere = assessment("work", records.path(), &proven(&epoch("boot-1")), at);
    std::fs::write(records.path().join("native.json"), &bytes).unwrap();
    let in_its_place = assessment("work", records.path(), &proven(&epoch("boot-1")), at);

    assert_eq!(elsewhere.0, NativeState::NotChecked, "{}", elsewhere.1);
    assert_eq!(in_its_place.0, NativeState::Unknown, "{}", in_its_place.1);
    assert_ne!(in_its_place.1["state"], "passed");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_native_scan_answers_only_with_the_fixtures_scan_of_this_challenge_in_this_epoch() {
    let directory = tempfile::Builder::new().tempdir_in("/tmp").unwrap();
    let (client, _keeper) = crate::test_support::vm_through_keeper(directory.path());
    let ownership = client.controller.acquire(false).unwrap();
    let verified = client.verify_vm().await.unwrap();
    let guest = guest(&client, &ownership, &verified);
    let canary = Canary::new();
    std::fs::write(canary.0.join("state.json"), b"{\"pid\":4242}").unwrap();
    let folder = canary.0.clone();
    // The fixture's observer: a scan of another attempt once it is handed
    // the challenge, then the scan asked for.
    let observer = std::thread::spawn(move || {
        let handed = loop {
            if let Some(handed) = std::fs::read(folder.join("challenge.json"))
                .ok()
                .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
            {
                break handed;
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        for attempt in [
            "1D4C7B2A-6E5F-4A3B-8C9D-0E1F2A3B4C5D",
            "8E3F2A10-4B7C-4D2E-9A61-3C5B7D9E0F12",
        ] {
            let mut said = scan(
                attempt,
                handed["challenge"].as_str().unwrap(),
                handed["epoch"].as_str().unwrap(),
                true,
            )["scan"]
                .clone();
            said["controls"] = json!([{"role": "AXButton", "title": "Cancel", "enabled": true}]);
            std::fs::write(folder.join("panel-observer.json"), said.to_string()).unwrap();
            std::thread::sleep(Duration::from_millis(150));
        }
        handed
    });

    let observed = guest
        .observed(&canary.fixture(), "8E3F2A10-4B7C-4D2E-9A61-3C5B7D9E0F12")
        .await
        .unwrap();
    let handed = observer.join().unwrap();
    let state = guest.text(&canary.fixture(), "state.json").await;

    assert_eq!(handed["epoch"], EPOCH, "{handed}");
    let observed = observed.expect("the scan of this challenge");
    assert!(canary::button(&observed, "Cancel").is_some_and(|cancel| cancel.enabled));
    assert!(!guest.late.get());
    assert_eq!(state.as_deref(), Some("{\"pid\":4242}"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_fixture_not_allowed_accessibility_in_the_guest_is_said_in_the_vms_words() {
    let directory = tempfile::Builder::new().tempdir_in("/tmp").unwrap();
    let (client, _keeper) = crate::test_support::vm_through_keeper(directory.path());
    let ownership = client.controller.acquire(false).unwrap();
    let verified = client.verify_vm().await.unwrap();
    let guest = guest(&client, &ownership, &verified);
    let canary = Canary::new();
    let folder = canary.0.clone();
    let observer = std::thread::spawn(move || {
        for _ in 0..200 {
            if let Some(handed) = std::fs::read(folder.join("challenge.json"))
                .ok()
                .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
            {
                let said = scan(
                    "8E3F2A10-4B7C-4D2E-9A61-3C5B7D9E0F12",
                    handed["challenge"].as_str().unwrap(),
                    handed["epoch"].as_str().unwrap(),
                    false,
                )["scan"]
                    .clone();
                std::fs::write(folder.join("panel-observer.json"), said.to_string()).unwrap();
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    });

    let refused = guest
        .observed(&canary.fixture(), "8E3F2A10-4B7C-4D2E-9A61-3C5B7D9E0F12")
        .await;
    observer.join().unwrap();

    let refused = refused.err().expect("an untrusted fixture is refused");
    assert_eq!(
        refused.error,
        "The fixture in work is not allowed Accessibility; allow it in work's window during \
         maintenance with atlas desktop maintain work."
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_guest_without_native_check_v1_is_told_to_update_and_nothing_is_launched() {
    let directory = tempfile::Builder::new().tempdir_in("/tmp").unwrap();
    let (client, keeper) = crate::test_support::vm_through_keeper(directory.path());
    *keeper.observing.lock().unwrap() = Some(|answer| {
        let older: Vec<&str> = atlas_session::compatibility::MACHINE
            .into_iter()
            .filter(|name| *name != NATIVE_CHECK)
            .collect();
        answer["declared"]["capabilities"] = json!(older);
        answer["status"]["hello"]["daemon"]["capabilities"] = json!(older);
    });
    let fixture = directory.path().join("DesktopFixture");
    std::fs::write(&fixture, b"fixture").unwrap();

    let refused = canary::verify(&client, &fixture).await.unwrap_err();

    assert_eq!(refused.code, Code::Update, "{refused:?}");
    assert_eq!(
        refused.error,
        "work's Atlas is from an older build and cannot check native Save and Open."
    );
    assert!(
        keeper.asked.lock().unwrap().iter().all(|asked| !matches!(
            asked["op"].as_str(),
            Some("stage" | "change")
        ) && asked["request"]["op"] != "native"),
        "nothing copied, run or read"
    );
    assert!(!client.records().join("native.json").exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_vm_whose_input_is_not_allowed_is_refused_in_its_guests_words_before_anything_is_launched()
 {
    let directory = tempfile::Builder::new().tempdir_in("/tmp").unwrap();
    let (client, keeper) = crate::test_support::vm_through_keeper(directory.path());
    *keeper.held.lock().unwrap() = Some(
        "In the VM's window, open System Settings › Privacy & Security › Accessibility and turn \
         Atlas on.",
    );
    let fixture = directory.path().join("DesktopFixture");
    std::fs::write(&fixture, b"fixture").unwrap();

    let refused = canary::verify(&client, &fixture).await.unwrap_err();

    assert_eq!(refused.code, Code::Permission, "{refused:?}");
    assert!(refused.error.contains("Accessibility"), "{refused:?}");
    assert_eq!(
        refused.details["next"],
        "Ask the person to open work's window with atlas desktop maintain work and allow Atlas \
         in Privacy & Security, Accessibility, there."
    );
    assert!(!refused.to_string().contains("helper"), "{refused:?}");
    assert!(
        keeper
            .asked
            .lock()
            .unwrap()
            .iter()
            .all(|asked| !matches!(asked["op"].as_str(), Some("stage" | "change"))),
        "nothing copied or run"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_vm_check_whose_paths_have_no_counterpart_on_this_mac_passes() {
    let directory = tempfile::Builder::new().tempdir_in("/tmp").unwrap();
    let (client, keeper) = crate::test_support::vm_through_keeper(directory.path());
    let ownership = client.controller.acquire(false).unwrap();
    let verified = client.verify_vm().await.unwrap();
    let guest = guest(&client, &ownership, &verified);
    let fixture = Fixture {
        pid: 4242,
        program: PathBuf::from("/Users/agent/DesktopFixture"),
        directory: PathBuf::from("/private/tmp/atlas-canary.Zz9Yy8"),
    };
    let expected = fixture.directory.join("atlas-fixture.txt");
    assert!(!expected.exists(), "no such path here");
    // The guest's word on what the path names in its folder.
    *keeper.answering.lock().unwrap() =
        Some(json!({"ok": true, "op": "native", "resolved": ["atlas-fixture.txt"]}));

    let same = guest
        .same_path(
            &fixture,
            Some("/private/tmp/atlas-canary.Zz9Yy8/atlas-fixture.txt"),
            &expected,
        )
        .await;
    let other = guest
        .same_path(
            &fixture,
            Some("/private/tmp/atlas-canary.Zz9Yy8/x"),
            &fixture.directory,
        )
        .await;

    assert!(same);
    assert!(!other, "the guest named a file, not the folder");
    let resolves: Vec<Value> = keeper
        .asked_for("read")
        .into_iter()
        .filter(|read| read["request"]["action"] == "resolve")
        .collect();
    assert_eq!(resolves.len(), 2, "{resolves:?}");
    assert_eq!(
        resolves[0]["request"]["paths"],
        json!(["/private/tmp/atlas-canary.Zz9Yy8/atlas-fixture.txt"])
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_path_with_a_trailing_slash_a_link_or_outside_the_canary_folder_never_names_its_file() {
    let directory = tempfile::Builder::new().tempdir_in("/tmp").unwrap();
    let (client, _keeper) = crate::test_support::vm_through_keeper(directory.path());
    let ownership = client.controller.acquire(false).unwrap();
    let verified = client.verify_vm().await.unwrap();
    let guest = guest(&client, &ownership, &verified);
    let canary = Canary::new();
    std::fs::write(canary.0.join("atlas-fixture.txt"), b"saved").unwrap();
    let outside = Canary::new();
    std::fs::write(outside.0.join("atlas-open-fixture.txt"), b"opened").unwrap();
    std::os::unix::fs::symlink(
        outside.0.join("atlas-open-fixture.txt"),
        canary.0.join("atlas-open-fixture.txt"),
    )
    .unwrap();
    let fixture = canary.fixture();
    let (saved, opened) = (
        canary.0.join("atlas-fixture.txt"),
        canary.0.join("atlas-open-fixture.txt"),
    );
    let said = |path: &Path| path.to_str().unwrap().to_owned();

    let real = guest.same_path(&fixture, Some(&said(&saved)), &saved).await;
    let folder = guest
        .same_path(&fixture, Some(&said(&canary.0)), &canary.0)
        .await;
    let slashed = guest
        .same_path(&fixture, Some(&format!("{}/", said(&saved))), &saved)
        .await;
    let linked = guest
        .same_path(&fixture, Some(&said(&opened)), &opened)
        .await;
    let elsewhere = guest
        .same_path(
            &fixture,
            Some(&said(&outside.0.join("atlas-open-fixture.txt"))),
            &opened,
        )
        .await;

    assert!(real && folder);
    assert!(!slashed, "atlas-fixture.txt/ is no file");
    assert!(!linked, "a link in the folder");
    assert!(!elsewhere, "a file outside the folder");
}

use super::*;
use crate::vm::tests::{Fake, desktop, home};
use atlas_core::process::Instance;

/// A VM's folder as a runner leaves it: its disk and the runs it kept,
/// its runner `runner` and its VM's process `machine`.
fn made(vms: &Vms, runner: i32, machine: Option<i32>) {
    let folder = vms.folder(&desktop());
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("Disk.asif"), "disk").unwrap();
    let process = |pid: i32| json!({"pid": pid, "startSeconds": 100, "startMicroseconds": 0});
    let mut record = json!({"run": process(runner), "earlier": []});
    if let Some(machine) = machine {
        record["machine"] = process(machine);
    }
    atlas_core::files::write_private(
        &folder.join("Incarnation.json"),
        record.to_string().as_bytes(),
    )
    .unwrap();
}

fn change() -> Token {
    Token::parse(&"c".repeat(48)).unwrap()
}

#[test]
fn a_stopped_vm_is_removed_whole_and_asked_again_answers_the_same() {
    let (_home, vms) = home();
    let fake = Fake::new();
    made(&vms, 41, Some(42));
    *fake.reply.lock().unwrap() = Some(json!({"ok": true, "outcome": {"state": "done"}}));
    std::fs::create_dir_all(vms.agents()).unwrap();
    std::fs::write(vms.agent_plist(&desktop()), "plist").unwrap();

    let removed = remove_with(&vms, &fake, &desktop(), &change()).unwrap();
    let again = remove_with(&vms, &fake, &desktop(), &change()).unwrap();

    assert_eq!(removed["removal"], "done", "{removed}");
    assert_eq!(again, removed);
    assert!(!vms.folder(&desktop()).exists());
    assert!(!vms.agent_plist(&desktop()).exists());
    assert_eq!(fake.asked.lock().unwrap()[0]["op"], "forget");
    assert!(fake.launched.lock().unwrap()[0].starts_with("bootout gui/"));
    assert_eq!(removed["passwords"][desktop().as_str()], "deleted");
    assert!(
        fake.deleted.lock().unwrap().is_empty(),
        "the runner deleted them"
    );
}

#[test]
fn a_runner_holding_the_vms_lock_keeps_its_folder_until_it_goes() {
    let (_home, vms) = home();
    let fake = Fake::new();
    made(&vms, 41, Some(42));
    let runner = try_lock(&vms.lifecycle(&desktop())).unwrap().unwrap();

    let held = remove_with(&vms, &fake, &desktop(), &change()).unwrap();
    drop(runner);
    let removed = remove_with(&vms, &fake, &desktop(), &change()).unwrap();

    assert_eq!(held["removal"], "pending", "{held}");
    assert_eq!(removed["removal"], "done", "{removed}");
    assert_eq!(
        fake.asked.lock().unwrap().len(),
        2,
        "forget is asked again while the runner held the lock"
    );
}

#[test]
fn a_vm_still_running_or_untold_keeps_its_folder() {
    for case in [
        "machine runs",
        "runner runs",
        "none kept, one started since",
        "untold",
    ] {
        let (_home, vms) = home();
        let fake = Fake::new();
        match case {
            "machine runs" => {
                made(&vms, 41, Some(42));
                fake.running.lock().unwrap().push(42);
            }
            "runner runs" => {
                made(&vms, 41, Some(42));
                fake.running.lock().unwrap().push(41);
            }
            "none kept, one started since" => {
                made(&vms, 41, None);
                *fake.machines.lock().unwrap() = Some(vec![Instance::new(99, [200, 0])]);
            }
            _ => {
                made(&vms, 41, None);
                *fake.machines.lock().unwrap() = None;
            }
        }

        let removal = remove_with(&vms, &fake, &desktop(), &change()).unwrap();

        assert_eq!(removal["removal"], "pending", "{case}: {removal}");
        assert!(vms.folder(&desktop()).join("Disk.asif").exists(), "{case}");
    }
}

#[test]
fn what_a_vms_runs_left_unreadable_keeps_its_folder() {
    let (_home, vms) = home();
    let fake = Fake::new();
    made(&vms, 41, Some(42));
    atlas_core::files::write_private(&vms.folder(&desktop()).join("Incarnation.json"), b"{")
        .unwrap();

    let removal = remove_with(&vms, &fake, &desktop(), &change()).unwrap();

    assert_eq!(removal["removal"], "pending", "{removal}");
    assert!(vms.folder(&desktop()).exists());
}

#[test]
fn a_folder_made_since_the_removal_began_is_never_deleted() {
    let (_home, vms) = home();
    let fake = Fake::new();
    made(&vms, 41, Some(42));
    // The first try stops at a runner that still holds the lock.
    let runner = try_lock(&vms.lifecycle(&desktop())).unwrap().unwrap();
    remove_with(&vms, &fake, &desktop(), &change()).unwrap();
    drop(runner);
    std::fs::remove_dir_all(vms.folder(&desktop())).unwrap();
    std::fs::create_dir_all(vms.folder(&desktop())).unwrap();
    std::fs::write(vms.folder(&desktop()).join("other"), "other").unwrap();

    let removal = remove_with(&vms, &fake, &desktop(), &change()).unwrap();

    assert_eq!(removal["removal"], "done", "{removal}");
    assert!(vms.folder(&desktop()).join("other").exists());
    assert_eq!(removal["kept"].as_array().unwrap().len(), 1, "{removal}");
}

#[test]
fn a_removal_cut_short_after_its_folder_moved_aside_deletes_it_when_asked_again() {
    let (_home, vms) = home();
    let fake = Fake::new();
    made(&vms, 41, Some(42));
    let runner = try_lock(&vms.lifecycle(&desktop())).unwrap().unwrap();
    remove_with(&vms, &fake, &desktop(), &change()).unwrap();
    drop(runner);
    // As a crash between the move and the deletion leaves it.
    let mut record = read(&vms.removal(&desktop())).unwrap().unwrap();
    record.step = Step::Deleting;
    record.proved = Some(Vec::new());
    keep(&vms, &vms.removal(&desktop()), &record).unwrap();
    let aside = vms.parent.join(format!(
        ".removing-{}-{}",
        desktop().as_str(),
        change().as_str()
    ));
    std::fs::rename(vms.folder(&desktop()), &aside).unwrap();

    let removal = remove_with(&vms, &fake, &desktop(), &change()).unwrap();

    assert_eq!(removal["removal"], "done", "{removal}");
    assert!(!aside.exists());
    assert!(removal["kept"].as_array().unwrap().is_empty());
}

#[test]
fn another_removal_of_a_vm_being_removed_is_refused() {
    let (_home, vms) = home();
    let fake = Fake::new();
    made(&vms, 41, Some(42));
    let runner = try_lock(&vms.lifecycle(&desktop())).unwrap().unwrap();
    remove_with(&vms, &fake, &desktop(), &change()).unwrap();

    let other = remove_with(
        &vms,
        &fake,
        &desktop(),
        &Token::parse(&"d".repeat(48)).unwrap(),
    )
    .unwrap();

    assert_eq!(other["ok"], false, "{other}");
    drop(runner);
}

#[test]
fn passwords_its_runner_did_not_delete_are_deleted_never_asking() {
    let (_home, vms) = home();
    let fake = Fake::new();
    made(&vms, 41, Some(42));

    let removal = remove_with(&vms, &fake, &desktop(), &change()).unwrap();

    assert_eq!(removal["removal"], "done", "{removal}");
    assert_eq!(
        *fake.deleted.lock().unwrap(),
        [
            desktop().as_str().to_owned(),
            format!("{}.admin", desktop().as_str())
        ]
    );
}

#[test]
fn an_older_runner_holding_only_its_folders_own_lock_keeps_the_folder() {
    let (_home, vms) = home();
    let fake = Fake::new();
    made(&vms, 41, Some(42));
    let older = crate::vm::take_lock(&vms.folder(&desktop()).join("Runner.lock"))
        .unwrap()
        .unwrap();

    let held = remove_with(&vms, &fake, &desktop(), &change()).unwrap();
    drop(older);
    let removed = remove_with(&vms, &fake, &desktop(), &change()).unwrap();

    assert_eq!(held["removal"], "pending", "{held}");
    assert_eq!(removed["removal"], "done", "{removed}");
}

#[test]
fn a_runner_busy_when_asked_to_forget_is_asked_again_while_it_lives() {
    let (_home, vms) = home();
    let fake = Fake::new();
    made(&vms, 41, Some(42));
    *fake.reply.lock().unwrap() =
        Some(json!({"ok": false, "error": "Another change is being done"}));
    let runner = try_lock(&vms.lifecycle(&desktop())).unwrap().unwrap();

    remove_with(&vms, &fake, &desktop(), &change()).unwrap();
    *fake.reply.lock().unwrap() = Some(json!({"ok": true, "outcome": {"state": "done"}}));
    remove_with(&vms, &fake, &desktop(), &change()).unwrap();
    drop(runner);
    let removed = remove_with(&vms, &fake, &desktop(), &change()).unwrap();

    let asked = fake.asked.lock().unwrap().clone();
    assert_eq!(asked.len(), 3, "{asked:?}");
    assert!(
        asked
            .iter()
            .all(|asked| asked["change"] == change().as_str())
    );
    assert_eq!(removed["removal"], "done", "{removed}");
    assert!(
        fake.deleted.lock().unwrap().is_empty(),
        "the runner deleted them"
    );
}

#[test]
fn a_job_a_late_bootstrap_brings_after_the_proof_keeps_the_removal_from_done() {
    let (_home, vms) = home();
    let fake = Fake::new();
    made(&vms, 41, Some(42));
    // Absent at retiring and at the proof; there before done.
    *fake.late.lock().unwrap() = Some((3, atlas_core::launchd::Loaded::default()));

    let first = remove_with(&vms, &fake, &desktop(), &change()).unwrap();
    let second = remove_with(&vms, &fake, &desktop(), &change()).unwrap();

    assert_eq!(first["removal"], "pending", "{first}");
    assert_eq!(first["step"], "passwords", "{first}");
    assert_eq!(second["removal"], "done", "{second}");
    assert!(
        fake.launched
            .lock()
            .unwrap()
            .iter()
            .filter(|asked| asked.starts_with("bootout"))
            .count()
            >= 2
    );
}

#[test]
fn a_finished_removal_holds_no_vm_and_one_under_way_does() {
    let (_home, vms) = home();
    let fake = Fake::new();
    made(&vms, 41, Some(42));
    *fake.reply.lock().unwrap() = Some(json!({"ok": true, "outcome": {"state": "done"}}));

    remove_with(&vms, &fake, &desktop(), &change()).unwrap();

    assert!(
        vms.kept().unwrap().is_empty(),
        "a finished removal kept a VM"
    );
    let mut record = read(&vms.removal(&desktop())).unwrap().unwrap();
    record.step = Step::Deleting;
    keep(&vms, &vms.removal(&desktop()), &record).unwrap();
    assert_eq!(vms.kept().unwrap(), vec![desktop()]);
}

#[test]
fn a_removal_record_that_cannot_be_read_may_hold_a_vm() {
    let (_home, vms) = home();
    std::fs::create_dir_all(vms.removal(&desktop()).parent().unwrap()).unwrap();
    std::fs::write(vms.removal(&desktop()), "not a record").unwrap();

    assert!(vms.kept().is_err());
}

#[test]
fn a_removal_retires_the_keeper_first_and_deletes_the_station_folder_and_enrolment() {
    let (home, vms) = home();
    let fake = Fake::new();
    made(&vms, 41, Some(42));
    *fake.reply.lock().unwrap() = Some(json!({"ok": true, "outcome": {"state": "done"}}));
    let label = Vms::keeper_label(&desktop());
    let plist = vms.agents().join(format!("{label}.plist"));
    std::fs::create_dir_all(vms.agents()).unwrap();
    std::fs::write(&plist, "plist").unwrap();
    *fake.keeper_job.lock().unwrap() = Some(atlas_core::launchd::Loaded {
        pid: Some(77),
        ..atlas_core::launchd::Loaded::default()
    });
    let station = crate::station::Station::under(home.path())
        .folder(&desktop())
        .unwrap();
    std::fs::write(station.join("resume.key"), "capability").unwrap();

    let removed = remove_with(&vms, &fake, &desktop(), &change()).unwrap();

    assert_eq!(removed["removal"], "done", "{removed}");
    assert_eq!(
        fake.launched.lock().unwrap()[0],
        format!("bootout {}/{label}", vms.domain()),
        "the keeper went first"
    );
    assert!(!plist.exists());
    assert!(!station.exists());
    assert_eq!(removed["enrolment"], "deleted");
    assert_eq!(
        *fake.enrolments.lock().unwrap(),
        [desktop().as_str().to_owned()]
    );
}

#[test]
fn a_keeper_launchd_still_runs_or_one_holding_its_lock_keeps_the_removal_pending() {
    let (_home, vms) = home();
    let fake = Fake::new();
    made(&vms, 41, Some(42));
    *fake.reply.lock().unwrap() = Some(json!({"ok": true, "outcome": {"state": "done"}}));
    let keeper = try_lock(&vms.keeper_lock(&desktop())).unwrap().unwrap();

    let held = remove_with(&vms, &fake, &desktop(), &change()).unwrap();

    assert_eq!(held["removal"], "pending", "{held}");
    assert!(held["why"].as_str().unwrap().contains("keeper"), "{held}");
    assert!(vms.folder(&desktop()).exists());
    assert!(fake.asked.lock().unwrap().is_empty(), "nothing went on");

    drop(keeper);
    let removed = remove_with(&vms, &fake, &desktop(), &change()).unwrap();
    assert_eq!(removed["removal"], "done", "{removed}");
}

#[test]
fn a_keeper_starting_while_its_vm_is_removed_is_refused_end_to_end() {
    let (_home, vms) = home();
    let fake = Fake::new();
    made(&vms, 41, Some(42));
    // The runner is busy: the removal is recorded and stays under way.
    let runner = try_lock(&vms.lifecycle(&desktop())).unwrap().unwrap();

    let held = remove_with(&vms, &fake, &desktop(), &change()).unwrap();
    let starting = super::super::keeper::hold(&vms, &desktop());

    assert_eq!(held["removal"], "pending", "{held}");
    assert_eq!(starting.unwrap_err().code, atlas_core::error::Code::State);
    drop(runner);
}

#[test]
fn a_keeper_job_a_late_bootstrap_brings_keeps_the_removal_from_done() {
    let (_home, vms) = home();
    let fake = Fake::new();
    made(&vms, 41, Some(42));
    *fake.reply.lock().unwrap() = Some(json!({"ok": true, "outcome": {"state": "done"}}));
    // Absent when the keeper is retired first; there before done.
    *fake.keeper_late.lock().unwrap() = Some((2, atlas_core::launchd::Loaded::default()));

    let first = remove_with(&vms, &fake, &desktop(), &change()).unwrap();
    let second = remove_with(&vms, &fake, &desktop(), &change()).unwrap();

    assert_eq!(first["removal"], "pending", "{first}");
    assert_eq!(first["step"], "passwords", "{first}");
    assert!(first["why"].as_str().unwrap().contains("keeper"), "{first}");
    assert_eq!(second["removal"], "done", "{second}");
    let keeper = format!("bootout {}/{}", vms.domain(), Vms::keeper_label(&desktop()));
    assert!(
        fake.launched
            .lock()
            .unwrap()
            .iter()
            .filter(|asked| **asked == keeper)
            .count()
            >= 2
    );
}

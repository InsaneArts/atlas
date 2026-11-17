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
        fake.ran.lock().unwrap().is_empty(),
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

fn admin() -> String {
    format!("{}.admin", desktop().as_str())
}

/// What Atlas VM's deletion-only entry keeps, for the removal `change`.
fn entry_result(vms: &Vms, change: &str, guest: &str, admin_fate: &str) -> (PathBuf, Value) {
    (
        vms.forgotten(&desktop()),
        json!({"change": change, "passwords": {desktop().as_str(): guest, admin(): admin_fate}}),
    )
}

#[test]
fn passwords_a_runner_that_cannot_run_left_are_deleted_by_atlas_vms_deletion_only_entry() {
    let (_home, vms) = home();
    let fake = Fake::new();
    made(&vms, 41, Some(42));
    *fake.entry.lock().unwrap() = Some(entry_result(&vms, change().as_str(), "deleted", "absent"));

    let removal = remove_with(&vms, &fake, &desktop(), &change()).unwrap();

    assert_eq!(removal["removal"], "done", "{removal}");
    assert_eq!(
        *fake.ran.lock().unwrap(),
        [format!(
            "{} forget-passwords {} --removal {}",
            fake.program().unwrap().display(),
            desktop().as_str(),
            change().as_str()
        )],
        "run from this Atlas's own bundle, under this removal"
    );
    assert_eq!(removal["passwords"][desktop().as_str()], "deleted");
    assert_eq!(removal["passwords"][admin()], "absent");
    assert_eq!(removal["left"], json!([]));
}

#[test]
fn passwords_are_left_and_named_when_the_deletion_only_entry_fails_too() {
    for case in [
        "exits otherwise",
        "another removal's result",
        "no result",
        "a password it left",
    ] {
        let (_home, vms) = home();
        let fake = Fake::new();
        made(&vms, 41, Some(42));
        let other = "d".repeat(48);
        let result = match case {
            "another removal's result" => Some(entry_result(&vms, &other, "deleted", "deleted")),
            "no result" => None,
            "a password it left" => Some(entry_result(&vms, change().as_str(), "deleted", "left")),
            _ => Some(entry_result(&vms, change().as_str(), "deleted", "deleted")),
        };
        *fake.entry.lock().unwrap() = result;
        *fake.entry_fails.lock().unwrap() = case == "exits otherwise";

        let removal = remove_with(&vms, &fake, &desktop(), &change()).unwrap();

        assert_eq!(removal["removal"], "done", "{case}: {removal}");
        assert_eq!(fake.ran.lock().unwrap().len(), 1, "{case}");
        assert_eq!(removal["passwords"][admin()], "left", "{case}: {removal}");
        let left = removal["left"].as_array().unwrap();
        assert!(
            left.contains(&json!({"service": "com.insanearts.atlas.vm", "account": admin()})),
            "{case}: {removal}"
        );
        if case == "a password it left" {
            assert_eq!(removal["passwords"][desktop().as_str()], "deleted");
            assert_eq!(left.len(), 1, "{case}: {removal}");
        } else {
            assert_eq!(removal["passwords"][desktop().as_str()], "left");
            assert_eq!(left.len(), 2, "{case}: {removal}");
        }
    }
}

#[test]
fn an_earlier_result_of_the_deletion_only_entry_is_never_taken_for_this_ones() {
    let (_home, vms) = home();
    let fake = Fake::new();
    made(&vms, 41, Some(42));
    let (path, earlier) = entry_result(&vms, change().as_str(), "deleted", "deleted");
    atlas_core::files::private_folder(&vms.removals(), atlas_core::error::Code::State).unwrap();
    atlas_core::files::write_private(&path, earlier.to_string().as_bytes()).unwrap();
    // This run exits cleanly and keeps nothing.
    let removal = remove_with(&vms, &fake, &desktop(), &change()).unwrap();

    assert_eq!(
        removal["passwords"][desktop().as_str()],
        "left",
        "{removal}"
    );
    assert!(!path.exists());
}

#[test]
fn items_left_are_looked_up_again_and_recorded_gone_once_deleted_by_hand() {
    let (_home, vms) = home();
    let fake = Fake::new();
    made(&vms, 41, Some(42));
    // Locked: neither the entry nor the station could delete anything.
    *fake.entry_fails.lock().unwrap() = true;
    *fake.locked.lock().unwrap() = true;
    let left = remove_with(&vms, &fake, &desktop(), &change()).unwrap();
    assert_eq!(left["enrolment"], "left", "{left}");
    assert_eq!(left["left"].as_array().unwrap().len(), 3, "{left}");

    // Still locked: a lookup that cannot tell leaves each as it was.
    let untold = remove_with(&vms, &fake, &desktop(), &change()).unwrap();
    assert_eq!(untold, left);

    // The person deleted the guest's password and the enrolment by hand.
    *fake.locked.lock().unwrap() = false;
    *fake.passwords.lock().unwrap() = vec![admin()];
    let looked = remove_with(&vms, &fake, &desktop(), &change()).unwrap();

    assert_eq!(looked["passwords"][desktop().as_str()], "gone", "{looked}");
    assert_eq!(looked["passwords"][admin()], "left", "{looked}");
    assert_eq!(looked["enrolment"], "gone", "{looked}");
    assert_eq!(
        looked["left"],
        json!([{"service": "com.insanearts.atlas.vm", "account": admin()}])
    );
    let recorded = recorded(&vms, &desktop()).unwrap().unwrap();
    assert_eq!(
        recorded.passwords[desktop().as_str()],
        "gone",
        "kept in the removal's own record"
    );
    assert_eq!(recorded.enrolment.as_deref(), Some("gone"));

    fake.passwords.lock().unwrap().clear();
    let cleared = remove_with(&vms, &fake, &desktop(), &change()).unwrap();
    assert_eq!(cleared["left"], json!([]), "{cleared}");
    assert_eq!(
        fake.ran.lock().unwrap().len(),
        1,
        "the entry is not run again"
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
        fake.ran.lock().unwrap().is_empty(),
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

#[test]
fn a_removal_under_the_callers_lock_never_takes_it_again_and_its_keeper_lock_comes_after() {
    let (_home, vms) = home();
    let fake = Fake::new();
    made(&vms, 41, Some(42));
    *fake.reply.lock().unwrap() = Some(json!({"ok": true, "outcome": {"state": "done"}}));
    let uninstalling = crate::vm::lifecycle::Lifecycle::under(&vms.home)
        .uninstalling(&Token::new(), &[])
        .unwrap();
    let hold = crate::vm::hold_in(&vms, &desktop(), &uninstalling)
        .unwrap()
        .unwrap();

    let wrapped = remove_with(&vms, &fake, &desktop(), &change()).unwrap_err();
    let removed = removed(&vms, &fake, &hold, &change()).unwrap();

    assert!(
        wrapped.error.contains("being uninstalled"),
        "{}",
        wrapped.error
    );
    assert_eq!(removed["removal"], "done", "{removed}");
    assert!(!vms.folder(&desktop()).exists());
}

/// What the controller of the VM `vm` keeps in the state folder under
/// `home`, with a pending command and the person's Stop, beside a Second
/// Login's own: the names of each, sorted.
pub(in crate::vm) fn left_state(home: &Path, vm: &DesktopId) -> (Vec<String>, Vec<String>) {
    let vm = atlas_core::desktop::Desktop::from(atlas_core::vm::Vm { id: vm.clone() });
    let user = atlas_core::desktop::Desktop::from(atlas_core::user::User { uid: 502 });
    for desktop in [&vm, &user] {
        let controller = atlas_control::Controller::in_home(desktop, home);
        drop(
            controller
                .quarantine_by(
                    Token::new().as_str(),
                    atlas_control::executors::Executor::Guest,
                )
                .unwrap(),
        );
        controller.note_stop(true).unwrap();
    }
    (state_of(home, &vm), state_of(home, &user))
}

/// The names `desktop`'s controller keeps in the state folder under `home`,
/// sorted.
pub(in crate::vm) fn state_of(home: &Path, desktop: &atlas_core::desktop::Desktop) -> Vec<String> {
    let folder = home.join("Library/Caches/Atlas");
    let named = format!("{}.", desktop.id().as_str());
    let mut names: Vec<String> = std::fs::read_dir(folder)
        .map(|entries| {
            entries
                .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
                .filter(|name| name.starts_with(&named))
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

#[test]
fn a_vm_removed_whole_forgets_its_controller_state_and_never_anothers() {
    let (home, vms) = home();
    let fake = Fake::new();
    made(&vms, 41, Some(42));
    *fake.reply.lock().unwrap() = Some(json!({"ok": true, "outcome": {"state": "done"}}));
    let (own, others) = left_state(home.path(), &desktop());
    assert!(!own.is_empty());

    let removed = remove_with(&vms, &fake, &desktop(), &change()).unwrap();

    assert_eq!(removed["removal"], "done", "{removed}");
    let vm = atlas_core::desktop::Desktop::from(atlas_core::vm::Vm { id: desktop() });
    let user = atlas_core::desktop::Desktop::from(atlas_core::user::User { uid: 502 });
    assert!(
        state_of(home.path(), &vm).is_empty(),
        "{:?}",
        state_of(home.path(), &vm)
    );
    assert_eq!(state_of(home.path(), &user), others);
}

#[test]
fn a_removal_that_kept_a_folder_or_left_a_keychain_item_leaves_the_vms_controller_state() {
    let (home, vms) = home();
    let fake = Fake::new();
    made(&vms, 41, Some(42));
    let runner = try_lock(&vms.lifecycle(&desktop())).unwrap().unwrap();
    remove_with(&vms, &fake, &desktop(), &change()).unwrap();
    drop(runner);
    std::fs::remove_dir_all(vms.folder(&desktop())).unwrap();
    std::fs::create_dir_all(vms.folder(&desktop())).unwrap();
    let (own, _) = left_state(home.path(), &desktop());

    let removal = remove_with(&vms, &fake, &desktop(), &change()).unwrap();

    assert_eq!(removal["removal"], "done", "{removal}");
    assert_eq!(removal["kept"].as_array().unwrap().len(), 1, "{removal}");
    let vm = atlas_core::desktop::Desktop::from(atlas_core::vm::Vm { id: desktop() });
    assert_eq!(state_of(home.path(), &vm), own);
}

#[test]
fn a_vm_whose_control_a_command_still_holds_is_not_done_until_it_lets_go() {
    let (home, vms) = home();
    let fake = Fake::new();
    made(&vms, 41, Some(42));
    *fake.reply.lock().unwrap() = Some(json!({"ok": true, "outcome": {"state": "done"}}));
    let vm = atlas_core::desktop::Desktop::from(atlas_core::vm::Vm { id: desktop() });
    let held = atlas_control::Controller::in_home(&vm, home.path())
        .acquire(false)
        .unwrap();

    let busy = remove_with(&vms, &fake, &desktop(), &change()).unwrap_err();
    drop(held);
    let removed = remove_with(&vms, &fake, &desktop(), &change()).unwrap();

    assert_eq!(busy.code, atlas_core::error::Code::Busy);
    assert_eq!(removed["removal"], "done", "{removed}");
    assert!(state_of(home.path(), &vm).is_empty());
}

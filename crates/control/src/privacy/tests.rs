use super::*;
use crate::{Beside, Lease};
use atlas_core::error::Holder;
use std::time::Duration;

fn github() -> PrivateHold {
    PrivateHold {
        site: "github.com".to_owned(),
        kind: HoldKind::SignIn,
        browser: Some("com.google.Chrome".to_owned()),
    }
}

/// A process that is gone: a child that was started and killed.
fn killed() -> Instance {
    let mut child =
        atlas_core::spawn::spawn(std::process::Command::new("/bin/sleep").arg("30")).unwrap();
    let pid = i32::try_from(child.id()).unwrap();
    let holder = Instance::new(pid, atlas_core::process::start(pid).unwrap());
    child.kill().unwrap();
    child.wait().unwrap();
    holder
}

fn interrupted(error: &Error) {
    assert_eq!(error.code, "busy");
    assert_eq!(error.holder, Some(Holder::Person));
    assert_eq!(error.error, crate::busy::SIGN_IN_INTERRUPTED);
}

/// Write the marker as `holder` would have under the person's lease now.
fn marker_of(controller: &Controller, holder: Instance) {
    let lease = fs::read_to_string(&controller.person).unwrap();
    let marker = Marker {
        hold: github(),
        holder,
        lease,
        since: now_ms(),
        let_go: false,
    };
    fs::write(&controller.private, serde_json::to_vec(&marker).unwrap()).unwrap();
}

fn hidden(error: &Error) {
    assert_eq!(error.code, "busy");
    assert_eq!(error.holder, Some(Holder::Person));
    assert_eq!(
        error.error,
        "The person is managing an account in the helper; the screen is hidden until they finish."
    );
}

#[test]
fn a_hold_hides_the_screen_while_its_lease_lives_and_ends_with_it() {
    let directory = tempfile::tempdir().unwrap();
    let controller = Controller::at(505, directory.path());
    assert!(controller.screen_visible().is_ok());

    let mut lease = Lease::new(Duration::from_secs(10));
    let token = lease.acquire(&controller).unwrap();
    lease.hide_screen(&controller, &token, &github()).unwrap();

    hidden(&controller.screen_visible().unwrap_err());
    assert_eq!(controller.private_hold(), Some(github()));

    lease.release();
    assert!(!lease.active());
    assert!(
        controller.screen_visible().is_ok(),
        "a hold left behind ends with its lease"
    );
    assert_eq!(controller.private_hold(), None);
}

#[test]
fn ending_a_hold_shows_the_screen_again_while_control_is_still_held() {
    let directory = tempfile::tempdir().unwrap();
    let controller = Controller::at(505, directory.path());
    let mut lease = Lease::new(Duration::from_secs(10));
    let token = lease.acquire(&controller).unwrap();
    lease.hide_screen(&controller, &token, &github()).unwrap();

    controller.end_private().unwrap();

    assert!(controller.screen_visible().is_ok());
    assert!(lease.active());
    controller.end_private().unwrap();
}

#[test]
fn a_hold_whose_holder_died_stays_hidden_and_refuses_agents() {
    let directory = tempfile::tempdir().unwrap();
    let controller = Controller::at(505, directory.path());
    let person = controller.acquire_for_person(Beside::Commands).unwrap();
    marker_of(&controller, killed());
    drop(person);

    interrupted(&controller.screen_visible().unwrap_err());
    assert_eq!(controller.private_hold(), None);
    assert_eq!(
        controller.interrupted_hold(),
        Some(Interrupted {
            hold: Some(github())
        })
    );
    interrupted(&controller.acquire(false).unwrap_err());
    interrupted(&controller.acquire_beside_commands().unwrap_err());
    interrupted(&controller.acquire_for_cleanup().unwrap_err());
    interrupted(
        &controller
            .acquire_for_service_change(false, Duration::from_millis(10))
            .unwrap_err(),
    );
}

#[test]
fn taking_control_again_never_clears_an_interrupted_hold() {
    let directory = tempfile::tempdir().unwrap();
    let controller = Controller::at(505, directory.path());
    let earlier = controller.acquire_for_person(Beside::Commands).unwrap();
    marker_of(&controller, killed());
    drop(earlier);

    let later = controller.acquire_for_person(Beside::Commands).unwrap();

    assert!(controller.private.exists());
    interrupted(&controller.screen_visible().unwrap_err());
    drop(later);
    assert!(controller.interrupted_hold().is_some());
}

#[test]
fn an_adopted_hold_is_live_for_every_reader_and_holds_the_exclusion_until_it_ends() {
    let directory = tempfile::tempdir().unwrap();
    let controller = Controller::at(505, directory.path());
    let person = controller.acquire_for_person(Beside::Commands).unwrap();
    marker_of(&controller, killed());
    drop(person);

    let mut adopted = controller.adopt_interrupted().unwrap().unwrap();

    assert_eq!(adopted.hold, Some(github()));
    assert_eq!(controller.private_hold(), Some(github()), "live again");
    assert_eq!(controller.interrupted_hold(), None);
    hidden(&controller.screen_visible().unwrap_err());
    let other = Controller::at(505, directory.path());
    assert_eq!(other.acquire(false).unwrap_err().code, "busy", "held");

    adopted.end().unwrap();
    drop(adopted);

    assert!(controller.screen_visible().is_ok());
    assert!(!controller.private.exists());
    assert!(other.acquire(false).is_ok());
}

#[test]
fn an_adopted_hold_dropped_before_it_ends_is_interrupted_again() {
    let directory = tempfile::tempdir().unwrap();
    let controller = Controller::at(505, directory.path());
    controller.interrupt_for_test(&github());

    let adopted = controller.adopt_interrupted().unwrap().unwrap();
    hidden(&controller.screen_visible().unwrap_err());
    drop(adopted);

    interrupted(&controller.screen_visible().unwrap_err());
    assert!(controller.interrupted_hold().is_some());
}

#[test]
fn a_hold_that_cannot_be_interrupted_again_stays_held_until_the_process_exits() {
    use std::os::unix::fs::PermissionsExt;

    let directory = tempfile::tempdir().unwrap();
    let controller = Controller::at(505, directory.path());
    controller.interrupt_for_test(&github());
    let adopted = controller.adopt_interrupted().unwrap().unwrap();
    // The folder can't be written: the marker can't name a gone holder.
    let folder = controller.private.parent().unwrap().to_owned();
    fs::set_permissions(&folder, fs::Permissions::from_mode(0o500)).unwrap();

    drop(adopted);

    hidden(&controller.screen_visible().unwrap_err());
    assert_eq!(
        Controller::at(505, directory.path())
            .acquire(false)
            .unwrap_err()
            .code,
        "busy",
        "the exclusion is kept"
    );
    fs::set_permissions(&folder, fs::Permissions::from_mode(0o700)).unwrap();
}

#[test]
fn ending_a_hold_moves_the_generation_before_the_screen_is_shown() {
    let directory = tempfile::tempdir().unwrap();
    let controller = Controller::at(505, directory.path());
    controller.interrupt_for_test(&github());
    let mut adopted = controller.adopt_interrupted().unwrap().unwrap();
    // A generation that can't be written: a folder in its place.
    fs::remove_file(&controller.private_generation).ok();
    fs::create_dir(&controller.private_generation).unwrap();

    assert!(adopted.end().is_err());

    assert!(controller.private.exists(), "the screen stays hidden");
    assert!(controller.screen_visible().is_err());
    fs::remove_dir(&controller.private_generation).unwrap();
    adopted.end().unwrap();
    assert!(controller.screen_visible().is_ok());
}

#[test]
fn nothing_is_adopted_when_no_hold_is_interrupted() {
    let directory = tempfile::tempdir().unwrap();
    let controller = Controller::at(505, directory.path());
    assert!(controller.adopt_interrupted().unwrap().is_none());
    let _person = controller.acquire_for_person(Beside::Commands).unwrap();
    marker_of(&controller, Instance::current().unwrap());
    let other = Controller::at(505, directory.path());
    assert!(other.adopt_interrupted().unwrap().is_none(), "a live hold");
}

#[test]
fn a_hold_from_an_earlier_lease_is_ignored_under_a_later_one() {
    let directory = tempfile::tempdir().unwrap();
    let controller = Controller::at(505, directory.path());
    let earlier = controller.acquire_for_person(Beside::Commands).unwrap();
    marker_of(&controller, Instance::current().unwrap());
    hidden(&controller.screen_visible().unwrap_err());
    drop(earlier);

    let before = controller.generation().unwrap();

    let _later = controller.acquire_for_person(Beside::Commands).unwrap();

    assert!(
        !controller.private.exists(),
        "the earlier lease's marker is cleared before the later one is noted"
    );
    assert!(controller.screen_visible().is_ok());
    assert_eq!(controller.generation().unwrap(), before + 1);
}

#[test]
fn an_unreadable_marker_is_interrupted_whoever_holds_control() {
    let directory = tempfile::tempdir().unwrap();
    let controller = Controller::at(505, directory.path());
    let earlier = controller.acquire_for_person(Beside::Commands).unwrap();
    fs::write(&controller.private, b"{\"site\":\"git").unwrap();
    drop(earlier);

    interrupted(&controller.screen_visible().unwrap_err());
    assert_eq!(
        controller.interrupted_hold(),
        Some(Interrupted { hold: None })
    );
    let later = controller.acquire_for_person(Beside::Commands).unwrap();
    assert!(controller.private.exists(), "only the person ends it");
    drop(later);

    // Nothing of it can be read, so it stays interrupted while adopted,
    // and the adopter's exclusion keeps every other controller out.
    let mut adopted = controller.adopt_interrupted().unwrap().unwrap();
    assert_eq!(adopted.hold, None);
    interrupted(&controller.screen_visible().unwrap_err());
    assert_eq!(
        Controller::at(505, directory.path())
            .acquire_for_person(Beside::Commands)
            .unwrap_err()
            .code,
        "busy"
    );
    adopted.end().unwrap();
    drop(adopted);
    assert!(controller.screen_visible().is_ok());
}

#[test]
fn a_marker_is_written_whole_and_never_through_a_link() {
    let directory = tempfile::tempdir().unwrap();
    let controller = Controller::at(505, directory.path());
    let mut lease = Lease::new(Duration::from_secs(10));
    let token = lease.acquire(&controller).unwrap();
    let elsewhere = directory.path().join("elsewhere");
    fs::write(&elsewhere, b"kept").unwrap();
    std::os::unix::fs::symlink(&elsewhere, &controller.private).unwrap();

    lease.hide_screen(&controller, &token, &github()).unwrap();

    assert_eq!(fs::read(&elsewhere).unwrap(), b"kept");
    assert!(
        !fs::symlink_metadata(&controller.private)
            .unwrap()
            .is_symlink()
    );
    assert_eq!(controller.private_hold(), Some(github()));
    lease.release();
}

#[test]
fn the_generation_goes_up_when_a_hold_begins_and_when_it_ends() {
    let directory = tempfile::tempdir().unwrap();
    let controller = Controller::at(505, directory.path());
    let before = controller.screen_visible().unwrap();
    let mut lease = Lease::new(Duration::from_secs(10));
    let token = lease.acquire(&controller).unwrap();

    lease.hide_screen(&controller, &token, &github()).unwrap();
    controller.end_private().unwrap();

    let after = controller.screen_visible().unwrap();
    assert_eq!(after, Visible(before.0 + 2));
    hidden(&controller.still_visible(before).unwrap_err());
    controller.still_visible(after).unwrap();
    assert_eq!(
        fs::read_to_string(&controller.private_generation).unwrap(),
        "2"
    );
}

#[tokio::test]
async fn a_read_that_a_hold_began_during_is_dropped() {
    let directory = tempfile::tempdir().unwrap();
    let controller = Controller::at(505, directory.path());
    let mut lease = Lease::new(Duration::from_secs(10));
    let token = lease.acquire(&controller).unwrap();

    let read = controller
        .shown(async {
            lease.hide_screen(&controller, &token, &github()).unwrap();
            Ok("the screen")
        })
        .await;

    hidden(&read.unwrap_err());
    controller.end_private().unwrap();
}

#[tokio::test]
async fn a_read_that_a_hold_began_and_ended_during_is_dropped() {
    let directory = tempfile::tempdir().unwrap();
    let controller = Controller::at(505, directory.path());
    let mut lease = Lease::new(Duration::from_secs(10));
    let token = lease.acquire(&controller).unwrap();

    let read = controller
        .shown(async {
            lease.hide_screen(&controller, &token, &github()).unwrap();
            controller.end_private().unwrap();
            Ok("the screen")
        })
        .await;

    hidden(&read.unwrap_err());
    assert_eq!(controller.shown(async { Ok(1) }).await.unwrap(), 1);
}

#[tokio::test]
async fn a_read_is_refused_before_it_begins_while_a_hold_is_in_force() {
    let directory = tempfile::tempdir().unwrap();
    let controller = Controller::at(505, directory.path());
    let mut lease = Lease::new(Duration::from_secs(10));
    let token = lease.acquire(&controller).unwrap();
    lease.hide_screen(&controller, &token, &github()).unwrap();
    let mut began = false;

    let read = controller
        .shown(async {
            began = true;
            Ok(())
        })
        .await;

    hidden(&read.unwrap_err());
    assert!(!began);
}

#[test]
fn a_lease_for_a_sign_in_is_refused_beside_a_running_command() {
    let directory = tempfile::tempdir().unwrap();
    let controller = Controller::at(505, directory.path());
    let command = controller.quarantine_command().unwrap();
    let mut lease = Lease::new(Duration::from_secs(10));

    let refused = lease
        .acquire_beside(&controller, Beside::Nothing)
        .unwrap_err();

    assert_eq!(refused.code, "busy");
    assert_eq!(refused.holder, Some(Holder::Agent));
    assert_eq!(refused.details["running"], "command");
    assert!(!lease.active());
    assert!(
        lease.acquire(&controller).is_ok(),
        "Take Control goes on beside it"
    );
    lease.release();
    controller.clear_if(&command).unwrap();
}

/// The controller of the VM `work`, with its state in `directory`.
fn of_vm(directory: &Path) -> Controller {
    let vm = atlas_core::vm::Vm {
        id: atlas_core::desktop::DesktopId::parse("0123456789abcdef01234567").unwrap(),
    };
    Controller::of_at(&atlas_core::desktop::Desktop::from(vm), directory).called("work")
}

#[test]
fn a_vms_hold_outlives_its_lease_until_nothing_the_person_did_is_pending() {
    let directory = tempfile::tempdir().unwrap();
    let controller = of_vm(directory.path());
    let mut lease = Lease::new(Duration::from_secs(10));
    let token = lease.acquire_beside(&controller, Beside::Nothing).unwrap();
    lease.hide_screen(&controller, &token, &github()).unwrap();
    let release = controller.quarantine().unwrap();
    assert!(
        !controller.end_private_once_let_go().unwrap(),
        "never while its lease lives"
    );

    lease.release();
    let hidden_still = controller.screen_visible().unwrap_err();
    assert!(
        !controller.end_private_once_let_go().unwrap(),
        "never while the person's release is pending"
    );
    let still = controller.screen_visible().unwrap_err();
    controller.clear_if(&release).unwrap();
    drop(release);
    let ended = controller.end_private_once_let_go().unwrap();

    for refused in [&hidden_still, &still] {
        assert_eq!(refused.code, "busy");
        assert_eq!(refused.holder, Some(Holder::Person));
        assert_eq!(
            refused.error,
            "The person is managing an account in work; the screen is hidden until they finish."
        );
    }
    assert_eq!(controller.private_hold(), None, "no sign-in under way");
    assert_eq!(controller.interrupted_hold(), None);
    assert!(ended);
    assert!(controller.screen_visible().is_ok());
}

#[test]
fn a_vms_hold_whose_holder_died_is_interrupted_in_the_vms_words() {
    let directory = tempfile::tempdir().unwrap();
    let controller = of_vm(directory.path());

    controller.interrupt_for_test(&github());

    let refused = controller.screen_visible().unwrap_err();
    assert_eq!(
        refused.error,
        "A sign-in in work was interrupted; the screen stays hidden and changes wait until the \
         person finishes or cancels it in Atlas."
    );
    assert!(
        !controller.end_private_once_let_go().unwrap(),
        "only the person ends an interrupted one"
    );
    assert!(controller.interrupted_hold().is_some());
}

#[test]
fn second_logins_hold_never_outlives_its_lease() {
    let directory = tempfile::tempdir().unwrap();
    let controller = Controller::at(505, directory.path());
    let mut lease = Lease::new(Duration::from_secs(10));
    let token = lease.acquire(&controller).unwrap();
    lease.hide_screen(&controller, &token, &github()).unwrap();

    let marker = fs::read_to_string(&controller.private).unwrap();
    lease.release();

    assert!(!marker.contains("letGo"), "{marker}");
    assert!(controller.screen_visible().is_ok());
    assert!(!controller.end_private_once_let_go().unwrap());
}

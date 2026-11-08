use super::*;
use crate::{Beside, Lease};
use atlas_core::error::Holder;
use std::time::Duration;

fn github() -> PrivateHold {
    PrivateHold {
        site: "github.com".to_owned(),
        kind: HoldKind::SignIn,
    }
}

/// Write the marker as `holder` would have under the person's lease now.
fn marker_of(controller: &Controller, holder: Instance) {
    let lease = fs::read_to_string(&controller.person).unwrap();
    let marker = Marker {
        hold: github(),
        holder,
        lease,
        since: now_ms(),
    };
    fs::write(&controller.private, serde_json::to_vec(&marker).unwrap()).unwrap();
}

fn hidden(error: &Error) {
    assert_eq!(error.code, "busy");
    assert_eq!(error.holder, Some(Holder::Person));
    assert_eq!(
        error.error,
        "The person is signing in to an account in the helper; the screen is hidden until they finish."
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
fn a_hold_whose_holder_was_killed_is_ignored() {
    let directory = tempfile::tempdir().unwrap();
    let controller = Controller::at(505, directory.path());
    let _person = controller.acquire_for_person(Beside::Commands).unwrap();
    let mut child =
        atlas_core::spawn::spawn(std::process::Command::new("/bin/sleep").arg("30")).unwrap();
    let pid = i32::try_from(child.id()).unwrap();
    let holder = Instance::new(pid, atlas_core::process::start(pid).unwrap());
    marker_of(&controller, holder);
    hidden(&controller.screen_visible().unwrap_err());

    child.kill().unwrap();
    child.wait().unwrap();

    assert!(controller.screen_visible().is_ok());
    assert_eq!(controller.private_hold(), None);
}

#[test]
fn a_hold_from_an_earlier_lease_is_ignored_under_a_later_one() {
    let directory = tempfile::tempdir().unwrap();
    let controller = Controller::at(505, directory.path());
    let earlier = controller.acquire_for_person(Beside::Commands).unwrap();
    marker_of(&controller, Instance::current().unwrap());
    hidden(&controller.screen_visible().unwrap_err());
    drop(earlier);

    let _later = controller.acquire_for_person(Beside::Commands).unwrap();

    assert!(
        controller.private.exists(),
        "the earlier lease's marker is still there"
    );
    assert!(controller.screen_visible().is_ok());
}

#[test]
fn an_unreadable_hold_hides_the_screen_only_while_the_person_has_control() {
    let directory = tempfile::tempdir().unwrap();
    let controller = Controller::at(505, directory.path());
    let person = controller.acquire_for_person(Beside::Commands).unwrap();
    fs::write(&controller.private, b"{not a hold").unwrap();

    hidden(&controller.screen_visible().unwrap_err());
    assert_eq!(controller.private_hold(), None);

    drop(person);
    assert!(controller.screen_visible().is_ok());
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

//! Which apps a VM has, asked of its guest end to end: through a real
//! client and a real keeper, its guest a stand-in that looks in its
//! Applications folder by a machine guest's own code.
use super::*;
use crate::vm::keeper::{
    change::tests::{live, proven},
    picture::tests::session,
};
use std::collections::BTreeMap;

/// The browsers doctor asks a VM after.
const BROWSERS: [&str; 2] = ["Google Chrome", "Firefox"];

/// An installed read of Firefox, as the station asks it.
fn installed() -> Value {
    json!({"op": "installed", "names": ["Firefox"], "session": session()})
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_vm_is_asked_which_browsers_it_has_and_a_fresh_one_has_neither() {
    let (_serving, _saying, client, _directory) = proven().await;

    let fresh = crate::app_selection::installed(&client, &BROWSERS)
        .await
        .unwrap();
    let report = crate::capability::vm_doctor_report(&client, client.status().await).await;

    assert_eq!(
        fresh,
        BTreeMap::from([
            ("Firefox".to_owned(), false),
            ("Google Chrome".to_owned(), false)
        ])
    );
    assert_eq!(
        report["capabilities"]["installedApps"],
        json!({"state": "checked", "apps": {"Google Chrome": false, "Firefox": false},
            "reason": "Neither Google Chrome nor Firefox is installed in work; sign-ins there \
                need one of them."}),
        "{report}"
    );
    assert!(
        !report["capabilities"].to_string().contains("helper"),
        "{report}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_vms_browser_counts_only_as_a_real_folder_in_its_applications_folder() {
    let (serving, _saying, client, _directory) = proven().await;
    let (applications, elsewhere) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    std::fs::create_dir_all(applications.path().join("Firefox.app/Contents")).unwrap();
    std::fs::create_dir(elsewhere.path().join("Google Chrome.app")).unwrap();
    std::os::unix::fs::symlink(
        elsewhere.path().join("Google Chrome.app"),
        applications.path().join("Google Chrome.app"),
    )
    .unwrap();
    *serving.fixture.guest.standin.applications.lock().unwrap() =
        vec![applications.path().to_owned()];

    let installed = crate::app_selection::installed(&client, &BROWSERS)
        .await
        .unwrap();
    let report = crate::capability::vm_doctor_report(&client, client.status().await).await;

    assert!(installed["Firefox"]);
    assert!(!installed["Google Chrome"], "a link is never followed");
    assert_eq!(
        report["capabilities"]["installedApps"]["reason"],
        "Firefox is installed in work; Google Chrome is not."
    );
}

#[test]
fn the_keeper_carries_an_installed_read_naming_apps_and_no_other_shape() {
    assert!(carries(&LaneKind::Read, &installed()));
    let mut unpinned = installed();
    unpinned.as_object_mut().unwrap().remove("session");
    let mut unnamed = installed();
    unnamed.as_object_mut().unwrap().remove("names");
    let mut one_name = installed();
    one_name["names"] = json!("Firefox");
    for refused in [unpinned, unnamed, one_name] {
        assert!(!carries(&LaneKind::Read, &refused), "{refused}");
    }
    assert!(!carries(&LaneKind::Input { change: None }, &installed()));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_installed_read_the_keeper_does_not_carry_is_refused_before_any_lane_opens() {
    let (serving, _saying, _client, _directory) = proven().await;
    let generation = live(&serving);
    let mut unpinned = installed();
    unpinned.as_object_mut().unwrap().remove("session");

    let carried = super::tests::asked(
        &serving.socket,
        &json!({"op": "read", "request": installed(), "generation": generation}),
    )
    .await;
    let lanes = crate::vm::keeper::change::tests::admitted(&serving).len();
    let refused = super::tests::asked(
        &serving.socket,
        &json!({"op": "read", "request": unpinned, "generation": generation}),
    )
    .await;

    assert_eq!(
        carried.last().unwrap(),
        &json!({"ok": true, "op": "installed", "installed": {"Firefox": false}}),
        "{carried:?}"
    );
    assert_eq!(refused[0]["code"], "denied", "{refused:?}");
    assert_eq!(
        crate::vm::keeper::change::tests::admitted(&serving).len(),
        lanes,
        "no lane opened for it"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_vm_whose_applications_folder_it_cannot_search_is_not_checked_in_its_guests_words() {
    use std::os::unix::fs::PermissionsExt;
    let (serving, _saying, client, _directory) = proven().await;
    let (system, home) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let own = home.path().join("Applications");
    std::fs::create_dir_all(own.join("Firefox.app")).unwrap();
    let set = |mode| std::fs::set_permissions(&own, std::fs::Permissions::from_mode(mode)).unwrap();
    set(0o400);
    *serving.fixture.guest.standin.applications.lock().unwrap() =
        vec![system.path().to_owned(), own.clone()];

    let refused = crate::app_selection::installed(&client, &BROWSERS).await;
    let report = crate::capability::vm_doctor_report(&client, client.status().await).await;

    set(0o700);
    let refused = refused.unwrap_err();
    assert_eq!(refused.code, Code::Apps, "{refused:?}");
    assert_eq!(
        report["capabilities"]["installedApps"],
        json!({"state": "notChecked", "reason": "An Applications folder could not be read, so \
            which apps are installed is not known."}),
        "{report}"
    );
}

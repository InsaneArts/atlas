use super::*;
use crate::viewer::{http, unpoisoned};
use atlas_client::{
    sign_ins::{self, Listed},
    test_support::{Helper, completed, exited},
};
use atlas_core::test_support::UID;
use std::sync::Mutex;

#[test]
fn a_typed_address_opens_as_typed_and_is_listed_by_its_site() {
    for (typed, address, site) in [
        ("github.com", "github.com", "github.com"),
        (
            " HTTPS://Accounts.Google.com/signin?x=1 ",
            "accounts.google.com",
            "google.com",
        ),
        ("alice.github.io/", "alice.github.io", "alice.github.io"),
        ("localhost:3000", "localhost:3000", "localhost"),
        ("http://127.0.0.1:8080/login", "127.0.0.1:8080", "127.0.0.1"),
    ] {
        assert_eq!(
            site_of(&json!(typed)).unwrap(),
            (address.to_owned(), site.to_owned()),
            "{typed}"
        );
    }
}

#[test]
fn anything_but_a_sites_address_is_refused() {
    for typed in [
        json!(""),
        json!("   "),
        json!("https://"),
        json!("git hub.com"),
        json!("me@github.com"),
        json!("github..com"),
        json!("[::1]"),
        json!(42),
        Value::Null,
    ] {
        let refused = site_of(&typed).unwrap_err();
        assert_eq!(refused.code, Code::Input, "{typed}");
        assert_eq!(refused.error, "Type a site's address, like github.com");
    }
}

#[test]
fn each_hold_opens_the_browsers_own_page_for_it() {
    let pages = [
        (
            Browser::Chrome,
            HoldKind::SignIn,
            "https://accounts.google.com",
        ),
        (
            Browser::Chrome,
            HoldKind::Forget,
            "chrome://settings/content/all?searchSubpage=google.com",
        ),
        (
            Browser::Chrome,
            HoldKind::Reset,
            "chrome://settings/clearBrowserData",
        ),
        (
            Browser::Firefox,
            HoldKind::SignIn,
            "https://accounts.google.com",
        ),
        (
            Browser::Firefox,
            HoldKind::Forget,
            "about:preferences#privacy",
        ),
        (
            Browser::Firefox,
            HoldKind::Reset,
            "about:preferences#privacy",
        ),
    ];
    for (browser, kind, page) in pages {
        assert_eq!(
            browser.page(kind, "accounts.google.com", "google.com"),
            page,
            "{browser:?} {kind:?}"
        );
    }
}

#[test]
fn chrome_is_chosen_when_installed_and_firefox_otherwise() {
    let applications = tempfile::tempdir().unwrap();
    assert_eq!(Browser::installed_in(applications.path()), None);
    fs_dir(applications.path(), "Firefox.app");
    assert_eq!(
        Browser::installed_in(applications.path()),
        Some(Browser::Firefox)
    );
    fs_dir(applications.path(), "Google Chrome.app");
    assert_eq!(
        Browser::installed_in(applications.path()),
        Some(Browser::Chrome)
    );
    assert_eq!(Browser::of("com.google.Chrome"), Some(Browser::Chrome));
    assert_eq!(Browser::of("org.mozilla.firefox"), Some(Browser::Firefox));
    assert_eq!(Browser::of("com.apple.Safari"), None);
}

fn fs_dir(parent: &Path, name: &str) {
    std::fs::create_dir(parent.join(name)).unwrap();
}

/// How the stand-in helper answers a sign-in's steps.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Fails {
    Nothing,
    /// An agent's background job still runs.
    Job,
    /// The browser can't be opened.
    Open,
    /// The helper saw the person type a password into another site, and
    /// says so as control ends.
    Typed,
}

/// A viewer of a stand-in helper with an input service, keeping the
/// person's accounts in `directory`, and that helper.
fn viewer_of(directory: &Path) -> (Arc<Viewer>, Helper) {
    let (client, helper) = Helper::client(directory, UID);
    let helper = helper.serving_input(&client, &[]);
    let viewer = http::configured(client, "127.0.0.1:1".to_owned(), None);
    *unpoisoned(&viewer.accounts_home) = Some(directory.join("home"));
    (viewer, helper)
}

/// The stand-in's answer to `request`, noting each step's word in `steps`.
fn step(steps: &Mutex<Vec<String>>, fails: Fails, request: &Value) -> Option<Value> {
    let word = match (request["op"].as_str(), request["action"].as_str()) {
        (Some("job"), _) => "jobs".to_owned(),
        (Some("run"), _) => format!(
            "open {}",
            request["argv"]
                .as_array()
                .unwrap()
                .iter()
                .skip(1)
                .map(|part| part.as_str().unwrap())
                .collect::<Vec<_>>()
                .join(" ")
        ),
        (Some(op), _) => op.to_owned(),
        (None, _) => panic!("a request without an op: {request}"),
    };
    steps.lock().unwrap().push(word.clone());
    Some(match word.as_str() {
        "jobs" if fails == Fails::Job => json!({"ok": true, "op": "job", "action": "show",
            "jobs": [{"id": "7c2e91a04b3f", "name": "web", "task": null, "session": null,
                "uid": UID, "boot": "B", "pid": 4312, "start": [1761650000, 123456],
                "pgid": 4312, "state": "running", "startedAt": 10, "endsAt": 20,
                "logResets": 0}]}),
        "jobs" => json!({"ok": true, "op": "job", "action": "show", "jobs": []}),
        open if open.starts_with("open ") && fails == Fails::Open => {
            exited(1, "Unable to find application named 'Google Chrome'")
        }
        open if open.starts_with("open ") => completed(""),
        "release" if fails == Fails::Typed => json!({"ok": true, "signedIn":
            {"app": "com.google.Chrome", "name": "Google Chrome", "host": "gitlab.com"}}),
        _ => json!({"ok": true}),
    })
}

/// Start `kind` for github.com in Chrome, as the stand-in answers `fails`.
async fn started(
    viewer: &Arc<Viewer>,
    helper: &Helper,
    steps: &Mutex<Vec<String>>,
    kind: HoldKind,
    fails: Fails,
) -> Result<Value> {
    let body = json!({"site": "github.com", "browser": "com.google.Chrome"});
    helper
        .serve(viewer.sign_in_start(kind, &body), |request| {
            step(steps, fails, request)
        })
        .await
}

/// Nothing a refused start began is left: no lease, no hold, no sign-in.
async fn nothing_left(viewer: &Arc<Viewer>) {
    let controller = viewer.client().controller.clone();
    assert!(!viewer.lease.lock().await.active());
    assert!(viewer.sign_in.lock().await.is_none());
    assert!(controller.screen_visible().is_ok());
    assert_eq!(controller.private_hold(), None);
    drop(controller.acquire(false).expect("control is free"));
}

#[tokio::test]
async fn a_sign_in_takes_control_checks_jobs_hides_the_screen_then_opens_the_site() {
    let directory = tempfile::tempdir().unwrap();
    let (viewer, helper) = viewer_of(directory.path());
    let steps = Mutex::new(Vec::new());
    let controller = viewer.client().controller.clone();

    let reply = started(&viewer, &helper, &steps, HoldKind::SignIn, Fails::Nothing)
        .await
        .unwrap();

    assert_eq!(
        *steps.lock().unwrap(),
        [
            "acquire",
            "jobs",
            "open -n -a Google Chrome --args --new-window https://github.com"
        ]
    );
    assert_eq!(reply["site"], "github.com");
    assert_eq!(reply["kind"], "signIn");
    assert_eq!(reply["browser"], "com.google.Chrome");
    let lease = reply["lease"].as_str().unwrap();
    viewer.lease.lock().await.require(lease).unwrap();
    assert_eq!(
        controller.private_hold(),
        Some(PrivateHold {
            site: "github.com".to_owned(),
            kind: HoldKind::SignIn
        })
    );
    assert_eq!(controller.screen_visible().unwrap_err().code, Code::Busy);
    assert_eq!(
        controller.acquire(false).unwrap_err().holder,
        Some(Holder::Person),
        "agents are kept out by the person's control"
    );
    assert!(!controller.stop_noted(), "Stop is never pressed for it");
    viewer.release(Some(lease)).await.ok();
}

#[tokio::test]
async fn done_adds_the_site_and_ends_the_hold_and_control_while_a_stop_stands() {
    let directory = tempfile::tempdir().unwrap();
    let (viewer, helper) = viewer_of(directory.path());
    let steps = Mutex::new(Vec::new());
    let controller = viewer.client().controller.clone();
    let reply = started(&viewer, &helper, &steps, HoldKind::SignIn, Fails::Nothing)
        .await
        .unwrap();
    let lease = reply["lease"].as_str().unwrap().to_owned();
    // The person stops agents while they sign in.
    controller.note_stop(true).unwrap();

    let done = helper
        .serve(viewer.sign_in_done(&lease), |request| {
            step(&steps, Fails::Nothing, request)
        })
        .await
        .unwrap();

    assert_eq!(done["added"], true);
    assert_eq!(done["site"], "github.com");
    nothing_left(&viewer).await;
    assert!(
        controller.stop_noted(),
        "the person's Stop stands after Done"
    );
    let Listed::Accounts(accounts) = sign_ins::listed_in(&directory.path().join("home")) else {
        panic!("the list can be read");
    };
    assert_eq!(accounts.len(), 1);
    assert_eq!(accounts[0].id, "github.com");
    assert_eq!(accounts[0].browser.as_deref(), Some("com.google.Chrome"));
    assert_eq!(accounts[0].level, sign_ins::Level::Ask);
    assert_eq!(
        viewer.sign_in_done(&lease).await.unwrap_err().code,
        Code::LeaseLost,
        "a sign-in is done once"
    );
}

#[tokio::test]
async fn agents_reads_are_refused_during_a_sign_in_and_allowed_after_it() {
    let directory = tempfile::tempdir().unwrap();
    let (viewer, helper) = viewer_of(directory.path());
    let steps = Mutex::new(Vec::new());
    let controller = viewer.client().controller.clone();
    let before = controller.screen_visible().unwrap();
    let reply = started(&viewer, &helper, &steps, HoldKind::SignIn, Fails::Nothing)
        .await
        .unwrap();
    let lease = reply["lease"].as_str().unwrap().to_owned();

    let read = controller.shown(async { Ok(()) }).await.unwrap_err();
    assert_eq!(read.holder, Some(Holder::Person));
    assert_eq!(
        read.error,
        "The person is signing in to an account in the helper; the screen is hidden until they finish."
    );
    helper
        .serve(viewer.sign_in_cancel(&lease), |request| {
            step(&steps, Fails::Nothing, request)
        })
        .await
        .unwrap();

    controller.shown(async { Ok(()) }).await.unwrap();
    assert!(
        controller.still_visible(before).is_err(),
        "a read admitted before the sign-in is never kept"
    );
}

#[tokio::test]
async fn cancel_ends_the_hold_and_control_and_adds_nothing() {
    let directory = tempfile::tempdir().unwrap();
    let (viewer, helper) = viewer_of(directory.path());
    let steps = Mutex::new(Vec::new());
    let reply = started(&viewer, &helper, &steps, HoldKind::SignIn, Fails::Nothing)
        .await
        .unwrap();
    let lease = reply["lease"].as_str().unwrap().to_owned();

    // The person typed a password before giving up: it is never listed.
    let cancelled = helper
        .serve(viewer.sign_in_cancel(&lease), |request| {
            step(&steps, Fails::Typed, request)
        })
        .await
        .unwrap();

    assert_eq!(cancelled["ended"], true);
    assert_eq!(steps.lock().unwrap().last().unwrap(), "release");
    nothing_left(&viewer).await;
    assert_eq!(
        sign_ins::listed_in(&directory.path().join("home")),
        Listed::Accounts(Vec::new())
    );
    assert_eq!(
        viewer.sign_in_cancel(&lease).await.unwrap()["ended"],
        false,
        "cancelling again is fine"
    );
}

#[tokio::test]
async fn forgetting_opens_the_sites_data_in_settings_and_removes_nothing_on_done() {
    let directory = tempfile::tempdir().unwrap();
    let (viewer, helper) = viewer_of(directory.path());
    let steps = Mutex::new(Vec::new());
    let home = Some(directory.path().join("home"));
    super::super::accounts::add_signed_in(&home, "github.com", "com.google.Chrome").unwrap();

    let reply = started(&viewer, &helper, &steps, HoldKind::Forget, Fails::Nothing)
        .await
        .unwrap();
    assert_eq!(
        steps.lock().unwrap().last().unwrap(),
        "open -n -a Google Chrome --args --new-window \
         chrome://settings/content/all?searchSubpage=github.com"
    );
    let lease = reply["lease"].as_str().unwrap().to_owned();
    let done = helper
        .serve(viewer.sign_in_done(&lease), |request| {
            step(&steps, Fails::Nothing, request)
        })
        .await
        .unwrap();

    assert_eq!(done["kind"], "forget");
    assert!(done.get("added").is_none());
    nothing_left(&viewer).await;
    let Listed::Accounts(accounts) = sign_ins::listed_in(&directory.path().join("home")) else {
        panic!("the list can be read");
    };
    assert_eq!(
        accounts.len(),
        1,
        "the person removes it, if they choose to"
    );
}

#[tokio::test]
async fn a_sign_in_beside_an_agents_running_command_is_refused_with_nothing_held() {
    let directory = tempfile::tempdir().unwrap();
    let (viewer, helper) = viewer_of(directory.path());
    let steps = Mutex::new(Vec::new());
    let command = viewer.client().controller.quarantine_command().unwrap();

    let refused = started(&viewer, &helper, &steps, HoldKind::SignIn, Fails::Nothing)
        .await
        .unwrap_err();

    assert_eq!(refused.code, Code::Busy);
    assert_eq!(
        refused.error,
        "An agent's command is still running. Try again when it ends."
    );
    assert_eq!(refused.holder, Some(Holder::Agent));
    assert!(steps.lock().unwrap().is_empty(), "{:?}", steps.lock());
    assert!(!viewer.lease.lock().await.active());
    assert!(viewer.sign_in.lock().await.is_none());
    assert!(viewer.client().controller.screen_visible().is_ok());
    viewer.client().controller.clear_if(&command).unwrap();
}

#[tokio::test]
async fn a_sign_in_beside_an_agents_background_job_is_undone() {
    let directory = tempfile::tempdir().unwrap();
    let (viewer, helper) = viewer_of(directory.path());
    let steps = Mutex::new(Vec::new());

    let refused = started(&viewer, &helper, &steps, HoldKind::SignIn, Fails::Job)
        .await
        .unwrap_err();

    assert_eq!(refused.code, Code::Busy);
    assert_eq!(
        refused.error,
        "An agent's background job is still running. Stop it, or try again when it ends."
    );
    assert_eq!(*steps.lock().unwrap(), ["acquire", "jobs", "release"]);
    nothing_left(&viewer).await;
}

#[tokio::test]
async fn a_hold_that_cant_be_set_is_undone_before_anything_opens() {
    let directory = tempfile::tempdir().unwrap();
    let (viewer, helper) = viewer_of(directory.path());
    let steps = Mutex::new(Vec::new());
    let controller = viewer.client().controller.clone();
    // A folder where the screen's privacy generation goes: the hold can't
    // be counted, so it never begins.
    let marker = controller.private_generation_path().to_owned();
    std::fs::create_dir_all(&marker).unwrap();

    let refused = started(&viewer, &helper, &steps, HoldKind::SignIn, Fails::Nothing).await;

    assert!(refused.is_err());
    assert_eq!(*steps.lock().unwrap(), ["acquire", "jobs", "release"]);
    std::fs::remove_dir(&marker).unwrap();
    nothing_left(&viewer).await;
}

#[tokio::test]
async fn a_browser_that_cant_open_is_undone_with_the_hold_and_control() {
    let directory = tempfile::tempdir().unwrap();
    let (viewer, helper) = viewer_of(directory.path());
    let steps = Mutex::new(Vec::new());
    let before = viewer.client().controller.screen_visible().unwrap();

    let refused = started(&viewer, &helper, &steps, HoldKind::SignIn, Fails::Open)
        .await
        .unwrap_err();

    assert_eq!(refused.code, Code::SpawnFailed);
    assert_eq!(steps.lock().unwrap().last().unwrap(), "release");
    nothing_left(&viewer).await;
    assert!(
        viewer.client().controller.still_visible(before).is_err(),
        "the hold began and ended"
    );
}

#[tokio::test]
async fn a_sign_in_renews_without_a_frame_and_only_for_its_own_lease() {
    let directory = tempfile::tempdir().unwrap();
    let (viewer, helper) = viewer_of(directory.path());
    let steps = Mutex::new(Vec::new());
    let reply = started(&viewer, &helper, &steps, HoldKind::SignIn, Fails::Nothing)
        .await
        .unwrap();
    let lease = reply["lease"].as_str().unwrap().to_owned();

    helper
        .serve(viewer.sign_in_renew(&lease), |request| {
            step(&steps, Fails::Nothing, request)
        })
        .await
        .unwrap();
    assert_eq!(steps.lock().unwrap().last().unwrap(), "renew");
    assert_eq!(
        viewer.sign_in_renew("someone else").await.unwrap_err().code,
        Code::LeaseLost
    );
    viewer.release(Some(&lease)).await.ok();
}

#[tokio::test]
async fn any_end_of_the_lease_ends_the_sign_in() {
    let directory = tempfile::tempdir().unwrap();
    let (viewer, helper) = viewer_of(directory.path());
    let steps = Mutex::new(Vec::new());
    started(&viewer, &helper, &steps, HoldKind::Reset, Fails::Nothing)
        .await
        .unwrap();
    assert!(viewer.client().controller.private_hold().is_some());

    viewer.lease.lock().await.expire_for_test();
    helper
        .serve(viewer.end_expired(), |request| {
            step(&steps, Fails::Nothing, request)
        })
        .await;

    nothing_left(&viewer).await;
}

#[tokio::test]
async fn a_second_sign_in_is_refused_while_one_is_under_way() {
    let directory = tempfile::tempdir().unwrap();
    let (viewer, helper) = viewer_of(directory.path());
    let steps = Mutex::new(Vec::new());
    let reply = started(&viewer, &helper, &steps, HoldKind::SignIn, Fails::Nothing)
        .await
        .unwrap();

    let refused = started(&viewer, &helper, &steps, HoldKind::SignIn, Fails::Nothing)
        .await
        .unwrap_err();

    assert_eq!(refused.code, Code::Busy);
    viewer
        .release(Some(reply["lease"].as_str().unwrap()))
        .await
        .ok();
}

#[tokio::test]
async fn the_status_says_which_site_the_person_signs_in_to() {
    let directory = tempfile::tempdir().unwrap();
    let (viewer, helper) = viewer_of(directory.path());
    let steps = Mutex::new(Vec::new());
    let reply = started(&viewer, &helper, &steps, HoldKind::SignIn, Fails::Nothing)
        .await
        .unwrap();
    let hold = viewer.client().controller.private_hold();
    assert_eq!(json!(hold), json!({"site": "github.com", "kind": "signIn"}));
    viewer
        .release(Some(reply["lease"].as_str().unwrap()))
        .await
        .ok();
    assert_eq!(viewer.client().controller.private_hold(), None);
}

#[tokio::test]
async fn done_adds_only_the_site_signed_in_to_whatever_else_was_typed() {
    let directory = tempfile::tempdir().unwrap();
    let (viewer, helper) = viewer_of(directory.path());
    let steps = Mutex::new(Vec::new());
    let reply = started(&viewer, &helper, &steps, HoldKind::SignIn, Fails::Nothing)
        .await
        .unwrap();
    let lease = reply["lease"].as_str().unwrap().to_owned();

    helper
        .serve(viewer.sign_in_done(&lease), |request| {
            step(&steps, Fails::Typed, request)
        })
        .await
        .unwrap();

    let Listed::Accounts(accounts) = sign_ins::listed_in(&directory.path().join("home")) else {
        panic!("the list can be read");
    };
    let ids: Vec<_> = accounts.iter().map(|account| account.id.as_str()).collect();
    assert_eq!(ids, ["github.com"]);
}

#[tokio::test]
async fn a_frame_read_during_a_sign_in_is_never_shown_from_the_cache_to_anyone_but_the_app() {
    let directory = tempfile::tempdir().unwrap();
    let (viewer, helper) = viewer_of(directory.path());
    let client = viewer.client();
    let steps = Mutex::new(Vec::new());
    let epoch = helper
        .serve(client.identity(), |request| {
            step(&steps, Fails::Nothing, request)
        })
        .await
        .unwrap()
        .epoch;
    let frame = || atlas_core::screen::frame::Frame {
        png: Arc::from(&b"the password page"[..]),
        width: 1,
        height: 1,
        epoch: epoch.clone(),
        at: std::time::Instant::now(),
    };

    // Read while the screen was hidden, and still recent once it is shown.
    viewer
        .frames
        .lock()
        .await
        .keep_for_test("private", frame(), None);
    let refused = helper
        .serve(viewer.shown_frame(), |request| {
            step(&steps, Fails::Nothing, request)
        })
        .await
        .err()
        .expect("refused");
    assert_eq!(refused.code, Code::Busy);
    let (key, _) = helper
        .serve(viewer.frame(), |request| {
            step(&steps, Fails::Nothing, request)
        })
        .await
        .unwrap();
    assert_eq!(key, "private", "the app is shown it");

    let visible = client.controller.screen_visible().unwrap();
    viewer
        .frames
        .lock()
        .await
        .keep_for_test("shown", frame(), Some(visible));
    let (key, _) = helper
        .serve(viewer.shown_frame(), |request| {
            step(&steps, Fails::Nothing, request)
        })
        .await
        .unwrap();
    assert_eq!(key, "shown");
}

#[tokio::test]
async fn a_launch_the_lease_runs_out_during_ends_the_sign_in_and_cleans_up() {
    let directory = tempfile::tempdir().unwrap();
    let (viewer, helper) = viewer_of(directory.path());
    let steps = Mutex::new(Vec::new());
    let body = json!({"site": "github.com", "browser": "com.google.Chrome"});

    let refused = helper
        .serve(viewer.sign_in_start(HoldKind::SignIn, &body), |request| {
            let answer = step(&steps, Fails::Nothing, request);
            // The browser opens just as the person's lease runs out.
            if steps.lock().unwrap().last().unwrap().starts_with("open ") {
                viewer.lease.try_lock().unwrap().expire_for_test();
            }
            answer
        })
        .await
        .unwrap_err();

    assert_eq!(refused.code, Code::LeaseLost);
    assert_eq!(
        refused.error,
        "The sign-in ended because control ended. Start it again."
    );
    assert_eq!(steps.lock().unwrap().last().unwrap(), "release");
    nothing_left(&viewer).await;
}

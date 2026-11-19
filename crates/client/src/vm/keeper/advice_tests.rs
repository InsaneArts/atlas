//! What a VM whose guest does not serve what is asked is told to do next,
//! and what its status says of updating it, through a real keeper and an
//! agent's client of it: judged from the guest's build beside this Mac's,
//! with nothing sent for it.
use super::*;
use crate::vm::keeper::{
    change::tests::{admitted, live, proven, restarted},
    clients::tests::{asked, client_of, serving_with},
    tests::{input_hello, until},
};
use atlas_core::files::Token;
use atlas_session::compatibility::{ENDED, INSTALLED, MACHINE, UPDATES};
use std::sync::atomic::Ordering::SeqCst;

/// The build the guest of `keeper`'s live generation reported, once read
/// in that generation.
fn reported_build(keeper: &Keeper) -> Option<String> {
    let state = keeper.shared.state();
    let report = state.reported(state.generation?)?;
    report["build"].as_str().map(str::to_owned)
}

/// What the guest of a command that needs `INSTALLED` is refused with, the
/// VM proved first.
async fn refused_a_listing(client: &crate::client::Client) -> Error {
    let verified = client.verify_vm().await.unwrap();
    client
        .generation_offering(&verified, INSTALLED, |name| {
            format!("{name}'s Atlas cannot list its apps.")
        })
        .unwrap_err()
}

/// A guest's daemon declaring every machine capability but `INSTALLED`.
fn lists_no_apps(hello: &mut Value) {
    let declared: Vec<&str> = MACHINE
        .into_iter()
        .filter(|name| *name != INSTALLED)
        .collect();
    hello["daemon"]["capabilities"] = json!(declared);
}

/// Its daemon, of the build `build`.
fn of_build(hello: &mut Value, build: &str) {
    hello["daemon"]["build"] = json!(build);
}

/// The input service's hello of the build `build`.
fn input_of(build: &str) -> Value {
    let mut hello = input_hello();
    hello["build"] = json!(build);
    hello
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_guest_missing_a_capability_is_told_to_run_atlas_update_when_its_build_is_older() {
    let (_serving, saying, client, _directory) = proven().await;
    *saying.hello.lock().unwrap() = Some(|hello| {
        lists_no_apps(hello);
        of_build(hello, "old");
        hello["daemon"]["version"] = json!("0.0.1");
    });
    *saying.input.lock().unwrap() = Some(input_of("old"));

    let refused = refused_a_listing(&client).await;

    assert_eq!(refused.code, Code::Update);
    assert_eq!(refused.error, "work's Atlas cannot list its apps.");
    assert_eq!(
        refused.details["next"],
        "Run atlas update to bring work's Atlas to this Mac's build."
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_guest_newer_than_this_mac_is_told_to_update_atlas_on_this_mac() {
    let (_serving, saying, client, _directory) = proven().await;
    *saying.hello.lock().unwrap() = Some(|hello| {
        lists_no_apps(hello);
        of_build(hello, "new");
        hello["daemon"]["version"] = json!("99.0.0");
    });
    *saying.input.lock().unwrap() = Some(input_of("new"));

    let refused = refused_a_listing(&client).await;
    let status = client.status().await;

    assert_eq!(refused.details["next"], "Update Atlas on this Mac.");
    assert_eq!(
        status["guest"]["update"],
        json!({"state": "newer", "next": "Update Atlas on this Mac."}),
        "{status}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_guest_on_this_build_missing_a_capability_is_sent_to_doctor() {
    // Of the keeper's own build, `b1`, whatever version it says.
    let (_serving, saying, client, _directory) = proven().await;
    *saying.hello.lock().unwrap() = Some(|hello| {
        lists_no_apps(hello);
        hello["daemon"]["version"] = json!("0.0.1");
    });

    let refused = refused_a_listing(&client).await;

    assert_eq!(
        refused.details["next"], "Run atlas doctor --desktop work.",
        "the same build, whatever its version says"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_guest_that_takes_no_update_is_told_to_remove_the_vm_and_add_it_again() {
    let (_serving, saying, client, _directory) = proven().await;
    *saying.hello.lock().unwrap() = Some(|hello| {
        let declared: Vec<&str> = MACHINE
            .into_iter()
            .filter(|name| *name != INSTALLED && *name != UPDATES)
            .collect();
        hello["daemon"]["capabilities"] = json!(declared);
        of_build(hello, "old");
        hello["daemon"]["version"] = json!("0.0.1");
    });
    *saying.input.lock().unwrap() = Some(input_of("old"));

    let refused = refused_a_listing(&client).await;

    assert_eq!(
        refused.details["next"],
        "work's Atlas cannot be updated from this Mac. Remove the VM and add it again."
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_guest_whose_build_is_not_known_is_judged_by_its_version_alone() {
    let (_serving, saying, client, _directory) = proven().await;
    *saying.hello.lock().unwrap() = Some(|hello| {
        lists_no_apps(hello);
        hello["daemon"].as_object_mut().unwrap().remove("build");
        hello["daemon"]["version"] = json!(atlas_core::VERSION);
    });
    let same_version = refused_a_listing(&client).await;
    *saying.hello.lock().unwrap() = Some(|hello| {
        lists_no_apps(hello);
        hello["daemon"].as_object_mut().unwrap().remove("build");
        hello["daemon"]["version"] = json!("0.0.1");
    });
    let older = refused_a_listing(&client).await;

    assert_eq!(
        same_version.details["next"], "Run atlas doctor --desktop work.",
        "it may be this very build"
    );
    assert_eq!(
        older.details["next"],
        "Run atlas update to bring work's Atlas to this Mac's build."
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_guest_whose_version_is_not_known_is_sent_to_doctor() {
    let (_serving, saying, client, _directory) = proven().await;
    *saying.hello.lock().unwrap() = Some(|hello| {
        lists_no_apps(hello);
        of_build(hello, "other");
        hello["daemon"].as_object_mut().unwrap().remove("version");
    });
    *saying.input.lock().unwrap() = Some(input_of("other"));

    let refused = refused_a_listing(&client).await;

    assert_eq!(refused.details["next"], "Run atlas doctor --desktop work.");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn status_says_an_update_is_available_when_the_guests_build_is_older() {
    let (serving, saying, client, _directory) = proven().await;
    // Read again in a new generation, so its report says its build.
    restarted(&serving).await;
    until("the guest's build reported", || {
        reported_build(&serving.fixture.keeper).as_deref() == Some("b1")
    })
    .await;
    let hello = serving.fixture.keeper.hello();
    let current = client.status().await;
    *saying.hello.lock().unwrap() = Some(|hello| {
        of_build(hello, "old");
        hello["daemon"]["version"] = json!("0.0.1");
    });
    *saying.input.lock().unwrap() = Some(input_of("old"));

    let older = client.status().await;

    assert_eq!(hello["guest"]["build"], "b1", "{hello}");
    assert_eq!(hello["program"]["build"], "b1", "{hello}");
    assert_eq!(
        current["guest"]["update"],
        json!({"state": "current"}),
        "{current}"
    );
    assert_eq!(current["ready"], true, "{current}");
    assert_eq!(
        older["guest"]["update"],
        json!({"state": "available",
            "next": "Run atlas update to bring work's Atlas to this Mac's build."}),
        "{older}"
    );
    assert!(
        !older.to_string().to_lowercase().contains("helper"),
        "{older}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_keepers_own_refusals_take_their_next_step_from_the_guests_build() {
    let serving = serving_with(true, true).await;
    serving.fixture.guest.proving();
    // Read again in a new generation, so its report says its build.
    restarted(&serving).await;
    until("the guest's build reported", || {
        reported_build(&serving.fixture.keeper).as_deref() == Some("b1")
    })
    .await;
    let opened = admitted(&serving).len();
    let directory = tempfile::tempdir().unwrap();
    let client = client_of(&serving, directory.path());
    let change = Token::new();
    let doctor = "Run atlas doctor --desktop work.";

    let changed = asked(
        &serving.socket,
        &json!({"op": "change", "lane": "change", "change": change,
            "generation": live(&serving),
            "request": {"op": "run", "argv": ["/bin/echo"], "change": change}}),
    )
    .await;
    let observed = asked(
        &serving.socket,
        &json!({"op": "observe", "challenge": "challenge"}),
    )
    .await;
    let unproven = client.verify_vm().await.unwrap_err();

    assert_eq!(changed[0]["code"], "update", "{changed:?}");
    assert_eq!(changed[0]["sent"], false);
    assert_eq!(changed[0]["next"], doctor, "{changed:?}");
    assert_eq!(observed[0]["code"], "update", "{observed:?}");
    assert_eq!(
        observed[0]["error"],
        "work's Atlas does not say what this Mac proves it by."
    );
    assert_eq!(observed[0]["next"], doctor);
    assert_eq!(unproven.code, Code::Update, "{unproven:?}");
    assert_eq!(unproven.details["next"], doctor, "kept by the client");
    assert_eq!(admitted(&serving).len(), opened, "nothing was sent");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_command_judges_the_guest_beside_the_keepers_build_never_its_own() {
    // This command is of another build than the keeper and the guest,
    // which are both of `b1`, as after a handover a long-lived client
    // outlived.
    let (_serving, saying, client, _directory) = proven().await;
    assert_ne!(
        crate::vm::update::Build::own().digest.as_deref(),
        Some("b1")
    );
    *saying.hello.lock().unwrap() = Some(lists_no_apps);

    let refused = refused_a_listing(&client).await;

    assert_eq!(
        refused.details["next"], "Run atlas doctor --desktop work.",
        "neither this Mac nor the guest is told to update"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_command_whose_keeper_says_nothing_of_its_build_judges_the_guest_beside_its_own() {
    let directory = tempfile::Builder::new().tempdir_in("/tmp").unwrap();
    let (client, keeper) = crate::test_support::vm_through_keeper(directory.path());
    *keeper.observing.lock().unwrap() = Some(|answer| {
        answer.as_object_mut().unwrap().remove("keeper");
        let hello = &mut answer["status"]["hello"];
        lists_no_apps(hello);
        let own = crate::vm::update::Build::own();
        hello["daemon"]["build"] = json!(own.digest);
        hello["daemon"]["version"] = json!("0.0.1");
    });

    let refused = refused_a_listing(&client).await;

    assert_eq!(
        refused.details["next"], "Run atlas doctor --desktop work.",
        "this command's own build"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_reconnected_guest_whose_new_build_is_not_known_is_never_said_current_from_the_last_report()
 {
    let (serving, saying, client, _directory) = proven().await;
    let keeper = &serving.fixture.keeper;
    restarted(&serving).await;
    until("the guest's build reported", || {
        reported_build(keeper).as_deref() == Some("b1")
    })
    .await;
    let last = keeper.shared.state().report.clone();
    // The guest comes back of a build it does not say, of this keeper's
    // version.
    *saying.hello.lock().unwrap() = Some(|hello| {
        hello["daemon"].as_object_mut().unwrap().remove("build");
        hello["daemon"]["version"] = json!(atlas_core::VERSION);
    });
    restarted(&serving).await;
    // As while the new generation's report is still being read: the last
    // generation's is all the keeper holds.
    keeper.shared.state().report = last;
    let session = keeper.shared.live().unwrap();

    let hello = keeper.hello();
    let facts = keeper.update_facts(&session);
    let status = client.status().await;

    assert_eq!(hello["guest"]["build"], Value::Null, "{hello}");
    assert_eq!(facts.guest.digest, None, "{facts:?}");
    assert_eq!(
        crate::vm::update::said("work", &facts),
        json!({"state": "unknown"})
    );
    assert_eq!(
        status["guest"]["update"],
        json!({"state": "unknown"}),
        "{status}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_guest_on_this_build_missing_a_capability_is_sent_from_the_command_to_doctor_and_from_doctor_to_a_restart()
 {
    let (serving, saying, client, _directory) = proven().await;
    // A guest of the keeper's own build that serves no proof of ended work,
    // in its control hello and its own.
    serving.fixture.guest.standin.unended.store(true, SeqCst);
    *saying.hello.lock().unwrap() = Some(|hello| {
        let declared: Vec<&str> = MACHINE.into_iter().filter(|name| *name != ENDED).collect();
        hello["daemon"]["capabilities"] = json!(declared);
    });
    restarted(&serving).await;
    let restart = "Restart work with atlas desktop maintain work --restart; if it stays, remove it \
                   and add it again.";

    let verified = client.verify_vm().await.unwrap();
    let refused = client
        .generation_offering(&verified, ENDED, |name| {
            format!("{name}'s Atlas cannot prove its work ended.")
        })
        .unwrap_err();
    let status = client.status().await;
    let report = crate::capability::vm_report(status.clone(), client.unreachable());

    assert_eq!(refused.details["next"], "Run atlas doctor --desktop work.");
    assert_eq!(
        status["guest"]["update"],
        json!({"state": "current"}),
        "{status}"
    );
    assert_eq!(status["ready"], false, "{status}");
    let served = report["checks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|check| check["name"] == "Atlas in it serves this build")
        .unwrap();
    assert_eq!(served["ok"], false, "{report}");
    assert_eq!(
        served["nextStep"],
        format!("work's Atlas does not serve all this build needs (ended-v1). {restart}")
    );
    let steps = report["nextSteps"].to_string();
    assert!(steps.contains(restart), "{report}");
    assert!(
        !steps.contains("atlas doctor"),
        "doctor never points at itself: {report}"
    );
}

//! The person's control of a VM through the viewer: taken beside no agent's
//! command, their input sent under the owner their lease holds, kept and
//! given back through the VM's keeper, the app's alone, whatever Stop holds
//! agents.
use super::super::{Viewer, http, two_desktops::post, unpoisoned};
use atlas_client::{
    client::Client,
    test_support::{GENERATION, ScriptedKeeper, kept_capability, vm_through_keeper},
};
use atlas_core::screen::frame::Frame;
use serde_json::{Value, json};
use std::sync::{Arc, atomic::Ordering::SeqCst};

/// The key the frame the person acts on is kept under.
const FRAME: &str = "shown";

/// What the keeper says of a change whose final reply never came.
fn lost() -> Value {
    json!({"ok": false, "code": "transport", "uncertain": true, "sent": true,
        "error": "The change's lane to the guest ended before its final reply."})
}

/// A viewer of the scripted VM `work`, its station keeping the keeper's
/// capability for the person, and the frame of the VM it shows kept.
struct OnVm {
    viewer: Arc<Viewer>,
    keeper: ScriptedKeeper,
    client: Client,
    capability: String,
    _vm: tempfile::TempDir,
    _home: tempfile::TempDir,
}

impl OnVm {
    async fn new() -> Self {
        let vm = tempfile::Builder::new().tempdir_in("/tmp").unwrap();
        let home = tempfile::tempdir().unwrap();
        let (client, keeper) = vm_through_keeper(vm.path());
        let capability = kept_capability(home.path(), &client.desktop().id());
        let viewer = http::configured(client.clone(), "127.0.0.1:0".to_owned(), None);
        *unpoisoned(&viewer.station_home) = Some(home.path().to_owned());
        let shown = Self {
            viewer,
            keeper,
            client,
            capability,
            _vm: vm,
            _home: home,
        };
        shown.shows_a_frame().await;
        shown
    }

    /// Keep a fresh frame of the VM as its proof shows it now.
    async fn shows_a_frame(&self) {
        let proof = self.viewer.client().person_proof_on_vm().await.unwrap();
        let frame = Frame {
            png: Arc::from(&b"a frame of work"[..]),
            width: 1,
            height: 1,
            epoch: proof.epoch().clone(),
            at: std::time::Instant::now(),
        };
        let mut frames = self.viewer.frames.lock().await;
        frames.items.clear();
        frames.items.push_back((FRAME.to_owned(), Arc::new(frame)));
    }

    /// Take control as the app does: the lease's token.
    async fn taken(&self) -> String {
        let (status, taken) = post(
            &self.viewer,
            "control/acquire",
            &json!({"frame": FRAME}),
            true,
        )
        .await;
        assert_eq!(status, 200, "{taken}");
        taken["lease"].as_str().unwrap().to_owned()
    }

    /// The changes to the guest's input the keeper was asked to carry, each
    /// on the person's own lane.
    fn input_changes(&self) -> Vec<Value> {
        let changes: Vec<Value> = self
            .keeper
            .asked_for("change")
            .into_iter()
            .filter(|asked| matches!(asked["lane"].as_str(), Some("input" | "person")))
            .collect();
        assert!(
            changes.iter().all(|asked| asked["lane"] == "person"),
            "{changes:?}"
        );
        changes
    }

    /// The input operations the keeper was asked to carry, in order.
    fn ops(&self) -> Vec<String> {
        self.input_changes()
            .iter()
            .map(|asked| asked["request"]["op"].as_str().unwrap().to_owned())
            .collect()
    }

    fn answering(&self, op: &'static str, answer: Value) {
        self.keeper.input_answers.lock().unwrap().push((op, answer));
    }

    fn pending(&self) -> Vec<String> {
        let mut pending = self.client.controller.pending().unwrap();
        pending.sort();
        pending
    }

    async fn active(&self) -> bool {
        self.viewer.lease.lock().await.active()
    }
}

/// A click the person sends.
fn click() -> Value {
    json!([{"type": "click", "x": 10, "y": 10}])
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_person_takes_control_of_a_vm_only_beside_no_agent_command() {
    let vm = OnVm::new().await;
    let frame = json!({"frame": FRAME});
    let take = || post(&vm.viewer, "control/acquire", &frame, true);
    // An agent acting now; then an agent's command still running once its
    // request went, its control let go of and its claim kept.
    let acting = vm.client.controller.acquire_beside_commands().unwrap();
    let (beside_acting, acting_refused) = take().await;
    let running = vm
        .client
        .controller
        .quarantine_command_by(
            &atlas_core::files::token(),
            atlas_control::executors::Executor::Guest,
        )
        .unwrap();
    drop(acting);
    let (beside_running, running_refused) = take().await;

    for (status, refused) in [
        (beside_acting, &acting_refused),
        (beside_running, &running_refused),
    ] {
        assert_eq!(status, 409, "{refused}");
        assert_eq!(refused["code"], "busy", "{refused}");
    }
    assert!(vm.input_changes().is_empty(), "nothing asked of the guest");
    assert!(!vm.active().await);
    vm.client.controller.clear_if(running.generation()).unwrap();
    drop(running);

    let lease = vm.taken().await;

    assert_eq!(vm.ops(), ["acquire"]);
    assert_eq!(
        vm.pending(),
        std::slice::from_ref(&lease),
        "the owner's claim"
    );
    assert!(
        vm.client.controller.acquire_beside_commands().is_err(),
        "no agent's command goes on beside it"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_persons_input_goes_under_their_lease_owner_in_the_proofs_generation() {
    let vm = OnVm::new().await;
    let lease = vm.taken().await;

    let (sent, input) = post(
        &vm.viewer,
        "input",
        &json!({"lease": lease, "frame": FRAME, "actions": click()}),
        true,
    )
    .await;
    let (kept, renewed) = post(
        &vm.viewer,
        "control/renew",
        &json!({"lease": lease, "frame": FRAME}),
        true,
    )
    .await;
    let (given, released) = post(
        &vm.viewer,
        "control/release",
        &json!({"lease": lease}),
        true,
    )
    .await;

    assert_eq!(
        (sent, kept, given),
        (200, 200, 200),
        "{input} {renewed} {released}"
    );
    assert_eq!(input["performed"], 1, "{input}");
    assert_eq!(vm.ops(), ["acquire", "input", "renew", "release"]);
    for asked in vm.input_changes() {
        assert_eq!(asked["generation"], GENERATION, "{asked}");
        assert_eq!(asked["capability"], vm.capability, "{asked}");
        assert_eq!(asked["request"]["owner"], lease, "{asked}");
        assert_eq!(asked["request"]["person"], true, "{asked}");
    }
    assert!(vm.pending().is_empty(), "let go of");
    assert!(!vm.active().await);
    let rows = std::fs::read_to_string(
        vm.client
            .output_dir()
            .unwrap()
            .join(atlas_client::activity_log::FILE),
    )
    .unwrap();
    let rows: Vec<Value> = rows
        .lines()
        .map(|row| serde_json::from_str(row).unwrap())
        .collect();
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0]["operation"], "input");
    assert_eq!(rows[0]["actor"], "viewer");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_new_vm_generation_ends_the_persons_control() {
    let vm = OnVm::new().await;
    let lease = vm.taken().await;
    vm.keeper.generation.store(GENERATION + 1, SeqCst);

    let (status, refused) = post(
        &vm.viewer,
        "control/renew",
        &json!({"lease": lease, "frame": FRAME}),
        true,
    )
    .await;
    let (_, input) = post(
        &vm.viewer,
        "input",
        &json!({"lease": lease, "frame": FRAME, "actions": click()}),
        true,
    )
    .await;

    assert_eq!(status, 409, "{refused}");
    assert_eq!(refused["code"], "session-changed", "{refused}");
    assert!(!vm.active().await, "control ended");
    assert_eq!(input["code"], "lease-lost", "{input}");
    assert_eq!(vm.ops(), ["acquire", "renew", "release"]);
    assert!(
        vm.input_changes()
            .iter()
            .all(|asked| asked["generation"] == GENERATION)
    );
    assert_eq!(
        vm.pending(),
        [lease],
        "its owner, for the keeper's recovery to prove let go of"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_lost_release_on_a_vm_stays_pending_and_agents_wait() {
    let vm = OnVm::new().await;
    let lease = vm.taken().await;
    vm.answering("release", lost());

    let (status, unconfirmed) = post(
        &vm.viewer,
        "control/release",
        &json!({"lease": lease}),
        true,
    )
    .await;

    assert_eq!(status, 409, "{unconfirmed}");
    assert_eq!(unconfirmed["code"], "uncertain", "{unconfirmed}");
    assert_eq!(unconfirmed["uncertain"], true, "{unconfirmed}");
    assert!(!unconfirmed.to_string().contains("helper"), "{unconfirmed}");
    assert!(!vm.active().await, "control ended");
    let release = vm.input_changes()[1]["change"].as_str().unwrap().to_owned();
    let mut kept = vec![lease, release];
    kept.sort();
    assert_eq!(vm.pending(), kept, "the owner and its release");
    assert!(
        vm.client.controller.acquire_beside_commands().is_err(),
        "agents wait"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn revoke_lets_go_of_the_guest_owner() {
    let vm = OnVm::new().await;
    let lease = vm.taken().await;

    let (status, revoked) = post(&vm.viewer, "control/revoke", &json!({}), true).await;

    assert_eq!(status, 200, "{revoked}");
    assert_eq!(revoked["released"], true, "{revoked}");
    assert_eq!(vm.ops(), ["acquire", "release"]);
    assert_eq!(vm.input_changes()[1]["request"]["owner"], lease);
    assert!(vm.pending().is_empty(), "let go of");
    assert!(
        vm.keeper
            .asked_for("settle")
            .iter()
            .any(|settled| settled["changes"]
                .as_array()
                .unwrap()
                .contains(&json!(lease))),
        "the guest told it may forget the owner"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_lost_acquire_reply_lets_go_of_what_it_may_have_taken_and_stays_pending() {
    let vm = OnVm::new().await;
    vm.answering("acquire", lost());

    let (status, refused) = post(
        &vm.viewer,
        "control/acquire",
        &json!({"frame": FRAME}),
        true,
    )
    .await;

    assert_eq!(status, 409, "{refused}");
    assert_eq!(refused["uncertain"], true, "{refused}");
    assert!(!vm.active().await);
    assert_eq!(vm.ops(), ["acquire", "release"]);
    let changes = vm.input_changes();
    let owner = &changes[0]["request"]["owner"];
    assert_eq!(
        &changes[1]["request"]["owner"], owner,
        "its owner let go of"
    );
    assert_eq!(
        vm.pending(),
        [changes[0]["change"].as_str().unwrap()],
        "the acquire alone, its owner let go of"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_lost_renew_reply_ends_the_persons_control_and_stays_pending() {
    let vm = OnVm::new().await;
    let lease = vm.taken().await;
    vm.answering("renew", lost());

    let (status, refused) = post(
        &vm.viewer,
        "control/renew",
        &json!({"lease": lease, "frame": FRAME}),
        true,
    )
    .await;

    assert_eq!(status, 409, "{refused}");
    assert_eq!(refused["uncertain"], true, "{refused}");
    assert!(!vm.active().await, "control ended");
    assert_eq!(vm.ops(), ["acquire", "renew", "release"]);
    assert_eq!(
        vm.pending(),
        [vm.input_changes()[1]["change"].as_str().unwrap()],
        "the renewal alone, its owner let go of"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_expired_guest_grant_ends_the_persons_control() {
    let vm = OnVm::new().await;
    let lease = vm.taken().await;
    vm.answering(
        "renew",
        json!({"ok": false, "code": "lease-lost", "uncertain": false,
            "error": "Native control was released or expired"}),
    );

    let (status, refused) = post(
        &vm.viewer,
        "control/renew",
        &json!({"lease": lease, "frame": FRAME}),
        true,
    )
    .await;

    assert_eq!(status, 409, "{refused}");
    assert_eq!(refused["code"], "lease-lost", "{refused}");
    assert!(!vm.active().await, "control ended");
    assert_eq!(vm.ops(), ["acquire", "renew", "release"]);
    assert!(vm.pending().is_empty(), "let go of, and settled");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_persons_control_of_a_vm_goes_on_while_agents_stay_stopped() {
    let vm = OnVm::new().await;
    *vm.keeper.stop.lock().unwrap() = "stopped";
    vm.client.controller.note_stop(true).unwrap();

    let lease = vm.taken().await;
    let status = vm.viewer.status_for(true).await;
    let (sent, input) = post(
        &vm.viewer,
        "input",
        &json!({"lease": lease, "frame": FRAME, "actions": click()}),
        true,
    )
    .await;
    let still = vm.active().await;
    let (given, released) = post(
        &vm.viewer,
        "control/release",
        &json!({"lease": lease}),
        true,
    )
    .await;

    assert_eq!(status["ready"], false, "agents are stopped: {status}");
    assert_eq!(status["controlActive"], true, "{status}");
    assert!(still, "a status read leaves their control as it is");
    assert_eq!((sent, given), (200, 200), "{input} {released}");
    assert_eq!(vm.ops(), ["acquire", "input", "release"]);
    assert!(vm.pending().is_empty());
    let revision = vm.client.controller.stop_revision().unwrap();
    vm.client.controller.lift_stop(revision.as_ref()).unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_persons_routes_of_a_vm_answer_the_app_alone() {
    let vm = OnVm::new().await;
    let lease = vm.taken().await;
    let asked = [
        ("control/acquire", json!({"frame": FRAME})),
        ("control/renew", json!({"lease": lease, "frame": FRAME})),
        (
            "input",
            json!({"lease": lease, "frame": FRAME, "actions": click()}),
        ),
        ("control/release", json!({"lease": lease})),
        ("recover", json!({})),
        ("cleanup", json!({"session": "x"})),
    ];

    for (route, body) in asked {
        let (status, refused) = post(&vm.viewer, route, &body, false).await;

        assert_eq!(status, 403, "{route}: {refused}");
        assert_eq!(refused["code"], "permission", "{route}: {refused}");
        assert_eq!(refused["error"], "Only the Atlas app can do this on work.");
    }
    assert_eq!(vm.ops(), ["acquire"], "nothing more asked of the guest");
    assert!(vm.active().await, "the person's control stays theirs");
}

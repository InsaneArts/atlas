//! Signing a VM in to an account through the viewer: its browser from the
//! guest's apps, its page opened by a change of the person's own, and its
//! screen hidden from agents until the guest let go of the person's input.
use super::super::super::{Viewer, http, two_desktops::post, unpoisoned};
use atlas_client::{
    client::Client,
    sign_ins::{self, Entry, Listed},
    test_support::{GENERATION, ScriptedKeeper, kept_capability, vm_through_keeper},
};
use atlas_control::{HoldKind, PrivateHold};
use atlas_core::{
    desktop::RecordKey,
    error::{Code, Error, Holder},
    screen::frame::Size,
};
use serde_json::{Value, json};
use std::{path::PathBuf, sync::Arc};

/// What the keeper says of a change whose final reply never came.
fn lost() -> Value {
    json!({"ok": false, "code": "transport", "uncertain": true, "sent": true,
        "error": "The change's lane to the guest ended before its final reply."})
}

/// The app's viewer of the scripted VM `work`, its station keeping the
/// keeper's capability for the person and the person's accounts in a home
/// of its own; nothing an agent started runs there, and Google Chrome is
/// installed in it.
struct OnVm {
    viewer: Arc<Viewer>,
    keeper: ScriptedKeeper,
    /// The VM as an agent's command reaches it.
    client: Client,
    home: PathBuf,
    _vm: tempfile::TempDir,
    _station: tempfile::TempDir,
}

impl OnVm {
    async fn new() -> Self {
        let vm = tempfile::Builder::new().tempdir_in("/tmp").unwrap();
        let station = tempfile::tempdir().unwrap();
        let (client, keeper) = vm_through_keeper(vm.path());
        let _capability = kept_capability(station.path(), &client.desktop().id());
        *keeper.leftovers.lock().unwrap() = "none";
        keeper.installed.lock().unwrap().push("Google Chrome");
        let home = station.path().join("home");
        std::fs::create_dir_all(&home).unwrap();
        let viewer = Self::service(&client, station.path().to_owned(), home.clone());
        Self {
            viewer,
            keeper,
            client,
            home,
            _vm: vm,
            _station: station,
        }
    }

    /// The app's service of `client`'s VM, as one starts: its station in
    /// `station`, the person's accounts in `home`.
    fn service(client: &Client, station: PathBuf, home: PathBuf) -> Arc<Viewer> {
        let viewer = http::configured(client.clone(), "127.0.0.1:0".to_owned(), None);
        *unpoisoned(&viewer.station_home) = Some(station);
        *unpoisoned(&viewer.accounts_home) = Some(home);
        let (lines, _) = tokio::sync::mpsc::unbounded_channel();
        *unpoisoned(&viewer.app) = Some(lines);
        viewer
    }

    /// Keep a fresh frame of the VM as its proof shows it now, under the
    /// key `shown`.
    async fn shows_a_frame(&self) {
        let proof = self.viewer.client().person_proof_on_vm().await.unwrap();
        let frame = atlas_core::screen::frame::Frame {
            png: Arc::from(&b"a frame of work"[..]),
            width: 1,
            height: 1,
            epoch: proof.epoch().clone(),
            at: std::time::Instant::now(),
        };
        self.viewer
            .frames
            .lock()
            .await
            .items
            .push_back(("shown".to_owned(), Arc::new(frame)));
    }

    /// Start signing in to `site` as the app does: the status and reply.
    async fn start(&self, site: &str) -> (u16, Value) {
        post(&self.viewer, "sign-in/start", &json!({"site": site}), true).await
    }

    /// Start signing in to GitHub, as the app does: the lease's token.
    async fn started(&self) -> String {
        let (status, started) = self.start("github.com").await;
        assert_eq!(status, 200, "{started}");
        started["lease"].as_str().unwrap().to_owned()
    }

    /// The changes of the person's own the keeper was asked to carry.
    fn persons(&self) -> Vec<Value> {
        self.keeper
            .asked_for("change")
            .into_iter()
            .filter(|asked| asked["lane"] == "person")
            .collect()
    }

    fn ops(&self) -> Vec<String> {
        self.persons()
            .iter()
            .map(|asked| asked["request"]["op"].as_str().unwrap().to_owned())
            .collect()
    }

    fn answering(&self, op: &'static str, answer: Value) {
        self.keeper.input_answers.lock().unwrap().push((op, answer));
    }

    /// What an agent's screenshot and its collect of a file get now.
    async fn agents_look(&self) -> [atlas_core::error::Result<Value>; 2] {
        let files = ["/Users/Shared/notes.txt".to_owned()];
        [
            self.client.capture(Size::Full, None, true).await,
            atlas_client::collect::collect(&self.client, &files, 1024).await,
        ]
    }

    /// The app's service dies holding what it held, as a killed one does,
    /// the marker of its sign-in naming a process that is gone, and starts
    /// again.
    fn restart(&mut self) {
        let station = unpoisoned(&self.viewer.station_home).clone().unwrap();
        let restarted = Self::service(&self.client, station, self.home.clone());
        let dead = std::mem::replace(&mut self.viewer, restarted);
        assert_eq!(Arc::strong_count(&dead), 1, "nothing else holds it");
        drop(dead);
        self.client.controller.interrupt_for_test(&github());
    }

    /// Settle every change left pending, as the keeper's recovery does once
    /// the guest proves each ended.
    fn recovered(&self) {
        for change in self.client.controller.pending().unwrap() {
            self.client.controller.clear_if(&change).unwrap();
        }
    }

    fn accounts(&self) -> Vec<Entry> {
        let Listed::Accounts(accounts) = sign_ins::listed_in(&self.home) else {
            panic!("the accounts could not be read");
        };
        accounts
    }
}

/// The record key of the scripted VM.
fn work_key(client: &Client) -> RecordKey {
    client.desktop().record_key()
}

/// That `refused` is an agent's look refused while the person signs `work`
/// in, in the VM's words.
fn hidden_while_signing_in(refused: &Error) {
    assert_eq!(refused.code, Code::Busy, "{refused:?}");
    assert_eq!(refused.holder, Some(Holder::Person), "{refused:?}");
    assert!(refused.error.contains("in work"), "{refused:?}");
    assert!(!refused.error.contains("helper"), "{refused:?}");
}

fn github() -> PrivateHold {
    PrivateHold {
        site: "github.com".to_owned(),
        kind: HoldKind::SignIn,
        browser: Some("com.google.Chrome".to_owned()),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_sign_in_on_a_vm_hides_its_screen_from_agents_until_done() {
    let vm = OnVm::new().await;

    let lease = vm.started().await;
    let looked = vm.agents_look().await;
    let opened = vm.persons()[1].clone();
    let (finished, done) = post(&vm.viewer, "sign-in/done", &json!({"lease": lease}), true).await;
    let after = vm.client.looking_identity().await;

    for refused in &looked {
        hidden_while_signing_in(refused.as_ref().unwrap_err());
    }
    assert_eq!(vm.ops(), ["acquire", "person-open", "release"]);
    assert_eq!(opened["generation"], GENERATION);
    let page = &opened["request"];
    assert_eq!(page["app"], "Google Chrome", "{page}");
    assert_eq!(page["url"], "https://github.com", "{page}");
    assert_eq!(
        page["lease"],
        json!(lease),
        "under the owner their lease holds"
    );
    assert_eq!(page["person"], true);
    assert_eq!((finished, &done["added"]), (200, &json!(true)), "{done}");
    let accounts = vm.accounts();
    assert_eq!(accounts.len(), 1);
    assert_eq!(accounts[0].desktop, Some(work_key(&vm.client)));
    assert!(
        after.is_ok(),
        "agents look again once it is done: {after:?}"
    );
    assert!(vm.client.controller.pending().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_vm_without_chrome_or_firefox_is_refused_in_plain_words() {
    let vm = OnVm::new().await;
    vm.keeper.installed.lock().unwrap().clear();

    let (status, refused) = vm.start("github.com").await;

    assert_eq!(status, 409, "{refused}");
    assert_eq!(refused["code"], "apps", "{refused}");
    assert_eq!(
        refused["error"],
        "Install Google Chrome or Firefox in work to sign it in to a site"
    );
    assert!(vm.ops().is_empty(), "nothing was taken");
    assert!(!vm.viewer.lease.lock().await.active());
    assert!(vm.client.controller.screen_visible().is_ok());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_lost_release_keeps_a_vm_sign_in_private_until_recovery_proves_it_let_go() {
    let vm = OnVm::new().await;
    let lease = vm.started().await;
    vm.answering("release", lost());

    let (_, done) = post(&vm.viewer, "sign-in/done", &json!({"lease": lease}), true).await;
    let ended = !vm.viewer.lease.lock().await.active();
    let still = vm.agents_look().await;
    vm.viewer.status_for(true).await;
    let after_a_read = vm.client.controller.screen_visible();
    vm.recovered();
    vm.viewer.status_for(true).await;

    assert_eq!(done["uncertain"], true, "{done}");
    assert!(ended, "the person's control ended");
    for refused in &still {
        hidden_while_signing_in(refused.as_ref().unwrap_err());
    }
    hidden_while_signing_in(&after_a_read.unwrap_err());
    assert!(vm.client.controller.screen_visible().is_ok());
    assert_eq!(vm.accounts().len(), 1, "Done still lists the account");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn input_whose_outcome_is_unknown_keeps_a_vm_sign_in_private_past_its_release() {
    let vm = OnVm::new().await;
    let lease = vm.started().await;
    vm.answering("input", lost());
    vm.shows_a_frame().await;

    let (_, sent) = post(
        &vm.viewer,
        "input",
        &json!({"lease": lease, "frame": "shown",
            "actions": [{"type": "click", "x": 10, "y": 10}]}),
        true,
    )
    .await;
    let ended = !vm.viewer.lease.lock().await.active();
    let kept = vm.client.controller.screen_visible();
    vm.recovered();
    vm.viewer.status_for(true).await;

    assert_eq!(sent["uncertain"], true, "{sent}");
    assert!(
        ended,
        "an input of unknown outcome ends the person's control"
    );
    assert_eq!(vm.ops(), ["acquire", "person-open", "input", "release"]);
    hidden_while_signing_in(&kept.unwrap_err());
    assert!(vm.client.controller.screen_visible().is_ok());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_lapsed_lease_keeps_a_vm_sign_in_private_while_its_release_is_unanswered() {
    let vm = OnVm::new().await;
    vm.started().await;
    vm.answering("release", lost());

    vm.viewer.lease.lock().await.expire_for_test();
    vm.viewer.end_expired().await;
    vm.viewer.status_for(true).await;

    assert!(!vm.viewer.lease.lock().await.active());
    assert_eq!(vm.ops(), ["acquire", "person-open", "release"]);
    hidden_while_signing_in(&vm.client.controller.screen_visible().unwrap_err());
    for refused in &vm.agents_look().await {
        hidden_while_signing_in(refused.as_ref().unwrap_err());
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_interrupted_vm_sign_in_stays_private_after_a_service_restart() {
    let mut vm = OnVm::new().await;
    vm.started().await;
    // Its guest owner's claim stays pending.
    vm.restart();
    let restarted = vm.viewer.clone();

    let before = vm.agents_look().await;
    let shown = restarted.status_for(true).await;
    let id = &shown["overview"]["signInInterrupted"]["id"];
    let (waiting, refused) = post(
        &restarted,
        "sign-in/interrupted",
        &json!({"done": true, "id": id}),
        true,
    )
    .await;
    let still = vm.agents_look().await;

    for refused in &before {
        let refused = refused.as_ref().unwrap_err();
        assert_eq!(refused.code, Code::Busy, "{refused:?}");
        assert_eq!(
            refused.error,
            "A sign-in in work was interrupted; the screen stays hidden and changes wait until \
             the person finishes or cancels it in Atlas."
        );
    }
    // Taken over, it is held as a live one until the person ends it.
    for refused in &still {
        hidden_while_signing_in(refused.as_ref().unwrap_err());
    }
    assert!(id.is_string(), "{shown}");
    assert_eq!(waiting, 409, "{refused}");
    assert_eq!(
        refused["error"],
        "work is still letting go of the sign-in's control; try again in a moment"
    );
    assert!(vm.accounts().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_interrupted_vm_sign_in_finished_after_a_restart_names_its_desktop() {
    let mut vm = OnVm::new().await;
    vm.started().await;
    vm.restart();
    let restarted = vm.viewer.clone();
    let shown = restarted.status_for(true).await;
    vm.recovered();

    let (finished, done) = post(
        &restarted,
        "sign-in/interrupted",
        &json!({"done": true, "id": shown["overview"]["signInInterrupted"]["id"]}),
        true,
    )
    .await;

    assert_eq!((finished, &done["added"]), (200, &json!(true)), "{done}");
    let accounts = vm.accounts();
    assert_eq!(accounts.len(), 1);
    assert_eq!(accounts[0].id, "github.com");
    assert_eq!(accounts[0].desktop, Some(work_key(&vm.client)));
    assert!(vm.client.controller.screen_visible().is_ok());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_lost_page_open_stays_pending_and_keeps_the_vm_private_until_it_is_answered() {
    let vm = OnVm::new().await;
    vm.answering("person-open", lost());

    let (status, refused) = vm.start("github.com").await;
    let pending = vm.client.controller.pending().unwrap();
    let kept = vm.client.controller.screen_visible();
    vm.recovered();
    vm.viewer.status_for(true).await;

    assert_eq!(status, 409, "{refused}");
    assert_eq!(refused["uncertain"], true, "{refused}");
    assert_eq!(vm.ops(), ["acquire", "person-open", "release"]);
    assert_eq!(pending.len(), 1, "only the page's change: {pending:?}");
    assert!(!vm.viewer.lease.lock().await.active());
    hidden_while_signing_in(&kept.unwrap_err());
    assert!(vm.client.controller.screen_visible().is_ok());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_sign_in_on_a_stopped_vm_goes_on_and_agents_stay_stopped() {
    let vm = OnVm::new().await;
    *vm.keeper.stop.lock().unwrap() = "stopped";
    vm.client.controller.note_stop(true).unwrap();

    let lease = vm.started().await;
    let (finished, done) = post(&vm.viewer, "sign-in/done", &json!({"lease": lease}), true).await;

    assert_eq!(finished, 200, "{done}");
    assert_eq!(vm.ops(), ["acquire", "person-open", "release"]);
    let revision = vm.client.controller.stop_revision().unwrap();
    assert!(revision.is_some(), "agents stay stopped");
    vm.client.controller.lift_stop(revision.as_ref()).unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_same_site_and_browser_on_both_desktops_are_two_accounts() {
    let vm = OnVm::new().await;
    let on_second_login = super::super::super::accounts::add_signed_in(
        &Some(vm.home.clone()),
        "github.com",
        "com.google.Chrome",
        None,
    )
    .unwrap();

    let lease = vm.started().await;
    let (_, done) = post(&vm.viewer, "sign-in/done", &json!({"lease": lease}), true).await;

    assert!(on_second_login);
    assert_eq!(done["added"], true, "{done}");
    let mut desktops: Vec<Option<RecordKey>> = vm
        .accounts()
        .into_iter()
        .filter(|entry| entry.id == "github.com")
        .map(|entry| entry.desktop)
        .collect();
    desktops.sort_by_key(|desktop| desktop.as_ref().map(ToString::to_string));
    assert_eq!(desktops, [None, Some(work_key(&vm.client))]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_vms_sign_in_answers_the_app_alone() {
    let vm = OnVm::new().await;

    let (status, refused) = post(
        &vm.viewer,
        "sign-in/start",
        &json!({"site": "github.com"}),
        false,
    )
    .await;

    assert_eq!(status, 403, "{refused}");
    assert!(vm.ops().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_password_typed_in_a_vm_lists_its_account_on_that_vm() {
    let vm = OnVm::new().await;
    vm.shows_a_frame().await;
    vm.answering(
        "release",
        json!({"ok": true, "released": true, "signedIn": {"app": "com.google.Chrome",
            "name": "Google Chrome", "host": "accounts.google.com"}}),
    );

    let (_, taken) = post(
        &vm.viewer,
        "control/acquire",
        &json!({"frame": "shown"}),
        true,
    )
    .await;
    let (given, released) = post(
        &vm.viewer,
        "control/release",
        &json!({"lease": taken["lease"]}),
        true,
    )
    .await;

    assert_eq!(given, 200, "{released}");
    let accounts = vm.accounts();
    assert_eq!(accounts.len(), 1, "{accounts:?}");
    assert_eq!(accounts[0].id, "google.com");
    assert_eq!(accounts[0].how, sign_ins::How::TakeControl);
    assert_eq!(accounts[0].desktop, Some(work_key(&vm.client)));
}

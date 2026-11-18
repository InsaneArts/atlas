//! Installs on a VM end to end, through a real keeper and a real client:
//! an app staged into a stand-in guest whose worker is Atlas's own,
//! installed from where the guest placed it into a home of the test's own,
//! and a cleanup that moves it aside and undoes a plan, every step as the
//! guest names its paths.
use super::{
    environment_tests::{
        Disk, Ending, GUEST_HOME, applied_record, ended, environment, frozen, last_change,
        manifest, plan_copy, plan_id, previewed, recover,
    },
    tests::{copy, stage_lanes},
};
use crate::{
    client::{Actor, Client},
    host_session::{Host, HostSession},
    task::{Claims, Owned, State, Task, store::Store as Tasks},
    vm::{
        copies::{Owned as Copied, Owners},
        keeper::{change::tests::admitted, tests::until},
    },
};
use atlas_core::{error::Code, files::Token};
use atlas_session::{lane::LaneKind, standin::copies::ROOT};
use serde_json::{Value, json};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering::SeqCst},
    },
};

/// Where the guest's worker puts an install of Probe, as the guest names it.
fn installed_at() -> String {
    format!("{GUEST_HOME}/Applications/Probe.app")
}

/// `client` as an agent's command of its own session and named task.
fn tasked(mut client: Client, session: &str) -> (Client, Task) {
    let host = HostSession::new(Host::Codex, session).unwrap();
    let task = Task::of(host.key(), Some("probe")).unwrap();
    client.host_session = Some(host);
    client.task = Some(task.clone());
    (client, task)
}

/// An app bundle as a build makes one, `Probe.app`, in a folder of its own:
/// a plist naming its executable and identifier, and a real program.
fn bundle() -> (tempfile::TempDir, PathBuf) {
    let folder = tempfile::tempdir().unwrap();
    let app = fs::canonicalize(folder.path()).unwrap().join("Probe.app");
    let contents = app.join("Contents");
    fs::create_dir_all(contents.join("MacOS")).unwrap();
    fs::write(
        contents.join("Info.plist"),
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleIdentifier</key><string>test.atlas.vm-install-probe</string>
<key>CFBundleExecutable</key><string>Probe</string>
<key>CFBundleShortVersionString</key><string>1.0</string>
</dict></plist>"#,
    )
    .unwrap();
    let program = contents.join("MacOS/Probe");
    fs::copy("/usr/bin/true", &program).unwrap();
    fs::set_permissions(&program, fs::Permissions::from_mode(0o755)).unwrap();
    (folder, app)
}

/// What `task` claims, in `client`'s records.
fn claims(client: &Client, task: &Task) -> Vec<(Owned, State)> {
    Tasks::at(client.output_dir().unwrap())
        .load(task.key())
        .unwrap()
        .claims()
        .collect()
}

/// The install `task` claims, and how it stands.
fn install_claim(client: &Client, task: &Task) -> (Token, State) {
    claims(client, task)
        .into_iter()
        .find_map(|(owned, state)| match owned {
            Owned::Install { id, .. } => Some((id, state)),
            _ => None,
        })
        .unwrap()
}

/// How `task`'s claim of copy `copy` stands.
fn copy_claim(client: &Client, task: &Task, copy: &Token) -> Option<State> {
    claims(client, task)
        .into_iter()
        .find_map(|(owned, state)| match owned {
            Owned::Copy { id } if id == *copy => Some(state),
            _ => None,
        })
}

/// This Mac's record of install `id`, when it keeps one.
fn install_record(client: &Client, id: &Token) -> Option<Value> {
    let path = client
        .output_dir()
        .unwrap()
        .join("environment/installs")
        .join(format!("install-{}.json", id.as_str()));
    fs::read(path)
        .ok()
        .map(|bytes| serde_json::from_slice(&bytes).unwrap())
}

/// The one step of `reply`, a cleanup's, that does `what`.
fn step(reply: &Value, what: &str) -> Value {
    reply["steps"]
        .as_array()
        .unwrap()
        .iter()
        .find(|step| step["do"] == what)
        .cloned()
        .unwrap_or_else(|| panic!("no {what} step: {reply}"))
}

/// An install of `app` through `client`, answered whole: its reply.
async fn installed(client: &Client, app: &Path) -> Value {
    let reply = crate::install_app::install(client, app).await.unwrap();
    assert_eq!(reply["installed"], true, "{reply}");
    reply
}

/// The environment requests the guest was asked whose operation is `what`.
fn asked_for(disk: &Disk, what: &str) -> Vec<Value> {
    disk.asked
        .lock()
        .unwrap()
        .iter()
        .filter(|asked| asked["packet"]["request"]["operation"] == what)
        .cloned()
        .collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_app_installed_on_a_vm_is_staged_installed_from_where_the_guest_placed_it_and_its_copy_removed()
 {
    let (serving, _saying, client, disk, _directory) = environment().await;
    let (client, task) = tasked(client, "installing");
    let (_folder, app) = bundle();

    let reply = installed(&client, &app).await;

    assert_eq!(reply["ok"], true, "{reply}");
    assert_eq!(reply["app"], "Probe", "{reply}");
    assert_eq!(reply["path"], installed_at(), "{reply}");
    assert_eq!(reply["bundleId"], "test.atlas.vm-install-probe", "{reply}");
    let staged = stage_lanes(&serving);
    assert_eq!(staged.len(), 1, "staged once: {staged:?}");
    let transport = &staged[0];
    let installs = asked_for(&disk, "installApp");
    assert_eq!(installs.len(), 1, "{installs:?}");
    let (id, state) = install_claim(&client, &task);
    assert_eq!(
        installs[0]["packet"]["request"],
        json!({"operation": "installApp", "id": id, "name": "Probe.app",
            "source": format!("{ROOT}/{transport}/Probe.app")})
    );
    assert_eq!(installs[0]["apply"], true);
    assert_eq!(installs[0]["timeoutMs"], 600_000);
    assert_eq!(installs[0]["packet"]["expected"]["uid"], 502);
    let placed = disk
        .home
        .join("Applications/Probe.app/Contents/MacOS/Probe");
    assert_eq!(
        fs::read(placed).unwrap(),
        fs::read("/usr/bin/true").unwrap()
    );
    assert_eq!(state, State::Made);
    assert!(
        copy(&serving, transport).is_none(),
        "the transport copy went"
    );
    assert_eq!(copy_claim(&client, &task, transport), Some(State::Gone));
    assert_eq!(
        Owners::in_records(client.output_dir().unwrap()).of(transport),
        Copied::Unknown
    );
    assert_eq!(
        install_record(&client, &id).unwrap(),
        json!({"version": 1, "acknowledged": true})
    );
    assert!(client.controller.pending().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_install_on_a_vm_whose_reply_was_lost_keeps_its_claim_its_copy_and_its_record_for_cleanup()
 {
    let (serving, _saying, client, disk, _directory) = environment().await;
    let (client, task) = tasked(client, "lost");
    let (_folder, app) = bundle();
    // Carried out whole, then nothing said or kept, and its command gone.
    *disk.ending.lock().unwrap() = Ending::Unanswered;
    let carried = Arc::new(AtomicBool::new(false));
    *disk.during.lock().unwrap() = Some(Box::new({
        let carried = carried.clone();
        move || carried.store(true, SeqCst)
    }));
    let installing = tokio::spawn({
        let (client, app) = (client.clone(), app.clone());
        async move { crate::install_app::install(&client, &app).await }
    });
    until("carried out", || carried.load(SeqCst)).await;
    installing.abort();
    let _ = installing.await;
    *disk.during.lock().unwrap() = None;

    let change = last_change(&disk);
    let (id, state) = install_claim(&client, &task);
    assert_eq!(state, State::Intent, "the install stays claimed");
    let transport = stage_lanes(&serving)[0].clone();
    assert_eq!(copy_claim(&client, &task, &transport), Some(State::Made));
    assert!(copy(&serving, &transport).is_some(), "its copy kept");
    assert_eq!(
        install_record(&client, &id).unwrap(),
        json!({"version": 1, "acknowledged": false})
    );
    assert_eq!(client.controller.pending().unwrap(), [change.as_str()]);
    assert!(disk.home.join("Applications/Probe.app").exists());

    // Once the guest proves it ended, the task's cleanup settles both.
    *disk.ending.lock().unwrap() = Ending::Answered;
    serving.fixture.guest.standin.done(&change, &ended());
    recover(&serving).await;
    until("its claim cleared", || {
        client.controller.pending().unwrap().is_empty()
    })
    .await;
    let cleaned = crate::cleanup::cleanup(&client, None, None, crate::cleanup::Mode::Apply)
        .await
        .unwrap();

    assert_eq!(cleaned["ok"], true, "{cleaned}");
    assert_eq!(
        step(&cleaned, "remove-install")["state"],
        "done",
        "{cleaned}"
    );
    assert_eq!(step(&cleaned, "remove-copy")["state"], "done", "{cleaned}");
    assert!(!disk.home.join("Applications/Probe.app").exists());
    assert!(copy(&serving, &transport).is_none());
    assert!(install_record(&client, &id).is_none(), "its record went");
    assert_eq!(install_claim(&client, &task).1, State::Gone);
    assert!(client.controller.pending().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_tasks_install_on_a_vm_is_where_open_finds_it_in_the_guest() {
    let (_serving, _saying, client, _disk, _directory) = environment().await;
    let (client, _task) = tasked(client, "opening");
    let (_folder, app) = bundle();
    installed(&client, &app).await;

    let path = crate::install_app::installed_path(&client, "Probe").await;
    let other = crate::install_app::installed_path(&client, "Other").await;

    assert_eq!(path, Some(installed_at()));
    assert_eq!(other, None, "only the task's own install");
    assert!(client.controller.pending().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_tasks_cleanup_on_a_vm_moves_its_install_aside_and_undoes_its_plan() {
    let (serving, _saying, client, disk, _directory) = environment().await;
    let (client, task) = tasked(client, "cleaned");
    let (_folder, app) = bundle();
    installed(&client, &app).await;
    let (id, _) = install_claim(&client, &task);
    let plan = plan_id(&previewed(&client).await);
    crate::environment::apply(&client, &plan).await.unwrap();
    assert_eq!(disk.file(".config/probe/notes.txt").unwrap(), b"notes\n");

    let preview = crate::cleanup::cleanup(&client, None, None, crate::cleanup::Mode::Preview)
        .await
        .unwrap();
    let cleaned = crate::cleanup::cleanup(&client, None, None, crate::cleanup::Mode::Apply)
        .await
        .unwrap();
    let again = crate::cleanup::cleanup(&client, None, None, crate::cleanup::Mode::Apply)
        .await
        .unwrap();

    let undo = step(&preview, "undo-plan");
    assert_eq!(undo["planState"], "applied", "{preview}");
    assert_eq!(
        undo["targets"],
        json!([".config/probe/notes.txt", "bin/probe.sh"])
    );
    let removal = step(&preview, "remove-install");
    assert_eq!(removal["present"], true, "{preview}");
    assert_eq!(removal["path"], installed_at(), "{preview}");
    assert_eq!(cleaned["ok"], true, "{cleaned}");
    assert_eq!(step(&cleaned, "undo-plan")["state"], "done", "{cleaned}");
    let removed = step(&cleaned, "remove-install");
    assert_eq!(removed["state"], "done", "{cleaned}");
    assert_eq!(
        removed["held"],
        format!("{GUEST_HOME}/.config/atlas/installs/held/{id}/Probe.app")
    );
    assert!(!disk.home.join("Applications/Probe.app").exists());
    assert!(
        disk.home
            .join(format!(".config/atlas/installs/held/{id}/Probe.app"))
            .exists(),
        "held, not deleted"
    );
    assert!(disk.file(".config/probe/notes.txt").is_none(), "undone");
    assert!(!frozen(&client, &plan).exists(), "its plan went");
    assert!(install_record(&client, &id).is_none(), "its record went");
    assert_eq!(install_claim(&client, &task).1, State::Gone);
    assert_eq!(again["steps"], json!([]), "{again}");
    assert_eq!(
        asked_for(&disk, "rollback")
            .iter()
            .map(|asked| asked["packet"]["request"]["apply"].clone())
            .collect::<Vec<_>>(),
        [json!(false), json!(true)],
        "asked how far it got, then undone"
    );
    assert!(copy(&serving, &stage_lanes(&serving)[0]).is_none());
    assert!(client.controller.pending().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_copy_a_plan_owns_is_kept_by_another_tasks_cleanup_and_its_plan_still_applies() {
    let (serving, _saying, client, disk, _directory) = environment().await;
    let (planning, _) = tasked(client.clone(), "planning");
    let (cleaning, task) = tasked(client, "cleaning");
    let plan = plan_id(&previewed(&planning).await);
    let change = plan_copy(&planning, &plan);
    // Claimed as a copy by the other task too, as its cleanup would find one.
    Claims::in_store(Tasks::at(cleaning.output_dir().unwrap()), task.clone())
        .claim(&Owned::Copy { id: change.clone() }, State::Made)
        .unwrap();

    let cleaned = crate::cleanup::cleanup(&cleaning, None, None, crate::cleanup::Mode::Apply)
        .await
        .unwrap();
    let applied = crate::environment::apply(&planning, &plan).await.unwrap();

    let kept = step(&cleaned, "remove-copy");
    assert_eq!(kept["state"], "kept", "{cleaned}");
    assert_eq!(kept["reason"], "another-session", "{cleaned}");
    assert!(
        cleaned["steps"]
            .as_array()
            .unwrap()
            .iter()
            .all(|step| step["do"] != "undo-plan"),
        "the plan is not the cleaning task's: {cleaned}"
    );
    assert_eq!(copy_claim(&cleaning, &task, &change), Some(State::Kept));
    assert_eq!(applied["applied"], true, "{applied}");
    assert_eq!(disk.file(".config/probe/notes.txt").unwrap(), b"notes\n");
    assert!(copy(&serving, &change).is_none(), "the apply let it go");
    assert!(cleaning.controller.pending().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_guest_that_lost_an_acknowledged_install_has_it_kept_by_cleanup_in_words_naming_it() {
    let (_serving, _saying, client, disk, _directory) = environment().await;
    let (client, task) = tasked(client, "forgetting");
    let (_folder, app) = bundle();
    installed(&client, &app).await;
    let (id, _) = install_claim(&client, &task);
    fs::remove_dir_all(disk.home.join(".config/atlas/installs")).unwrap();

    let cleaned = crate::cleanup::cleanup(&client, None, None, crate::cleanup::Mode::Apply)
        .await
        .unwrap();
    let again = crate::cleanup::cleanup(&client, None, None, crate::cleanup::Mode::Apply)
        .await
        .unwrap();

    let kept = step(&cleaned, "remove-install");
    assert_eq!(kept["state"], "kept", "{cleaned}");
    assert_eq!(kept["reason"], "forgotten", "{cleaned}");
    assert_eq!(
        kept["error"],
        "work has no record of this install, though it may have been made there, so Atlas \
         moved nothing aside and keeps its own record of it until work is removed"
    );
    assert_eq!(install_claim(&client, &task).1, State::Kept, "never gone");
    assert_eq!(
        install_record(&client, &id).unwrap()["acknowledged"],
        true,
        "this Mac's record kept"
    );
    assert!(disk.home.join("Applications/Probe.app").exists());
    assert_eq!(again["steps"], json!([]), "{again}");
    assert!(client.controller.pending().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_guest_that_lost_an_acknowledged_apply_has_its_plan_kept_by_cleanup_in_words_naming_it() {
    let (_serving, _saying, client, disk, _directory) = environment().await;
    let (client, _task) = tasked(client, "forgetting");
    let plan = plan_id(&previewed(&client).await);
    crate::environment::apply(&client, &plan).await.unwrap();
    fs::remove_dir_all(disk.home.join(crate::environment::STATE)).unwrap();

    let preview = crate::cleanup::cleanup(&client, None, None, crate::cleanup::Mode::Preview)
        .await
        .unwrap();
    let cleaned = crate::cleanup::cleanup(&client, None, None, crate::cleanup::Mode::Apply)
        .await
        .unwrap();

    assert_eq!(
        step(&preview, "undo-plan")["planState"],
        "unknown",
        "{preview}"
    );
    let kept = step(&cleaned, "undo-plan");
    assert_eq!(kept["state"], "kept", "{cleaned}");
    assert_eq!(kept["reason"], "forgotten", "{cleaned}");
    assert_eq!(
        kept["error"],
        "work has no record of this copy, though it may have applied it, so Atlas undid \
         nothing and keeps its plan until work is removed"
    );
    assert!(frozen(&client, &plan).join("plan.json").exists());
    assert_eq!(applied_record(&client, &plan)["acknowledged"], true);
    assert_eq!(disk.file(".config/probe/notes.txt").unwrap(), b"notes\n");
    assert!(client.controller.pending().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_install_on_a_vm_whose_guest_has_no_environment_worker_is_refused_before_anything_is_staged()
 {
    let (serving, saying, client, disk, _directory) = environment().await;
    let (client, task) = tasked(client, "older");
    *saying.hello.lock().unwrap() = Some(|hello: &mut Value| {
        let capabilities = hello["daemon"]["capabilities"].as_array_mut().unwrap();
        capabilities.retain(|name| name != atlas_session::compatibility::ENVIRONMENT);
    });
    let (_folder, app) = bundle();

    let refused = crate::install_app::install(&client, &app)
        .await
        .unwrap_err();

    assert_eq!(refused.code, Code::Update, "{refused:?}");
    assert_eq!(
        refused.error,
        "work is running an older Atlas that has no environment worker"
    );
    assert!(stage_lanes(&serving).is_empty(), "nothing staged");
    assert!(
        admitted(&serving)
            .iter()
            .all(|header| !matches!(header.kind, LaneKind::Change { .. })),
        "no change asked for"
    );
    assert!(disk.asked.lock().unwrap().is_empty());
    assert!(
        Tasks::at(client.output_dir().unwrap())
            .load(task.key())
            .is_none(),
        "nothing claimed"
    );
    assert!(client.controller.pending().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_persons_cleanup_of_a_session_on_a_vm_reaches_its_installs_and_plans_too() {
    let (_serving, _saying, client, disk, _directory) = environment().await;
    let (client, _task) = tasked(client, "persons");
    let (_folder, app) = bundle();
    installed(&client, &app).await;
    let (_manifest_folder, manifest) = manifest();
    let plan = crate::environment::sync(&client, &manifest, true)
        .await
        .unwrap();
    let mut person = client.clone();
    person.actor = Actor::Viewer;
    let session = client.host_session.as_ref().unwrap().key().clone();

    let cleaned = crate::cleanup::for_person(&person, &session, crate::cleanup::Mode::Apply)
        .await
        .unwrap();

    assert_eq!(cleaned["ok"], true, "{cleaned}");
    assert_eq!(step(&cleaned, "remove-install")["state"], "done");
    assert_eq!(step(&cleaned, "undo-plan")["state"], "done");
    assert!(!disk.home.join("Applications/Probe.app").exists());
    assert!(disk.file(".config/probe/notes.txt").is_none());
    assert!(!frozen(&client, &plan_id(&plan)).exists());
    assert!(client.controller.pending().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_plan_whose_rollback_was_cut_short_after_its_phase_was_saved_is_let_go_of_by_cleanup() {
    let (_serving, _saying, client, disk, _directory) = environment().await;
    let (client, _task) = tasked(client, "cut-short");
    let plan = plan_id(&previewed(&client).await);
    crate::environment::apply(&client, &plan).await.unwrap();
    crate::environment::rollback(&client, &plan, true)
        .await
        .unwrap();
    // As a rollback cut short between saving its phase and letting go of
    // the guest's active transaction leaves it.
    let active = disk
        .home
        .join(crate::environment::STATE)
        .join("active.json");
    fs::write(&active, serde_json::to_vec(&plan).unwrap()).unwrap();
    fs::set_permissions(&active, fs::Permissions::from_mode(0o600)).unwrap();

    let cleaned = crate::cleanup::cleanup(&client, None, None, crate::cleanup::Mode::Apply)
        .await
        .unwrap();

    assert_eq!(step(&cleaned, "undo-plan")["state"], "done", "{cleaned}");
    assert!(!active.exists(), "the guest let go of it");
    assert!(!frozen(&client, &plan).exists(), "its plan went");
    let next = plan_id(&previewed(&client).await);
    crate::environment::apply(&client, &next).await.unwrap();
    assert_eq!(disk.file(".config/probe/notes.txt").unwrap(), b"notes\n");
    assert!(client.controller.pending().unwrap().is_empty());
}

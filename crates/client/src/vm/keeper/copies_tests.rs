//! Copies on a VM end to end: who each was made for kept here before it is
//! staged, `open`, `close` and a cleanup through a real keeper and a real
//! client, and every removal only for the session a copy's record names.
use super::tests::{answering_reads, contents_of, copy, held, stage_lanes, staged, tree};
use crate::{
    agent::Closing,
    doing::Doing,
    handoff::Target,
    host_session::{Host, HostSession},
    vm::{
        copies::{Owned, Owners},
        keeper::{
            change::tests::{admitted, live, proven, restarted, settled},
            clients::tests::{asked, client_of, serving},
            tests::until,
        },
    },
};
use atlas_core::files::Token;
use atlas_session::standin::copies::ROOT;
use serde_json::json;

/// A client of host session `name` beside `client`.
fn of_session(client: &crate::client::Client, name: &str) -> crate::client::Client {
    let mut client = client.clone();
    client.host_session = Some(HostSession::new(Host::Codex, name).unwrap());
    client
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_tree_opened_on_a_vm_is_staged_into_its_guest_and_opened_from_where_it_was_placed() {
    let (serving, saying, client, _directory) = proven().await;
    answering_reads(&saying);
    let client = client.doing(Doing::new("open"));
    let (_folder, tree) = tree();

    let reply = crate::agent::open(&client, None, &[tree.to_string_lossy().into_owned()], &[])
        .await
        .unwrap();

    assert_eq!(reply["opened"], true, "{reply}");
    let lanes = stage_lanes(&serving);
    assert_eq!(lanes.len(), 1, "one stage");
    let change = &lanes[0];
    let placed = format!("{ROOT}/{change}/Probe.app");
    assert_eq!(reply["copied"][0]["to"], placed, "{reply}");
    assert_eq!(
        copy(&serving, change).unwrap().contents,
        contents_of(&tree),
        "every byte, in order"
    );
    let runs = saying.runs.lock().unwrap().clone();
    assert_eq!(runs.len(), 1, "{runs:?}");
    assert_eq!(runs[0]["argv"], json!(["/usr/bin/open", "--", placed]));
    assert!(client.controller.pending().unwrap().is_empty());
    until("the stage settled with the guest", || {
        settled(&serving).contains(change)
    })
    .await;
    // The session's record names the guest's copy by its token.
    let copies = crate::handoff::Copies::of(&client.desktop()).unwrap();
    assert_eq!(copies.token_of(&placed).as_ref(), Some(change));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn closing_an_app_on_a_vm_removes_its_copy_from_the_guest() {
    let (serving, saying, client, _directory) = proven().await;
    answering_reads(&saying);
    let (_folder, tree) = tree();
    let change = staged(&client, &tree).await;
    let client = client.doing(Doing::new("close").app(Some("Probe")));

    let reply = crate::agent::close(&client, Closing::App("Probe"))
        .await
        .unwrap();

    assert_eq!(reply["ok"], true, "{reply}");
    assert_eq!(reply["removed"], json!([format!("{ROOT}/{change}")]));
    assert!(copy(&serving, &change).is_none(), "removed in the guest");
    assert!(client.controller.pending().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_persons_cleanup_of_a_session_on_a_vm_removes_the_copy_its_task_claims() {
    let (serving, saying, mut client, _directory) = proven().await;
    // Atlas.app's, on the person's behalf.
    client.actor = crate::client::Actor::Viewer;
    answering_reads(&saying);
    let (_folder, tree) = tree();
    let host = HostSession::new(Host::Codex, "cleaned").unwrap();
    let session = host.key().clone();
    client.host_session = Some(host);
    let change = staged(&client, &tree).await;
    let task = crate::task::Task::of(&session, None).unwrap();
    crate::task::Claims::in_store(
        crate::task::store::Store::at(client.output_dir().unwrap()),
        task,
    )
    .claim(
        &crate::task::Owned::Copy { id: change.clone() },
        crate::task::State::Made,
    )
    .unwrap();

    let reply = crate::cleanup::for_person(&client, &session, crate::cleanup::Mode::Apply)
        .await
        .unwrap();

    assert_eq!(reply["ok"], true, "{reply}");
    let removal = reply["steps"]
        .as_array()
        .unwrap()
        .iter()
        .find(|step| step["do"] == "remove-copy")
        .cloned()
        .unwrap_or_default();
    assert_eq!(removal["state"], "done", "{reply}");
    assert_eq!(removal["path"], format!("{ROOT}/{change}"));
    assert_eq!(removal["app"], "Probe");
    assert!(copy(&serving, &change).is_none(), "removed in the guest");
    assert!(client.controller.pending().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_reads_open_close_and_cleanup_need_are_carried_and_every_other_read_is_still_refused() {
    let (serving, saying, client, _directory) = proven().await;
    answering_reads(&saying);

    let apps = client.running_apps().await.unwrap();
    assert!(apps.rows().is_empty());
    assert!(
        crate::vm::copies::inventory(&client)
            .await
            .unwrap()
            .is_empty()
    );
    for read in [
        json!({"op": "apps"}),
        json!({"op": "windows"}),
        json!({"op": "elements", "pid": 1}),
        json!({"op": "screenshot"}),
        json!({"op": "remove-copy", "copy": Token::new(), "node": "1:2"}),
        json!({"op": "stage"}),
    ] {
        let said = asked(&serving.socket, &json!({"op": "read", "request": read})).await;
        assert_eq!(said[0]["code"], "denied", "{read}: {said:?}");
        let refused = client.link.call(&read).await.unwrap_err();
        assert_eq!(refused.error, client.unreachable().error, "{read}");
    }
    assert_eq!(
        client.apps().await.unwrap_err().error,
        client.unreachable().error,
        "atlas apps itself is not built for a VM"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sixty_five_copies_of_one_session_are_none_of_another_sessions_to_close() {
    let (serving, saying, client, _directory) = proven().await;
    answering_reads(&saying);
    let ours = of_session(&client, "ours").doing(Doing::new("open"));
    let trees: Vec<_> = (0..65).map(|_| tree()).collect();
    let targets: Vec<String> = trees
        .iter()
        .map(|(_, tree)| tree.to_string_lossy().into_owned())
        .collect();
    let opened = crate::agent::open(&ours, None, &targets, &[])
        .await
        .unwrap();
    assert_eq!(opened["copied"].as_array().unwrap().len(), 65, "{opened}");
    let theirs = of_session(&client, "theirs").doing(Doing::new("close").app(Some("Probe")));

    let kept = crate::agent::close(&theirs, Closing::App("Probe"))
        .await
        .unwrap();
    let removed = crate::agent::close(
        &ours.clone().doing(Doing::new("close").app(Some("Probe"))),
        Closing::App("Probe"),
    )
    .await
    .unwrap();

    assert_eq!(kept["removed"], json!([]), "{kept}");
    assert_eq!(kept["keptOthers"], 65, "{kept}");
    assert_eq!(
        removed["removed"].as_array().unwrap().len(),
        65,
        "{removed}"
    );
    assert!(serving.fixture.guest.standin.copies().is_empty());
    let owners = Owners::in_records(client.output_dir().unwrap());
    for token in stage_lanes(&serving) {
        assert_eq!(owners.of(&token), Owned::Unknown, "forgotten once removed");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_copy_whose_stager_never_noted_it_is_still_its_sessions_alone() {
    let (serving, saying, client, _directory) = proven().await;
    answering_reads(&saying);
    let (_folder, tree) = tree();
    // As a command killed once it staged: nothing reached its session's
    // record, which names no copy at all.
    let ours = of_session(&client, "ours");
    let change = staged(&ours, &tree).await;
    let sessions = crate::host_session::store::Store::at(client.output_dir().unwrap());
    assert!(sessions.all().iter().all(|record| record.copies.is_empty()));

    let theirs = crate::agent::close(&of_session(&client, "theirs"), Closing::App("Probe"))
        .await
        .unwrap();
    let nobodys = crate::agent::close(&client, Closing::App("Probe"))
        .await
        .unwrap();
    assert!(copy(&serving, &change).is_some(), "kept for its session");
    let own = crate::agent::close(&ours, Closing::App("Probe"))
        .await
        .unwrap();

    assert_eq!(theirs["keptOthers"], 1, "{theirs}");
    assert_eq!(nobodys["keptOthers"], 1, "{nobodys}");
    assert_eq!(own["removed"], json!([format!("{ROOT}/{change}")]), "{own}");
    assert!(copy(&serving, &change).is_none());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_copy_with_an_unreadable_record_or_none_is_never_removed() {
    let (serving, saying, client, _directory) = proven().await;
    answering_reads(&saying);
    let mut client = of_session(&client, "ours");
    let (_folder, tree) = tree();
    let unreadable = staged(&client, &tree).await;
    let records = client.output_dir().unwrap().join("copies");
    std::fs::write(records.join(format!("{unreadable}.json")), b"{").unwrap();
    let unrecorded = Token::new();
    client
        .stage_vm(&unrecorded, &held(&tree), &tree, live(&serving))
        .await
        .unwrap();
    let host = client.host_session.clone().unwrap();

    let closed = crate::agent::close(&client, Closing::App("Probe"))
        .await
        .unwrap();
    client.actor = crate::client::Actor::Viewer;
    let task = crate::task::Task::of(host.key(), None).unwrap();
    let claims = crate::task::Claims::in_store(
        crate::task::store::Store::at(client.output_dir().unwrap()),
        task,
    );
    for id in [&unreadable, &unrecorded] {
        claims
            .claim(
                &crate::task::Owned::Copy { id: id.clone() },
                crate::task::State::Made,
            )
            .unwrap();
    }
    let cleaned = crate::cleanup::for_person(&client, host.key(), crate::cleanup::Mode::Apply)
        .await
        .unwrap();

    assert_eq!(closed["removed"], json!([]), "{closed}");
    assert_eq!(closed["keptOthers"], 2, "{closed}");
    let removals: Vec<_> = cleaned["steps"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|step| step["do"] == "remove-copy")
        .collect();
    assert_eq!(removals.len(), 2, "{cleaned}");
    assert!(
        removals.iter().all(|step| step["state"] != "done"),
        "{cleaned}"
    );
    assert!(copy(&serving, &unreadable).is_some());
    assert!(copy(&serving, &unrecorded).is_some());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_folder_holding_a_secret_is_refused_before_anything_is_owned_or_staged() {
    let (serving, _saying, client, _directory) = proven().await;
    let folder = tempfile::tempdir().unwrap();
    let project = folder.path().join("project");
    std::fs::create_dir(&project).unwrap();
    std::fs::write(project.join("main.rs"), b"fn main() {}").unwrap();
    std::fs::write(project.join(".env"), b"TOKEN=secret").unwrap();
    let ownership = client.controller.acquire_beside_commands().unwrap();

    let refused = crate::vm::copies::hand_over_to(
        &client,
        &ownership,
        vec![Target::Path(project)],
        &client.verify_vm().await.unwrap(),
    )
    .await
    .unwrap_err();

    assert_eq!(
        refused.code,
        atlas_core::error::Code::Credentials,
        "{refused:?}"
    );
    assert!(stage_lanes(&serving).is_empty());
    assert!(!client.output_dir().unwrap().join("copies").exists());
    assert!(client.controller.pending().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_guest_started_anew_between_choosing_what_to_quit_and_quitting_it_is_never_signalled() {
    let (serving, saying, client, _directory) = proven().await;
    *saying.otherwise.lock().unwrap() = Some(std::sync::Arc::new(
        |_header: &atlas_session::lane::Header, _request| {
            json!({"ok": true, "starts": null, "apps": [{"pid": 4321, "name": "Probe",
            "executable": "/Applications/Probe.app/Contents/MacOS/Probe"}]})
        },
    ));
    let verified = client.verify_vm().await.unwrap();
    let apps = client.apps_in(&verified).await.unwrap();
    let chosen = crate::app_selection::selected(&apps, Closing::App("Probe"), None);
    assert_eq!(chosen.len(), 1);
    restarted(&serving).await;
    let ownership = client.controller.acquire_beside_commands().unwrap();

    let refused = crate::agent::quit_until(
        &client,
        &ownership,
        (&chosen, Some(&verified)),
        |_, _| Vec::new(),
        crate::agent::QUIT_WAIT,
    )
    .await;

    assert!(refused.is_err(), "never quit in another generation");
    assert!(saying.runs.lock().unwrap().is_empty(), "nothing signalled");
    assert!(
        admitted(&serving)
            .iter()
            .all(|header| !matches!(header.kind, atlas_session::lane::LaneKind::Change { .. })),
        "no change lane opened"
    );
    assert!(client.controller.pending().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_guest_started_anew_between_staging_and_launching_is_never_launched_in() {
    let (serving, saying, client, _directory) = proven().await;
    answering_reads(&saying);
    let client = client.doing(Doing::new("open"));
    let (_folder, tree) = tree();
    let guest = serving.fixture.guest.standin.clone();
    guest
        .holding
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let opening = tokio::spawn({
        let (client, tree) = (client.clone(), tree.to_string_lossy().into_owned());
        async move { crate::agent::open(&client, None, &[tree], &[]).await }
    });
    until("every byte in", || {
        guest.received.load(std::sync::atomic::Ordering::SeqCst) == 1
    })
    .await;
    let proved = live(&serving);

    restarted(&serving).await;
    guest
        .holding
        .store(false, std::sync::atomic::Ordering::SeqCst);
    let opened = opening.await.unwrap();

    let refused = opened.unwrap_err();
    assert_eq!(
        refused.code,
        atlas_core::error::Code::SessionChanged,
        "{refused:?}"
    );
    assert!(saying.runs.lock().unwrap().is_empty(), "never launched");
    assert!(
        admitted(&serving).iter().all(|header| {
            !matches!(header.kind, atlas_session::lane::LaneKind::Change { .. })
                || header.generation.0 == proved
        }),
        "no change in the new generation"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_guest_started_anew_between_quitting_and_removing_keeps_the_copy() {
    let (serving, saying, client, _directory) = proven().await;
    answering_reads(&saying);
    let (_folder, tree) = tree();
    let client = of_session(&client, "ours");
    let change = staged(&client, &tree).await;
    let session = client.host_session.as_ref().unwrap().key().clone();
    let verified = client.verify_vm().await.unwrap();

    restarted(&serving).await;
    let removed =
        crate::vm::copies::remove_of_app(&client, "Probe", &[], (Some(&session), &verified)).await;
    let cleaned = crate::handoff::Copies::of(&client.desktop())
        .unwrap()
        .remove_with(&client, &change, (&session, Some(&verified)), &[])
        .await;

    assert!(removed.folders.is_empty(), "{removed:?}");
    assert_eq!(removed.failed.len(), 1, "{removed:?}");
    assert!(
        removed.failed[0].error.contains("ended"),
        "session-changed: {removed:?}"
    );
    assert!(
        matches!(&cleaned, crate::handoff::CopyRemoval::Failed(error) if error.contains("ended")),
        "session-changed: {cleaned:?}"
    );
    assert!(copy(&serving, &change).is_some(), "kept in the guest");
    assert!(
        matches!(
            Owners::in_records(client.output_dir().unwrap()).of(&change),
            Owned::By(_)
        ),
        "still owned"
    );
    assert!(client.controller.pending().unwrap().is_empty());
}

/// A folder of the test's own inside Atlas's own sockets folder, which the
/// system names outside the home: Atlas's own, as a real VM's records are,
/// in `~/Library/Application Support/Atlas`.
fn inside_atlas_own() -> (std::path::PathBuf, tempfile::TempDir) {
    let sockets = atlas_core::own_folder::sockets().unwrap();
    atlas_core::files::private_folder(&sockets, atlas_core::error::Code::State).unwrap();
    let inside = tempfile::Builder::new()
        .prefix("test-records-")
        .tempdir_in(&sockets)
        .unwrap();
    (sockets, inside)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_file_beside_atlass_own_folders_is_staged_and_only_what_is_in_them_or_holds_them_refused()
{
    let serving = serving(true).await;
    let saying = serving.fixture.guest.proving();
    answering_reads(&saying);
    // The VM's records in Atlas's own folder, as they are for real: its
    // copies' owners are kept there.
    let (sockets, records) = inside_atlas_own();
    assert!(atlas_core::own_folder::refuse(records.path()).is_err());
    let client = client_of(&serving, records.path());
    // The file in a folder of the home, beside `~/Library`, which holds
    // Atlas's own folder: the home holds both.
    let home = atlas_core::config::home().unwrap();
    let beside = tempfile::Builder::new()
        .prefix(".atlas-test-")
        .tempdir_in(&home)
        .unwrap();
    let file = beside.path().join("probe.sh");
    std::fs::write(&file, b"#!/bin/sh\necho hi\n").unwrap();

    let opened = crate::agent::open(
        &client.clone().doing(Doing::new("open")),
        None,
        &[file.to_string_lossy().into_owned()],
        &[],
    )
    .await
    .unwrap();
    let change = stage_lanes(&serving)[0].clone();
    let owners = Owners::in_records(client.output_dir().unwrap());
    let owned = owners.of(&change);
    let closed = crate::agent::close(
        &client.clone().doing(Doing::new("close").app(Some("probe"))),
        Closing::App("probe"),
    )
    .await
    .unwrap();

    assert_eq!(opened["opened"], true, "{opened}");
    assert!(matches!(owned, Owned::By(_)), "{owned:?}");
    assert_eq!(
        closed["removed"],
        json!([format!("{ROOT}/{change}")]),
        "{closed}"
    );
    assert!(copy(&serving, &change).is_none());
    assert_eq!(owners.of(&change), Owned::Unknown, "forgotten once removed");

    // What holds Atlas's own folders, and what is in them, is refused before
    // anything is owned or staged.
    let inside = records.path().join("inside.txt");
    std::fs::write(&inside, b"records").unwrap();
    let verified = client.verify_vm().await.unwrap();
    let ownership = client.controller.acquire_beside_commands().unwrap();
    for refused in [home.clone(), sockets.parent().unwrap().to_owned(), inside] {
        let error = crate::vm::copies::hand_over_to(
            &client,
            &ownership,
            vec![Target::Path(refused.clone())],
            &verified,
        )
        .await
        .unwrap_err();
        assert_eq!(
            error.error,
            atlas_core::own_folder::REFUSAL,
            "{refused:?}: {error:?}"
        );
    }
    assert_eq!(stage_lanes(&serving).len(), 1, "nothing more staged");
}

/// The native check's fixture on this Mac, holding `bytes`.
fn fixture(bytes: &[u8]) -> (tempfile::TempDir, std::path::PathBuf) {
    let folder = tempfile::tempdir().unwrap();
    let path = std::fs::canonicalize(folder.path())
        .unwrap()
        .join("DesktopFixture");
    std::fs::write(&path, bytes).unwrap();
    (folder, path)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn two_native_checks_in_a_row_reuse_one_copy_of_their_fixture_at_one_path() {
    let (serving, saying, client, _directory) = proven().await;
    answering_reads(&saying);
    let verified = client.verify_vm().await.unwrap();
    let (_folder, path) = fixture(b"fixture one");

    let first = crate::vm::copies::fixture_copy(&client, &verified, &path)
        .await
        .unwrap();
    let second = crate::vm::copies::fixture_copy(&client, &verified, &path)
        .await
        .unwrap();

    let lanes = stage_lanes(&serving);
    assert_eq!(lanes.len(), 1, "staged once");
    assert_eq!(first, second);
    assert_eq!(
        first.to_string_lossy(),
        format!("{ROOT}/{}/DesktopFixture", lanes[0])
    );
    assert_eq!(copy(&serving, &lanes[0]).unwrap().contents, b"fixture one");
    assert!(client.controller.pending().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_new_fixture_digest_replaces_its_copy_and_the_old_one_is_removed_as_a_change() {
    let (serving, saying, client, _directory) = proven().await;
    answering_reads(&saying);
    let verified = client.verify_vm().await.unwrap();
    let (_folder, path) = fixture(b"fixture one");
    let first = crate::vm::copies::fixture_copy(&client, &verified, &path)
        .await
        .unwrap();
    let old = stage_lanes(&serving)[0].clone();

    std::fs::write(&path, b"fixture two, rebuilt").unwrap();
    let replaced = crate::vm::copies::fixture_copy(&client, &verified, &path)
        .await
        .unwrap();

    let lanes = stage_lanes(&serving);
    assert_eq!(lanes.len(), 2, "{lanes:?}");
    assert_ne!(first, replaced);
    assert!(copy(&serving, &old).is_none(), "the old copy removed");
    assert_eq!(
        copy(&serving, &lanes[1]).unwrap().contents,
        b"fixture two, rebuilt"
    );
    let owners = Owners::in_records(client.output_dir().unwrap());
    assert_eq!(owners.of(&old), Owned::Unknown, "its record gone with it");
    assert!(matches!(owners.of(&lanes[1]), Owned::By(owner) if owner.fixture.is_some()));
    assert!(client.controller.pending().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn neither_close_nor_a_removal_for_any_session_takes_the_native_checks_fixture() {
    let (serving, saying, client, _directory) = proven().await;
    answering_reads(&saying);
    let verified = client.verify_vm().await.unwrap();
    let (_folder, path) = fixture(b"fixture one");
    crate::vm::copies::fixture_copy(&client, &verified, &path)
        .await
        .unwrap();
    let kept = stage_lanes(&serving)[0].clone();

    let closed = crate::agent::close(
        &client
            .clone()
            .doing(Doing::new("close").app(Some("DesktopFixture"))),
        Closing::App("DesktopFixture"),
    )
    .await
    .unwrap();
    let removal = crate::vm::copies::removal(&client, &kept, (None, &verified), &[]).await;

    assert_eq!(closed["removed"], json!([]), "{closed}");
    assert_eq!(closed["keptOthers"], 1, "{closed}");
    assert_eq!(removal, crate::handoff::CopyRemoval::AnotherSession);
    assert!(copy(&serving, &kept).is_some(), "kept for the next check");
    let claimed = crate::task::store::Store::at(client.output_dir().unwrap())
        .all()
        .iter()
        .any(|record| {
            record
                .claims()
                .any(|(owned, _)| owned.same(&crate::task::Owned::Copy { id: kept.clone() }))
        });
    assert!(!claimed, "no task claims it");
}

/// What the fixture's kept copy becomes when it is checked again after
/// `damage` was done to it in the guest: the copy launched, the copy
/// damaged, and every stage asked.
async fn checked_again_after(
    damage: impl FnOnce(&mut atlas_session::standin::copies::Copy),
) -> (std::path::PathBuf, std::path::PathBuf, Vec<Token>, bool) {
    let (serving, saying, client, _directory) = proven().await;
    answering_reads(&saying);
    let verified = client.verify_vm().await.unwrap();
    let (_folder, path) = fixture(b"fixture one");
    let first = crate::vm::copies::fixture_copy(&client, &verified, &path)
        .await
        .unwrap();
    let damaged = stage_lanes(&serving)[0].clone();
    damage(
        serving
            .fixture
            .guest
            .standin
            .copies()
            .get_mut(&damaged)
            .unwrap(),
    );

    let again = crate::vm::copies::fixture_copy(&client, &verified, &path)
        .await
        .unwrap();

    let gone = copy(&serving, &damaged).is_none();
    (first, again, stage_lanes(&serving), gone)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_kept_fixture_whose_executable_was_deleted_is_staged_again_never_launched_as_it_was() {
    let (first, again, stages, gone) = checked_again_after(|copy| {
        copy.snapshot
            .entries
            .retain(|entry| !matches!(entry.kind, atlas_core::stage::Kind::File { .. }));
        copy.contents.clear();
    })
    .await;

    assert_eq!(stages.len(), 2, "staged again: {stages:?}");
    assert_ne!(first, again);
    assert!(again.starts_with(format!("{ROOT}/{}", stages[1])));
    assert!(gone, "the damaged copy removed as a change");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_kept_fixture_whose_executable_was_replaced_is_staged_again_never_launched_as_it_was() {
    let (first, again, stages, gone) = checked_again_after(|copy| {
        // Other code of the same length in its place.
        copy.contents = b"fixture 1!!".to_vec();
    })
    .await;

    assert_eq!(stages.len(), 2, "staged again: {stages:?}");
    assert_ne!(first, again);
    assert!(gone, "the replaced copy removed as a change");
}

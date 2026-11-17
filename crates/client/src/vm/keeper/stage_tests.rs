//! Stages and copies on a VM end to end, through a real keeper and a real
//! client, its guest a stand-in that keeps copies as a machine guest does:
//! each stage's bytes read with credit given back, placed in a folder named
//! by its change, listed with what runs from it, and removed as a change.
use super::*;
use crate::vm::keeper::{
    change::tests::{admitted, live, proven, record, settled},
    clients::tests::Serving,
    tests::{Saying, until},
};
use atlas_core::{
    protocol::{JobReply, Showing},
    stage::Snapshot,
};
use atlas_session::lane::Header;
use std::{
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering::SeqCst as Ordered},
    },
};

/// Answer the reads `open`, `close` and a cleanup make that the stand-in
/// does not: the guest's apps (none running) and its jobs (none).
pub(super) fn answering_reads(saying: &Saying) {
    *saying.otherwise.lock().unwrap() = Some(Arc::new(|header: &Header, request: Value| {
        match (&header.kind, request["op"].as_str()) {
            (LaneKind::Read, Some("apps")) => json!({"ok": true, "apps": [], "starts": null}),
            (LaneKind::Read, Some("job")) => {
                JobReply::Show(Showing::Every { jobs: Vec::new() }).into_value()
            }
            _ => json!({"ok": true, "echo": request}),
        }
    }));
}

/// A folder to stage: `Probe.app` holding a program and its plist, in a
/// folder of its own.
pub(super) fn tree() -> (tempfile::TempDir, std::path::PathBuf) {
    let folder = tempfile::tempdir().unwrap();
    let tree = folder.path().join("Probe.app");
    std::fs::create_dir_all(tree.join("Contents/MacOS")).unwrap();
    std::fs::write(tree.join("Contents/MacOS/Probe"), b"#!/bin/sh\necho hi\n").unwrap();
    std::fs::write(tree.join("Contents/Info.plist"), b"<plist/>").unwrap();
    (folder, tree)
}

/// Every file's contents under `tree`, in a snapshot's order.
pub(super) fn contents_of(tree: &Path) -> Vec<u8> {
    let snapshot = Snapshot::of(tree).unwrap();
    snapshot
        .files()
        .flat_map(|(entry, _, _)| {
            let path = entry
                .path
                .iter()
                .fold(tree.to_owned(), |path, part| path.join(part));
            std::fs::read(path).unwrap()
        })
        .collect()
}

/// The stand-in's copy of `change`, when it keeps one.
pub(super) fn copy(
    serving: &Serving,
    change: &Token,
) -> Option<atlas_session::standin::copies::Copy> {
    serving.fixture.guest.standin.copies().get(change).cloned()
}

/// Every stage lane the guest admitted.
pub(super) fn stage_lanes(serving: &Serving) -> Vec<Token> {
    admitted(serving)
        .into_iter()
        .filter_map(|header| match header.kind {
            LaneKind::Stage { change } => Some(change),
            _ => None,
        })
        .collect()
}

/// Hand `tree` to the guest through `client`, as `open` does: its copy's
/// token.
pub(super) async fn staged(client: &crate::client::Client, tree: &Path) -> Token {
    let ownership = client.controller.acquire_beside_commands().unwrap();
    let handed = crate::vm::copies::hand_over_to(
        client,
        &ownership,
        vec![crate::handoff::Target::Path(tree.to_owned())],
        &client.verify_vm().await.unwrap(),
    )
    .await
    .unwrap();
    handed.copied[0].token.clone()
}

/// `tree` held as a stage sends it.
pub(super) fn held(tree: &Path) -> Arc<atlas_core::stage::Held> {
    Arc::new(Snapshot::held(tree).unwrap())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_lost_stage_reply_is_recovered_from_the_guests_record_and_no_byte_is_sent_again() {
    let (serving, _saying, client, _directory) = proven().await;
    let (_folder, tree) = tree();
    serving.fixture.guest.standin.dropping.store(1, Ordered);
    let change = Token::new();

    let reply = client
        .stage_vm(&change, &held(&tree), &tree, live(&serving))
        .await
        .unwrap();

    assert_eq!(reply["ok"], true, "{reply}");

    assert_eq!(
        stage_lanes(&serving),
        std::slice::from_ref(&change),
        "staged once"
    );
    assert_eq!(serving.fixture.guest.standin.received.load(Ordered), 1);
    assert_eq!(
        copy(&serving, &change).unwrap().contents,
        contents_of(&tree)
    );
    assert!(client.controller.pending().unwrap().is_empty());
    until("settled with the guest", || {
        settled(&serving).contains(&change)
    })
    .await;
}

/// The keeper's stage request for `snapshot`, its manifest `manifest`, as
/// `change`.
pub(super) fn stage_request(
    serving: &Serving,
    change: &Token,
    snapshot: &Snapshot,
    manifest: &[u8],
) -> Value {
    json!({"op": "stage", "change": change, "generation": live(serving),
        "request": {"op": "stage", "change": change, "name": snapshot.name,
            "digest": snapshot.digest, "entries": snapshot.entries.len(),
            "size": snapshot.size(), "attributes": snapshot.attribute_bytes(),
            "manifest": manifest.len()}})
}

/// A client of the keeper that says `request` and reads its first line.
pub(super) async fn begun(serving: &Serving, request: &Value) -> (UnixStream, Value) {
    let mut stream = UnixStream::connect(&serving.socket).await.unwrap();
    let mut line = serde_json::to_vec(request).unwrap();
    line.push(b'\n');
    stream.write_all(&line).await.unwrap();
    let mut first = Vec::new();
    let mut byte = [0_u8; 1];
    while byte[0] != b'\n' {
        stream.read_exact(&mut byte).await.unwrap();
        first.push(byte[0]);
    }
    (stream, serde_json::from_slice(&first).unwrap())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_client_that_hangs_up_halfway_has_its_lane_cancelled_and_nothing_is_placed() {
    let (serving, _saying, _client, _directory) = proven().await;
    let folder = tempfile::tempdir().unwrap();
    let file = folder.path().join("half.bin");
    std::fs::write(&file, vec![7_u8; 200_000]).unwrap();
    let snapshot = Snapshot::of(&file).unwrap();
    let manifest = serde_json::to_vec(&snapshot).unwrap();
    let change = Token::new();

    let (mut stream, first) = begun(
        &serving,
        &stage_request(&serving, &change, &snapshot, &manifest),
    )
    .await;
    assert_eq!(first, json!({"ready": true}));
    stream
        .write_all(&(manifest.len() as u64).to_be_bytes())
        .await
        .unwrap();
    stream.write_all(&manifest).await.unwrap();
    stream.write_all(&[7_u8; 100_000]).await.unwrap();
    drop(stream);

    until("the guest kept what it came to", || {
        record(&serving, &change).is_some_and(|kept| kept.is_some())
    })
    .await;
    let kept = record(&serving, &change).unwrap().unwrap();
    assert_eq!(kept["ok"], false, "{kept}");
    assert!(
        kept["error"]
            .as_str()
            .is_some_and(|error| error.starts_with("Nothing was staged")),
        "{kept}"
    );
    assert!(copy(&serving, &change).is_none(), "nothing placed");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn bytes_sent_before_the_guest_is_ready_are_refused_and_nothing_is_said() {
    let (serving, _saying, _client, _directory) = proven().await;
    let folder = tempfile::tempdir().unwrap();
    let file = folder.path().join("early.txt");
    std::fs::write(&file, b"early").unwrap();
    let snapshot = Snapshot::of(&file).unwrap();
    let manifest = serde_json::to_vec(&snapshot).unwrap();
    let change = Token::new();
    let mut said =
        serde_json::to_vec(&stage_request(&serving, &change, &snapshot, &manifest)).unwrap();
    said.extend(b"\n0123456789");

    let mut stream = UnixStream::connect(&serving.socket).await.unwrap();
    stream.write_all(&said).await.unwrap();
    let mut answered = String::new();
    stream.read_to_string(&mut answered).await.unwrap();

    let answered: Value = serde_json::from_str(answered.trim()).unwrap();
    assert_eq!(answered["code"], "bad-request", "{answered}");
    assert_eq!(answered["sent"], false);
    assert!(stage_lanes(&serving).is_empty(), "no lane opened");
    assert!(record(&serving, &change).is_none());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_stop_before_a_stage_is_placed_places_nothing_and_clears_its_claim() {
    let (serving, _saying, client, _directory) = proven().await;
    let (_folder, tree) = tree();
    let held = held(&tree);
    let change = Token::new();
    let guest = serving.fixture.guest.standin.clone();
    guest.holding.store(true, Ordered);
    let generation = live(&serving);
    let staging = tokio::spawn({
        let (client, change, tree) = (client.clone(), change.clone(), tree.clone());
        async move { client.stage_vm(&change, &held, &tree, generation).await }
    });
    until("every byte in", || guest.received.load(Ordered) == 1).await;

    client.controller.note_stop(true).unwrap();
    until("the guest stopped", || guest.stops.load(Ordered) > 0).await;
    guest.holding.store(false, Ordered);
    let reply = staging.await.unwrap().unwrap();

    assert_eq!(reply["ok"], false, "{reply}");
    assert_eq!(reply["error"], "Nothing was staged: agents were stopped");
    assert!(copy(&serving, &change).is_none(), "nothing placed");
    assert!(client.controller.pending().unwrap().is_empty(), "certain");
    until("settled with the guest", || {
        settled(&serving).contains(&change)
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_lost_removal_reply_is_recovered_as_removed_from_the_guests_record() {
    let (serving, _saying, client, _directory) = proven().await;
    let (_folder, tree) = tree();
    let change = staged(&client, &tree).await;
    let node = copy(&serving, &change).unwrap().node;
    serving.fixture.guest.standin.dropping.store(1, Ordered);

    let reply =
        crate::vm::copies::remove(&client, &change, &node, &client.verify_vm().await.unwrap())
            .await
            .unwrap();

    assert_eq!(reply["outcome"], "removed", "{reply}");
    assert_eq!(reply["copy"], json!(change));
    assert!(copy(&serving, &change).is_none());
    assert!(client.controller.pending().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_three_mib_stage_is_relayed_within_the_guests_credit_never_held_whole_by_the_keeper() {
    const SIZE: usize = 3 << 20;
    let (serving, _saying, _client, _directory) = proven().await;
    let folder = tempfile::tempdir().unwrap();
    let file = folder.path().join("large.bin");
    let written: Vec<u8> = (0..SIZE).map(|at| (at % 251) as u8).collect();
    std::fs::write(&file, &written).unwrap();
    let snapshot = Snapshot::of(&file).unwrap();
    let manifest = serde_json::to_vec(&snapshot).unwrap();
    let change = Token::new();
    let guest = serving.fixture.guest.standin.clone();
    guest.withholding.store(true, Ordered);

    let (stream, first) = begun(
        &serving,
        &stage_request(&serving, &change, &snapshot, &manifest),
    )
    .await;
    assert_eq!(first, json!({"ready": true}));
    let (mut reading, mut writing) = stream.into_split();
    let sent = Arc::new(AtomicU64::new(0));
    let sending = tokio::spawn({
        let (sent, written, manifest) = (sent.clone(), written.clone(), manifest.clone());
        async move {
            let mut bytes = (manifest.len() as u64).to_be_bytes().to_vec();
            bytes.extend(&manifest);
            bytes.extend(&written);
            for chunk in bytes.chunks(4096) {
                writing.write_all(chunk).await.unwrap();
                sent.fetch_add(chunk.len() as u64, Ordered);
            }
            writing
        }
    });
    // Once the guest's credit is spent the keeper reads no further: what
    // the client got out stalls near it, far short of the whole.
    let mut last = 0;
    loop {
        tokio::time::sleep(Duration::from_millis(500)).await;
        let now = sent.load(Ordered);
        if now == last {
            break;
        }
        last = now;
    }
    let bound = u64::from(atlas_session::credit::BULK) + (512 << 10);
    assert!(
        last < bound,
        "{last} sent while the guest gave no credit back"
    );
    guest.give_credit_back();
    let _writing = sending.await.unwrap();
    let mut answered = String::new();
    tokio::time::timeout(
        Duration::from_secs(30),
        reading.read_to_string(&mut answered),
    )
    .await
    .unwrap()
    .unwrap();

    let answered: Value = serde_json::from_str(answered.trim()).unwrap();
    assert_eq!(answered["ok"], true, "{answered}");
    assert_eq!(copy(&serving, &change).unwrap().contents, written);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_stand_in_declares_only_what_a_real_machine_guest_declares() {
    let (serving, _saying, _client, _directory) = proven().await;
    let declared = serving
        .fixture
        .keeper
        .shared
        .live()
        .unwrap()
        .control
        .declared()
        .clone();

    assert_eq!(
        declared.capabilities,
        atlas_session::compatibility::MACHINE,
        "the very names a machine guest serving everything declares"
    );
    assert!(declared.capabilities.iter().any(|name| name == STAGE));
}

#[test]
fn a_stage_names_its_change_and_generation_within_a_guests_bounds() {
    let change = Token::new();
    let stage = |manifest: u64, size: u64| {
        json!({"change": change, "generation": 3, "request": {"op": "stage",
            "change": change, "manifest": manifest, "size": size}})
    };

    assert_eq!(Asked::of(&stage(10, 20)).unwrap().bytes, 38);
    assert!(Asked::of(&stage(MOST_MANIFEST as u64 + 1, 0)).is_none());
    assert!(Asked::of(&stage(0, MOST_CONTENTS + 1)).is_none());
    let mut another = stage(1, 1);
    another["request"]["change"] = json!(Token::new());
    assert!(Asked::of(&another).is_none(), "another change");
    let mut run = stage(1, 1);
    run["request"]["op"] = json!("run");
    assert!(Asked::of(&run).is_none(), "not a stage");
}

#[test]
fn a_stages_deadline_grows_with_its_bytes() {
    assert!(deadline(0) >= GRACE + SETTLING);
    assert_eq!(deadline(RATE * 100) - deadline(0), Duration::from_secs(100));
}

/// Stage `held`, taken at `source`, through `client`: refused before any
/// lane opens, with nothing claimed and nothing received by the guest.
async fn refused_unsent(
    serving: &Serving,
    client: &crate::client::Client,
    held: &Arc<atlas_core::stage::Held>,
    source: &Path,
) {
    let refused = client
        .stage_vm(&Token::new(), held, source, live(serving))
        .await
        .unwrap_err();
    assert!(refused.error.contains("changed"), "{refused:?}");
    assert!(!refused.uncertain, "{refused:?}");
    assert!(stage_lanes(serving).is_empty(), "no lane opened");
    assert_eq!(serving.fixture.guest.standin.received.load(Ordered), 0);
    assert!(client.controller.pending().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_path_swapped_for_a_secret_after_its_snapshot_is_refused_and_none_of_it_is_sent() {
    let (serving, _saying, client, _directory) = proven().await;
    let folder = tempfile::tempdir().unwrap();
    let (file, linked) = (
        folder.path().join("notes.txt"),
        folder.path().join("linked.txt"),
    );
    let secret = folder.path().join(".env");
    std::fs::write(&file, b"notes").unwrap();
    std::fs::write(&linked, b"notes").unwrap();
    std::fs::write(&secret, b"TOKEN=secret").unwrap();
    let (by_file, by_link) = (held(&file), held(&linked));

    std::fs::rename(&file, folder.path().join("notes.old")).unwrap();
    std::fs::copy(&secret, &file).unwrap();
    std::fs::remove_file(&linked).unwrap();
    std::os::unix::fs::symlink(&secret, &linked).unwrap();

    refused_unsent(&serving, &client, &by_file, &file).await;
    refused_unsent(&serving, &client, &by_link, &linked).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_folder_above_a_snapshot_redirected_after_it_is_refused_and_none_of_it_is_sent() {
    let (serving, _saying, client, _directory) = proven().await;
    let folder = tempfile::tempdir().unwrap();
    let project = folder.path().join("project");
    std::fs::create_dir(&project).unwrap();
    let source = project.join("notes.txt");
    std::fs::write(&source, b"notes").unwrap();
    let held = held(&source);
    let elsewhere = folder.path().join("elsewhere");
    std::fs::create_dir(&elsewhere).unwrap();
    std::fs::write(elsewhere.join("notes.txt"), b"SECRET").unwrap();

    std::fs::rename(&project, folder.path().join("project.old")).unwrap();
    std::os::unix::fs::symlink(&elsewhere, &project).unwrap();

    refused_unsent(&serving, &client, &held, &source).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_file_grown_while_it_is_sent_ends_its_stage_and_nothing_is_placed() {
    let (serving, _saying, client, _directory) = proven().await;
    let folder = tempfile::tempdir().unwrap();
    let file = folder.path().join("large.bin");
    std::fs::write(&file, vec![9_u8; 3 << 20]).unwrap();
    let held = held(&file);
    let guest = serving.fixture.guest.standin.clone();
    guest.withholding.store(true, Ordered);
    let generation = live(&serving);
    let change = Token::new();
    let staging = tokio::spawn({
        let (client, change, file) = (client.clone(), change.clone(), file.clone());
        async move { client.stage_vm(&change, &held, &file, generation).await }
    });
    until("its stage lane open", || !stage_lanes(&serving).is_empty()).await;
    tokio::time::sleep(Duration::from_millis(500)).await;

    use std::io::Write;
    std::fs::OpenOptions::new()
        .append(true)
        .open(&file)
        .unwrap()
        .write_all(b"grown")
        .unwrap();
    guest.give_credit_back();
    let staged = staging.await.unwrap();

    // Ended before its last byte: some of it was sent, so what it came to
    // is the guest's to say, nothing staged, and its claim goes once the
    // guest says so.
    match &staged {
        Ok(reply) => assert_eq!(reply["ok"], false, "{reply}"),
        Err(error) => assert!(error.uncertain, "{error:?}"),
    }
    assert!(copy(&serving, &change).is_none(), "nothing placed");
    until("its claim settled from the guest's answer", || {
        client.controller.pending().unwrap().is_empty()
    })
    .await;
    assert_eq!(guest.received.load(Ordered), 0, "never all its bytes");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_client_gone_while_the_guest_withholds_credit_has_its_lane_cancelled_at_once() {
    let (serving, _saying, _client, _directory) = proven().await;
    let folder = tempfile::tempdir().unwrap();
    let file = folder.path().join("large.bin");
    std::fs::write(&file, vec![3_u8; 3 << 20]).unwrap();
    let snapshot = Snapshot::of(&file).unwrap();
    let manifest = serde_json::to_vec(&snapshot).unwrap();
    let change = Token::new();
    let guest = serving.fixture.guest.standin.clone();
    guest.withholding.store(true, Ordered);
    let (stream, first) = begun(
        &serving,
        &stage_request(&serving, &change, &snapshot, &manifest),
    )
    .await;
    assert_eq!(first, json!({"ready": true}));
    let (reading, mut writing) = stream.into_split();
    let sending = tokio::spawn(async move {
        let mut bytes = (manifest.len() as u64).to_be_bytes().to_vec();
        bytes.extend(&manifest);
        bytes.extend(vec![3_u8; 3 << 20]);
        let _ = writing.write_all(&bytes).await;
    });
    tokio::time::sleep(Duration::from_secs(1)).await;
    assert!(
        record(&serving, &change).unwrap().is_none(),
        "still under way"
    );

    sending.abort();
    drop(reading);
    let gone = std::time::Instant::now();
    until("the guest kept what it came to", || {
        record(&serving, &change).is_some_and(|kept| kept.is_some())
    })
    .await;

    assert!(
        gone.elapsed() < Duration::from_secs(5),
        "{:?}",
        gone.elapsed()
    );
    let kept = record(&serving, &change).unwrap().unwrap();
    assert!(
        kept["error"]
            .as_str()
            .is_some_and(|error| error.starts_with("Nothing was staged")),
        "{kept}"
    );
    assert!(copy(&serving, &change).is_none());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_part_overwritten_in_place_while_credit_is_withheld_never_reaches_the_guest() {
    use std::os::unix::fs::FileExt;
    let (serving, _saying, client, _directory) = proven().await;
    let folder = tempfile::tempdir().unwrap();
    let file = folder.path().join("large.bin");
    std::fs::write(&file, vec![9_u8; 3 << 20]).unwrap();
    let held = held(&file);
    let guest = serving.fixture.guest.standin.clone();
    guest.withholding.store(true, Ordered);
    let generation = live(&serving);
    let change = Token::new();
    let staging = tokio::spawn({
        let (client, change, file) = (client.clone(), change.clone(), file.clone());
        async move { client.stage_vm(&change, &held, &file, generation).await }
    });
    until("its stage lane open", || !stage_lanes(&serving).is_empty()).await;
    tokio::time::sleep(Duration::from_millis(500)).await;

    // The same file, rewritten in place where nothing was read yet.
    std::fs::OpenOptions::new()
        .write(true)
        .open(&file)
        .unwrap()
        .write_at(&[0x55; 4096], (5 << 20) / 2)
        .unwrap();
    guest.give_credit_back();
    let staged = staging.await.unwrap();

    if let Ok(reply) = &staged {
        assert_eq!(reply["ok"], false, "{reply}");
    }
    let received = guest
        .partial
        .lock()
        .unwrap()
        .get(&change)
        .cloned()
        .unwrap_or_default();
    assert!(received.len() < 3 << 20, "stopped before the changed chunk");
    assert!(
        received.iter().all(|byte| *byte == 9),
        "no changed byte crossed"
    );
    assert!(copy(&serving, &change).is_none(), "nothing placed");
}

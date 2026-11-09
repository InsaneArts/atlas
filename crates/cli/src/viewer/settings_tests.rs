use super::*;
use crate::viewer::http::bind;
use atlas_client::test_support::client_in;
use atlas_core::test_support::SETTINGS;
use serde_json::json;
use std::time::Duration;

/// A viewer following `settings` (written from SETTINGS) in `base`, with its
/// own record kept in `out`.
async fn followed(base: &Path, settings: &Path) -> (Arc<Viewer>, std::path::PathBuf) {
    std::fs::write(settings, SETTINGS).unwrap();
    let mut client = Client::new(Config::load(settings).unwrap()).unwrap();
    client.controller = atlas_control::Controller::at(60001, &base.join("state"));
    let (_, viewer) = bind(client, 0, Some(settings)).await.unwrap();

    let record = base.join("out").join(MENUBAR_RECORD);
    std::fs::create_dir_all(record.parent().unwrap()).unwrap();
    let written = json!({"url": viewer.url, "pid": std::process::id()});
    std::fs::write(&record, written.to_string()).unwrap();
    *viewer.record.lock().unwrap() = Some((record.clone(), written));
    (viewer, record)
}

/// Keep `record` from being written with a folder in its place, inside a
/// private output folder.
fn obstruct(record: &Path) {
    use std::os::unix::fs::DirBuilderExt;
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(record.parent().unwrap())
        .unwrap();
    std::fs::create_dir(record).unwrap();
}

fn mode(path: &Path) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path).unwrap().permissions().mode() & 0o777
}

#[tokio::test]
async fn an_unchanged_settings_file_retries_a_failed_viewer_record_move() {
    for (changed, folder, uid) in [
        (SETTINGS.replace("\"out\"", "\"moved\""), "moved", 60001),
        (
            SETTINGS
                .replace("60001", "60002")
                .replace("\"out\"", "\"elsewhere\""),
            "elsewhere",
            60002,
        ),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let base = std::fs::canonicalize(dir.path()).unwrap();
        let settings = base.join("u.toml");
        let (viewer, record) = followed(&base, &settings).await;

        let moved = base.join(folder).join(MENUBAR_RECORD);
        obstruct(&moved);
        std::fs::write(&settings, changed).unwrap();
        viewer.follow_settings().await;
        assert_eq!(viewer.client().config.helper_uid, uid);
        assert_eq!(viewer.client().output_dir().unwrap(), base.join(folder));
        assert!(record.exists(), "{folder}: discovery stays where it was");
        assert_eq!(viewer.record.lock().unwrap().as_ref().unwrap().0, record);

        std::fs::remove_dir(&moved).unwrap();
        viewer.follow_settings().await;
        assert!(moved.is_file(), "{folder}: a later read moves it");
        assert_eq!(mode(&moved), 0o600);
        assert_eq!(
            read_record(&moved).unwrap()["pid"],
            json!(std::process::id())
        );
        assert!(!record.exists(), "{folder}: its old record is removed");
        assert_eq!(viewer.record.lock().unwrap().as_ref().unwrap().0, moved);
    }
}

#[tokio::test]
async fn a_menu_bar_record_never_moves_into_a_folder_another_menu_bar_keeps() {
    use std::os::unix::fs::DirBuilderExt;
    let dir = tempfile::tempdir().unwrap();
    let base = std::fs::canonicalize(dir.path()).unwrap();
    let settings = base.join("u.toml");
    let (viewer, record) = followed(&base, &settings).await;
    *viewer.alone.lock().unwrap() = Some(crate::menubar::alone(record.parent().unwrap()).unwrap());
    let folder = base.join("moved");
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(&folder)
        .unwrap();
    let theirs = folder.join(MENUBAR_RECORD);
    let written = br#"{"url":"http://127.0.0.1:8799/private/","pid":4242}"#;
    std::fs::write(&theirs, written).unwrap();
    let held = crate::menubar::alone(&folder).unwrap();

    std::fs::write(&settings, SETTINGS.replace("\"out\"", "\"moved\"")).unwrap();
    viewer.follow_settings().await;
    assert_eq!(viewer.client().output_dir().unwrap(), folder);
    assert_eq!(viewer.record.lock().unwrap().as_ref().unwrap().0, record);
    assert!(record.exists(), "its record stays where it was");
    assert!(crate::menubar::alone(record.parent().unwrap()).is_err());
    assert_eq!(std::fs::read(&theirs).unwrap(), written);

    drop(held);
    viewer.follow_settings().await;
    assert_eq!(viewer.record.lock().unwrap().as_ref().unwrap().0, theirs);
    assert!(
        crate::menubar::alone(&folder).is_err(),
        "it holds where it moved"
    );
    assert!(
        crate::menubar::alone(record.parent().unwrap()).is_ok(),
        "and lets go of where it was"
    );
}

/// A picture taken eight days ago, kept in `output`'s private pictures folder.
fn expired_picture(output: &Path) -> std::path::PathBuf {
    use std::os::unix::fs::DirBuilderExt;
    let folder = output.join(atlas_client::before::FOLDER);
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&folder)
        .unwrap();
    let taken = std::time::SystemTime::now() - Duration::from_secs(8 * 24 * 60 * 60);
    let nanos = taken
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let picture = folder.join(format!("{nanos}-before.jpg"));
    std::fs::write(&picture, b"jpeg").unwrap();
    picture
}

async fn gone(picture: &Path) {
    tokio::time::timeout(Duration::from_secs(5), async {
        while picture.exists() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("{} was never pruned", picture.display()));
}

#[tokio::test]
async fn pictures_expire_in_the_output_folder_the_settings_move_to_and_the_one_before() {
    let dir = tempfile::tempdir().unwrap();
    let base = std::fs::canonicalize(dir.path()).unwrap();
    let settings = base.join("u.toml");
    let (viewer, _) = followed(&base, &settings).await;
    let first = expired_picture(&base.join("out"));
    let pruning = tokio::spawn(crate::viewer::app::prune_pictures(viewer.clone()));
    gone(&first).await;

    let left = expired_picture(&base.join("out"));
    let moved = expired_picture(&base.join("moved"));
    std::fs::write(&settings, SETTINGS.replace("\"out\"", "\"moved\"")).unwrap();
    viewer.follow_settings().await;
    gone(&moved).await;
    gone(&left).await;
    pruning.abort();
}

#[tokio::test]
async fn two_moves_while_a_pruning_runs_still_prune_every_folder_served() {
    let dir = tempfile::tempdir().unwrap();
    let base = std::fs::canonicalize(dir.path()).unwrap();
    let settings = base.join("u.toml");
    let (viewer, _) = followed(&base, &settings).await;
    // Both moves land before the pruning task reads anything, as they would
    // while a pruning runs: their signals coalesce into one.
    for folder in ["between", "last"] {
        std::fs::write(
            &settings,
            SETTINGS.replace("\"out\"", &format!("\"{folder}\"")),
        )
        .unwrap();
        viewer.follow_settings().await;
    }
    let pictures = ["out", "between", "last"].map(|folder| expired_picture(&base.join(folder)));
    let pruning = tokio::spawn(crate::viewer::app::prune_pictures(viewer.clone()));
    for picture in &pictures {
        gone(picture).await;
    }
    pruning.abort();
}

#[tokio::test]
async fn a_record_retry_publishes_the_current_output_after_another_settings_change() {
    let dir = tempfile::tempdir().unwrap();
    let base = std::fs::canonicalize(dir.path()).unwrap();
    let settings = base.join("u.toml");
    let (viewer, record) = followed(&base, &settings).await;

    let blocked = base.join("moved").join(MENUBAR_RECORD);
    obstruct(&blocked);
    std::fs::write(&settings, SETTINGS.replace("\"out\"", "\"moved\"")).unwrap();
    viewer.follow_settings().await;
    assert!(record.exists());

    // Another viewer has since recorded itself where this one was.
    let other = json!({"url": "http://127.0.0.1:1/other/", "pid": std::process::id() + 1});
    std::fs::write(&record, other.to_string()).unwrap();
    std::fs::write(&settings, SETTINGS.replace("\"out\"", "\"newest\"")).unwrap();
    viewer.follow_settings().await;

    let newest = base.join("newest").join(MENUBAR_RECORD);
    assert!(newest.is_file());
    assert_eq!(viewer.record.lock().unwrap().as_ref().unwrap().0, newest);
    assert!(blocked.is_dir(), "the older output is never published");
    assert_eq!(
        read_record(&record).unwrap(),
        other,
        "another's record stays"
    );

    viewer.follow_settings().await;
    assert!(newest.is_file());
    assert!(blocked.is_dir());
}

#[test]
fn screenshot_rows_keep_the_folder_they_were_listed_from() {
    use std::future::{Future, poll_fn};

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .max_blocking_threads(1)
        .build()
        .unwrap();
    runtime.block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let (_, viewer) = bind(client_in(dir.path()), 0, None).await.unwrap();
        let original = viewer.client();
        let output = original.output_dir().unwrap().to_path_buf();
        std::fs::write(output.join("1-capture.png"), b"old capture").unwrap();

        // Hold the only blocking worker so settings can change after
        // listing has captured its client but before it reads that folder.
        let (release, held) = std::sync::mpsc::channel();
        let (started, ready) = tokio::sync::oneshot::channel();
        let worker = tokio::task::spawn_blocking(move || {
            let _ = started.send(());
            let _ = held.recv_timeout(Duration::from_secs(5));
        });
        ready.await.unwrap();
        let mut listing = Box::pin(viewer.screenshots());
        poll_fn(|context| {
            assert!(listing.as_mut().poll(context).is_pending());
            std::task::Poll::Ready(())
        })
        .await;

        let text = SETTINGS.replace("\"out\"", "\"moved\"").into_bytes();
        let config = Config::parse(&text, &dir.path().join("u.toml")).unwrap();
        viewer.follow_output(&original, config, text).await;
        assert_ne!(
            viewer.client().config.output_dir,
            original.config.output_dir
        );
        release.send(()).unwrap();
        worker.await.unwrap();

        let listed = listing.await.unwrap();
        assert_eq!(listed["folder"], json!(output));
        assert_eq!(
            listed["screenshots"],
            json!([{"name": "1-capture.png", "takenAt": 0}])
        );
    });
}

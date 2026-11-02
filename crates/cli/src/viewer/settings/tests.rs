use super::*;
use crate::viewer::http::bind;
use atlas_client::test_support::client_in;
use atlas_core::test_support::SETTINGS;
use serde_json::json;
use std::time::Duration;

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

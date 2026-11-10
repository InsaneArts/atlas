use super::*;
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::json;

#[tokio::test]
async fn shutdown_revokes_ownership_before_input_finishes_and_keeps_unconfirmed_cleanup_pending() {
    let directory = tempfile::tempdir().unwrap();
    let client = atlas_client::test_support::client_in(directory.path());
    let controller = client.controller.clone();
    let (listener, viewer) = http::bind(client, 0, None).await.unwrap();
    let token = viewer.lease.lock().await.acquire(&controller).unwrap();
    let input_in_progress = viewer.changing().await.unwrap();
    let (shutdown, stopped) = oneshot::channel();
    let serving = tokio::spawn(serve(listener, viewer.clone(), stopped));
    shutdown.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(2), async {
        while viewer.lease.lock().await.require(&token).is_ok() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    assert!(
        !serving.is_finished(),
        "Exclusion must outlast the action that is still settling"
    );
    drop(input_in_progress);
    let error = serving.await.unwrap().unwrap_err();
    assert!(error.uncertain);
    assert!(
        !controller.pending().unwrap().is_empty(),
        "A missing native acknowledgment must remain pending for recovery"
    );
}

#[tokio::test]
async fn each_terminal_frame_is_captured_at_the_requested_size_with_fresh_session_proofs() {
    let directory = tempfile::tempdir().unwrap();
    let (client, helper) =
        atlas_client::test_support::Helper::client(directory.path(), atlas_core::test_support::UID);
    let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
    png.extend(100_u32.to_be_bytes());
    png.extend(100_u32.to_be_bytes());
    png.resize(33, 0);
    let service = TerminalService::start(
        client,
        &directory.path().join("unused.toml"),
        640,
        CaptureBackend::CoreGraphics,
    )
    .await
    .unwrap();
    let mut captures = 0;
    helper.serve(async {
        let mut keys = Vec::new();
        for _ in 0..2 {
            let url = service.url.clone();
            let frame = tokio::task::spawn_blocking(move || super::super::fetch_frame(&url,Duration::from_secs(2),1<<16)).await.unwrap().unwrap();
            keys.push(frame.key);
            assert_eq!(frame.display,(100.0,100.0));
        }
        assert_ne!(keys[0],keys[1]);
    }, |request| {
        assert_eq!(request["op"],"screenshot");
        assert_eq!(request["maxDimension"],640);
        captures += 1;
        Some(json!({"ok":true,"png":STANDARD.encode(&png),"width":100,"height":100,"scale":1.0}))
    }).await;
    assert_eq!(
        captures, 2,
        "Terminal frames must not reuse the menu bar's 400 ms snapshot"
    );
    assert!(
        helper.proofs() >= 6,
        "Every capture keeps its fresh before/after session verification"
    );
    service.shutdown().await.unwrap();
}

#[tokio::test]
async fn a_helper_session_change_is_refused_by_the_terminal_capture_pipeline() {
    let directory = tempfile::tempdir().unwrap();
    let (client, helper) =
        atlas_client::test_support::Helper::client(directory.path(), atlas_core::test_support::UID);
    let helper = helper.moving_after(2);
    let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
    png.extend(100_u32.to_be_bytes());
    png.extend(100_u32.to_be_bytes());
    png.resize(33, 0);
    let service = TerminalService::start(
        client,
        &directory.path().join("unused.toml"),
        640,
        CaptureBackend::CoreGraphics,
    )
    .await
    .unwrap();
    helper
        .serve(
            async {
                let url = service.url.clone();
                let frame = tokio::task::spawn_blocking(move || {
                    super::super::fetch_frame(&url, Duration::from_secs(2), 1 << 16)
                })
                .await
                .unwrap();
                assert!(
                    frame.is_err(),
                    "No frame is returned across a changed graphical session"
                );
            },
            |request| {
                assert_eq!(request["op"],"screenshot");
                Some(json!({"ok":true,"png":STANDARD.encode(&png),"width":100,"height":100,"scale":1.0}))
            },
        )
        .await;
    service.shutdown().await.unwrap();
}

#[tokio::test]
async fn terminal_viewing_never_replaces_the_menu_bars_private_record() {
    let directory = tempfile::tempdir().unwrap();
    let client = atlas_client::test_support::client_in(directory.path());
    let output = client.output_dir().unwrap().to_path_buf();
    let record = output.join(super::super::MENUBAR_RECORD);
    std::fs::write(&record, b"existing menu bar").unwrap();
    let service = TerminalService::start(
        client,
        &directory.path().join("settings.toml"),
        640,
        CaptureBackend::CoreGraphics,
    )
    .await
    .unwrap();
    assert!(service.url.starts_with("http://127.0.0.1:"));
    let url = service.url.clone();
    let reply = tokio::task::spawn_blocking(move || {
        super::super::fetch(&url, "", Duration::from_secs(1), 1 << 16)
    })
    .await
    .unwrap()
    .unwrap();
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&reply).unwrap()["service"],
        "atlas-viewer"
    );
    service.shutdown().await.unwrap();
    assert_eq!(std::fs::read(record).unwrap(), b"existing menu bar");
}

#[tokio::test]
async fn a_terminal_that_followed_the_settings_to_another_helper_heeds_that_helpers_sign_in() {
    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    let client = atlas_client::test_support::client_in(first.path());
    let first_controller = client.controller.clone();
    let viewer = http::configured(client, "127.0.0.1:0".to_owned(), None);
    let mut frames = TerminalFrames {
        viewer: viewer.clone(),
        stream: None,
        retired: None,
        connected: false,
        client: None,
        dimension: 800,
        backend: CaptureBackend::ScreenCaptureKit,
        reconfiguring: false,
        ring: Ring::default(),
    };
    // The settings now name another helper, whose person signs in to it.
    let other = Arc::new(atlas_client::test_support::client_in(second.path()));
    let other_controller = other.controller.clone();
    *crate::viewer::unpoisoned(&viewer.client) = other;
    let mut lease = atlas_control::Lease::default();
    let token = lease.acquire(&other_controller).unwrap();
    let hold = atlas_control::PrivateHold {
        site: "github.com".to_owned(),
        kind: atlas_control::HoldKind::SignIn,
    };
    lease.hide_screen(&other_controller, &token, &hold).unwrap();
    assert!(first_controller.screen_visible().is_ok());

    let refused = frames.next().await.err().expect("refused");

    assert_eq!(refused.code, Code::Busy);
    assert_eq!(refused.holder, Some(atlas_core::error::Holder::Person));
    other_controller.end_private().unwrap();
    lease.release();
}

use super::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[tokio::test]
async fn a_socket_left_by_a_service_that_ended_is_replaced_and_a_live_one_refuses() {
    let home = tempfile::tempdir().unwrap();
    let path = home.path().join("Atlas").join("attach.sock");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::set_permissions(path.parent().unwrap(), fs::Permissions::from_mode(0o700)).unwrap();
    drop(std::os::unix::net::UnixListener::bind(&path).unwrap());

    let live = Attach::bind_at(&path).await.unwrap();
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );

    let refused = Attach::bind_at(&path).await.err().unwrap();
    assert_eq!(refused.error, "Atlas already runs in the background");
    drop(live);
    assert!(!path.exists(), "removed with its service");
}

#[tokio::test]
async fn a_process_that_is_not_the_app_is_told_nothing() {
    let home = tempfile::tempdir().unwrap();
    let path = home.path().join("Atlas").join("attach.sock");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::set_permissions(path.parent().unwrap(), fs::Permissions::from_mode(0o700)).unwrap();
    let attach = Attach::bind_at(&path).await.unwrap();
    let client = atlas_client::test_support::client_in(home.path());
    let viewer = super::super::http::configured(client, "127.0.0.1:1".to_owned(), None);
    let (_lines, receiver) = tokio::sync::mpsc::unbounded_channel();
    // The app this service expects; the test itself runs another program.
    let app = home.path().join("AtlasApp");
    let serving = tokio::spawn(attach.serve(viewer, app, receiver));

    let mut stream = UnixStream::connect(&path).await.unwrap();
    let mut told = Vec::new();
    stream.read_to_end(&mut told).await.unwrap();

    assert!(told.is_empty(), "{}", String::from_utf8_lossy(&told));
    serving.abort();
}

#[tokio::test]
async fn a_window_the_app_went_away_without_taking_up_is_kept_for_the_next() {
    let home = tempfile::tempdir().unwrap();
    let client = atlas_client::test_support::client_in(home.path());
    let viewer = super::super::http::configured(client, "127.0.0.1:1".to_owned(), None);
    viewer.attaching.want_window();
    let (lines, mut receiver) = tokio::sync::mpsc::unbounded_channel();
    let (service, mut app) = UnixStream::pair().unwrap();
    lines.send(Line::Viewer).unwrap();
    let serving = {
        let viewer = viewer.clone();
        tokio::spawn(async move { attached(&viewer, service, 1, &mut receiver).await })
    };

    let mut told = vec![0_u8; 4096];
    let mut read = 0;
    while !String::from_utf8_lossy(&told[..read]).ends_with("viewer\n") {
        read += app.read(&mut told[read..]).await.unwrap();
    }
    drop(app);

    assert_eq!(serving.await.unwrap(), Some(Line::Viewer));
    assert!(viewer.attaching.wanted.load(SeqCst), "still wanted");
}

#[tokio::test]
async fn a_window_the_app_took_up_is_not_asked_for_again() {
    let home = tempfile::tempdir().unwrap();
    let client = atlas_client::test_support::client_in(home.path());
    let viewer = super::super::http::configured(client, "127.0.0.1:1".to_owned(), None);
    viewer.attaching.want_window();
    let (lines, mut receiver) = tokio::sync::mpsc::unbounded_channel();
    let (service, mut app) = UnixStream::pair().unwrap();
    lines.send(Line::Viewer).unwrap();
    let serving = {
        let viewer = viewer.clone();
        tokio::spawn(async move { attached(&viewer, service, 1, &mut receiver).await })
    };

    let mut told = vec![0_u8; 4096];
    let mut read = 0;
    while !String::from_utf8_lossy(&told[..read]).ends_with("viewer\n") {
        read += app.read(&mut told[read..]).await.unwrap();
    }
    app.write_all(b"\n").await.unwrap();
    let handshake = String::from_utf8_lossy(&told[..read]).into_owned();
    assert!(handshake.contains("\nprotocol 1\n"), "{handshake}");
    while viewer.attaching.wanted.load(SeqCst) {
        tokio::task::yield_now().await;
    }
    drop(app);

    assert_eq!(serving.await.unwrap(), None);
}

use super::*;
use tokio::io::AsyncWriteExt;

#[tokio::test]
async fn a_fragmented_line_is_reassembled_and_bounded() {
    let (mut client, mut server) = tokio::io::duplex(64);
    let reader = tokio::spawn(async move { read_line(&mut server, 1024).await });

    client.write_all(b"{\"op\":").await.unwrap();
    client.write_all(b"\"hello\"}\nextra").await.unwrap();
    assert_eq!(reader.await.unwrap().unwrap(), b"{\"op\":\"hello\"}");

    let (mut client, mut server) = tokio::io::duplex(4096);
    client.write_all(&[b'x'; 2048]).await.unwrap();
    let error = read_line(&mut server, 1024).await.unwrap_err();
    assert_eq!(error.error, "Request exceeds 1 KiB");
}

#[tokio::test]
async fn a_live_endpoint_is_never_taken_over() {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("run/s.sock");
    let uid = nix::unistd::getuid().as_raw();
    let (_listener, endpoint) = bind(&socket, uid, "test service", Directory::Shared(0o700))
        .await
        .unwrap();

    assert_eq!(
        fs::metadata(socket.parent().unwrap()).unwrap().mode() & 0o777,
        0o700
    );

    let second = bind(&socket, uid, "test service", Directory::Shared(0o700))
        .await
        .err()
        .unwrap();
    assert_eq!(second.code, "busy");

    drop(endpoint);
    assert!(!socket.exists(), "the endpoint removes its own socket");
}

/// A socket nothing listens on is replaced; anything that is not a
/// socket is refused and left alone.
#[tokio::test]
async fn a_stale_socket_is_replaced_and_a_non_socket_refused() {
    // A spawn on another test thread can keep the stale listener open
    // in its child until exec, so the socket would look served.
    if !crate::test_support::isolated(
        "native::endpoint::tests::a_stale_socket_is_replaced_and_a_non_socket_refused",
    ) {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("run/s.sock");
    let uid = nix::unistd::getuid().as_raw();

    fs::create_dir(socket.parent().unwrap()).unwrap();
    fs::set_permissions(socket.parent().unwrap(), fs::Permissions::from_mode(0o700)).unwrap();
    drop(std::os::unix::net::UnixListener::bind(&socket).unwrap());
    let stale = fs::symlink_metadata(&socket).unwrap().ino();
    let (_listener, endpoint) = bind(&socket, uid, "test service", Directory::Shared(0o700))
        .await
        .unwrap();

    assert_ne!(fs::symlink_metadata(&socket).unwrap().ino(), stale);
    UnixStream::connect(&socket).await.unwrap();

    drop((_listener, endpoint));
    fs::write(&socket, b"not a socket").unwrap();

    let refused = bind(&socket, uid, "test service", Directory::Shared(0o700))
        .await
        .err()
        .unwrap();
    assert_eq!(refused.code, "identity");
    assert_eq!(fs::read(&socket).unwrap(), b"not a socket");
}

#[tokio::test]
async fn a_turned_away_client_reads_why() {
    use tokio::io::AsyncReadExt;

    let (mut client, server) = UnixStream::pair().unwrap();
    turn_away(server, &serde_json::json!({"ok": false, "code": "busy"}));

    let mut reply = String::new();
    client.read_to_string(&mut reply).await.unwrap();
    let line = reply.strip_suffix('\n').unwrap();
    let value: serde_json::Value = serde_json::from_str(line).unwrap();
    assert_eq!(value, serde_json::json!({"ok": false, "code": "busy"}));
}

#[tokio::test]
async fn a_directory_whose_acl_grants_writes_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let run = dir.path().join("run");
    fs::create_dir(&run).unwrap();
    fs::set_permissions(&run, fs::Permissions::from_mode(0o700)).unwrap();

    let user = std::env::var("USER").unwrap();
    let granted = std::process::Command::new("/bin/chmod")
        .args(["+a", &format!("user:{user} allow add_file")])
        .arg(&run)
        .status()
        .unwrap();
    assert!(granted.success());

    let uid = nix::unistd::getuid().as_raw();
    let refused = bind(
        &run.join("s.sock"),
        uid,
        "test service",
        Directory::Shared(0o700),
    )
    .await
    .err()
    .unwrap();
    assert_eq!(refused.code, "identity");
}

/// A private directory is recreated with the controller's search grant,
/// and one that is open by mode or carries other grants is refused.
#[tokio::test]
async fn a_private_directory_keeps_setups_policy() {
    let dir = tempfile::tempdir().unwrap();
    let uid = nix::unistd::getuid().as_raw();
    let private = Directory::Private { controller: uid };
    let run = dir.path().join("helper");
    let (listener, endpoint) = bind(&run.join("s.sock"), uid, "test", private)
        .await
        .unwrap();

    assert_eq!(fs::metadata(&run).unwrap().mode() & 0o777, 0o700);
    let lines = crate::acl::acl_lines(&run).unwrap();
    assert_eq!(crate::acl::acl_policy(&lines, Some(uid)), Ok(true));

    drop((listener, endpoint));
    fs::set_permissions(&run, fs::Permissions::from_mode(0o755)).unwrap();
    let open = bind(&run.join("s.sock"), uid, "test", private).await;
    assert_eq!(open.err().unwrap().code, "identity");

    fs::set_permissions(&run, fs::Permissions::from_mode(0o700)).unwrap();
    let user = std::env::var("USER").unwrap();
    std::process::Command::new("/bin/chmod")
        .args(["+a", &format!("user:{user} allow list")])
        .arg(&run)
        .status()
        .unwrap();

    let extra = bind(&run.join("s.sock"), uid, "test", private).await;
    assert_eq!(extra.err().unwrap().code, "identity");
}

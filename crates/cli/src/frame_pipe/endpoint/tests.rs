use super::*;
use std::{os::unix::fs::symlink, process::Command};

/// Held by tests that spawn processes and by tests that need a closed socket
/// to stay closed: a child forked meanwhile holds every open socket until it
/// executes, so a listener closed here could still answer from it.
static FORKS: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Give `path` an ACL entry that grants nothing its owner lacks.
fn add_acl(path: &Path) {
    let user = nix::unistd::User::from_uid(nix::unistd::getuid())
        .unwrap()
        .unwrap()
        .name;
    assert!(
        Command::new("/bin/chmod")
            .args(["+a", &format!("user:{user} allow readattr")])
            .arg(path)
            .status()
            .unwrap()
            .success()
    );
}

fn me() -> u32 {
    nix::unistd::getuid().as_raw()
}

/// A short base under /tmp, so socket paths fit a Unix address.
fn base() -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix("fp")
        .tempdir_in("/tmp")
        .unwrap()
}

#[test]
fn the_owner_is_the_controller_and_helper_pair_alone() {
    let key = owner_key(501, 502);
    assert_eq!(key, owner_key(501, 502));
    assert_ne!(key, owner_key(502, 501));
    assert_ne!(key, owner_key(501, 503));
    let mut hash = Sha256::new();
    hash.update(b"atlas-frame-owner-v1\0\0\0\x01\xf5\0\0\x01\xf6");
    assert_eq!(
        key,
        <[u8; 32]>::from(hash.finalize()),
        "the documented bytes"
    );
    let path = folder(Path::new(BASE), 501, &key);
    let name = path.file_name().unwrap().to_str().unwrap();
    assert!(name.starts_with("atlas-fp.501.") && name.len() == "atlas-fp.501.".len() + 16);
}

#[tokio::test]
async fn one_service_binds_private_sockets_and_a_second_is_refused() {
    let base = base();
    let (_listeners, endpoint) = bind_in(base.path(), me(), 7001).await.unwrap();
    let folder = endpoint.folder.clone();
    assert_eq!(fs::metadata(&folder).unwrap().mode() & 0o777, 0o700);
    for name in ["c.sock", "f.sock"] {
        let metadata = fs::symlink_metadata(folder.join(name)).unwrap();
        assert!(metadata.file_type().is_socket());
        assert_eq!(metadata.mode() & 0o777, 0o600);
    }
    assert_eq!(
        bind_in(base.path(), me(), 7001).await.err().unwrap().code,
        Code::Busy
    );
    drop(endpoint);
    assert!(
        !folder.join("c.sock").exists(),
        "its own sockets go with it"
    );
    assert!(
        bind_in(base.path(), me(), 7001).await.is_ok(),
        "then it binds again"
    );
}

#[tokio::test]
async fn an_unsafe_folder_is_refused_and_left_as_it_is() {
    let base = base();
    let path = folder(base.path(), me(), &owner_key(me(), 7002));
    fs::create_dir(&path).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(
        bind_in(base.path(), me(), 7002).await.err().unwrap().code,
        Code::Identity
    );
    assert_eq!(
        fs::metadata(&path).unwrap().mode() & 0o777,
        0o755,
        "not repaired"
    );

    let elsewhere = base.path().join("elsewhere");
    fs::create_dir(&elsewhere).unwrap();
    let linked = folder(base.path(), me(), &owner_key(me(), 7003));
    symlink(&elsewhere, &linked).unwrap();
    assert_eq!(
        bind_in(base.path(), me(), 7003).await.err().unwrap().code,
        Code::Identity
    );
}

#[tokio::test]
async fn a_left_socket_is_cleared_only_when_no_one_answers_and_a_successor_is_never_removed() {
    let _forks = FORKS.lock().await;
    let base = base();
    let (listeners, endpoint) = bind_in(base.path(), me(), 7004).await.unwrap();
    let control = endpoint.socket("c.sock");
    let frame = endpoint.socket("f.sock");
    drop((listeners, endpoint));

    // A crash leaves a socket behind with nothing listening: it is cleared.
    drop(std::os::unix::net::UnixListener::bind(&control).unwrap());
    fs::set_permissions(&control, fs::Permissions::from_mode(0o600)).unwrap();
    assert!(control.exists());
    let (listeners, endpoint) = bind_in(base.path(), me(), 7004).await.unwrap();

    // A socket bound in its place later is another's, and stays.
    fs::remove_file(&frame).unwrap();
    let successor = UnixListener::bind(&frame).unwrap();
    fs::set_permissions(&frame, fs::Permissions::from_mode(0o600)).unwrap();
    drop((listeners, endpoint));
    assert!(
        frame.exists(),
        "another socket at the same path is not this one's"
    );
    assert!(!control.exists());

    // One that answers is never taken over.
    assert_eq!(
        bind_in(base.path(), me(), 7004).await.err().unwrap().code,
        Code::Busy
    );
    assert!(
        !control.exists(),
        "a refused start leaves none of its own sockets"
    );
    drop(successor);
}

#[tokio::test]
async fn a_folder_another_owner_claimed_or_a_loose_lock_file_is_refused() {
    let base = base();
    let (listeners, endpoint) = bind_in(base.path(), me(), 7005).await.unwrap();
    let lock = endpoint.folder.join("owner.lock");
    drop((listeners, endpoint));
    let recorded = fs::read_to_string(&lock).unwrap();
    assert!(
        recorded.ends_with(" 7005\n") && recorded.len() == 64 + 6,
        "{recorded:?}"
    );

    // Only a prefix of the key names the folder: a collision is caught here.
    fs::write(&lock, format!("{} 7005\n", "0".repeat(64))).unwrap();
    assert_eq!(
        bind_in(base.path(), me(), 7005).await.err().unwrap().code,
        Code::Identity
    );

    fs::write(&lock, recorded).unwrap();
    fs::set_permissions(&lock, fs::Permissions::from_mode(0o644)).unwrap();
    assert_eq!(
        bind_in(base.path(), me(), 7005).await.err().unwrap().code,
        Code::Identity
    );
    assert_eq!(
        fs::metadata(&lock).unwrap().mode() & 0o777,
        0o644,
        "not repaired"
    );
}

#[tokio::test]
async fn a_lock_or_left_socket_with_an_acl_is_refused_and_kept() {
    let _forks = FORKS.lock().await;
    let base = base();
    let (listeners, endpoint) = bind_in(base.path(), me(), 7006).await.unwrap();
    let (lock, control) = (
        endpoint.folder.join("owner.lock"),
        endpoint.socket("c.sock"),
    );
    drop((listeners, endpoint));

    drop(std::os::unix::net::UnixListener::bind(&control).unwrap());
    fs::set_permissions(&control, fs::Permissions::from_mode(0o600)).unwrap();
    add_acl(&control);
    assert_eq!(
        bind_in(base.path(), me(), 7006).await.err().unwrap().code,
        Code::Identity
    );
    assert!(control.exists(), "not cleared");
    fs::remove_file(&control).unwrap();

    add_acl(&lock);
    assert_eq!(
        bind_in(base.path(), me(), 7006).await.err().unwrap().code,
        Code::Identity
    );
    assert!(bind_in(base.path(), me(), 7008).await.is_ok());

    // A folder of its own, so only its ACL is wrong.
    let (listeners, endpoint) = bind_in(base.path(), me(), 7009).await.unwrap();
    let folder = endpoint.folder.clone();
    drop((listeners, endpoint));
    add_acl(&folder);
    assert_eq!(
        bind_in(base.path(), me(), 7009).await.err().unwrap().code,
        Code::Identity
    );
    assert!(!folder.join("c.sock").exists());
}

#[tokio::test]
async fn a_lock_or_socket_that_is_a_link_is_refused_and_its_target_untouched() {
    let base = base();
    let target = base.path().join("target");
    fs::write(&target, "kept").unwrap();
    fs::set_permissions(&target, fs::Permissions::from_mode(0o644)).unwrap();

    let (listeners, endpoint) = bind_in(base.path(), me(), 7010).await.unwrap();
    let lock = endpoint.folder.join("owner.lock");
    drop((listeners, endpoint));
    fs::remove_file(&lock).unwrap();
    symlink(&target, &lock).unwrap();
    assert!(bind_in(base.path(), me(), 7010).await.is_err());

    let (listeners, endpoint) = bind_in(base.path(), me(), 7011).await.unwrap();
    let control = endpoint.socket("c.sock");
    drop((listeners, endpoint));
    symlink(&target, &control).unwrap();
    assert_eq!(
        bind_in(base.path(), me(), 7011).await.err().unwrap().code,
        Code::Identity
    );
    assert_eq!(fs::read_to_string(&target).unwrap(), "kept");
    assert_eq!(fs::metadata(&target).unwrap().mode() & 0o777, 0o644);
}

#[tokio::test]
async fn sockets_in_a_folder_put_in_its_place_are_never_removed() {
    let base = base();
    let (listeners, endpoint) = bind_in(base.path(), me(), 7007).await.unwrap();
    let folder = endpoint.folder.clone();
    let moved = base.path().join("moved");
    fs::rename(&folder, &moved).unwrap();
    fs::create_dir(&folder).unwrap();
    let stranger = folder.join("c.sock");
    let _held = UnixListener::bind(&stranger).unwrap();
    drop((listeners, endpoint));
    assert!(stranger.exists(), "another folder's socket stays");
    assert!(moved.join("f.sock").exists(), "a moved folder is let be");
}

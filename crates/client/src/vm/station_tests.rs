use super::*;
use crate::vm::tests::{Fake, desktop};
use std::os::unix::fs::PermissionsExt;

const UID: u32 = 501;

fn folder() -> Option<Entry> {
    Some(Entry {
        kind: Kind::Folder,
        uid: UID,
        mode: 0o040_700,
    })
}

fn socket() -> Option<Entry> {
    Some(Entry {
        kind: Kind::Socket,
        uid: UID,
        mode: 0o140_755,
    })
}

#[test]
fn a_socket_of_this_accounts_in_its_private_vm_folder_may_be_dialled() {
    assert_eq!(judge_path(folder(), socket(), UID), Ok(()));
}

#[test]
fn a_station_path_that_leads_out_of_the_vm_folder_is_refused() {
    let link = Some(Entry {
        kind: Kind::Link,
        uid: UID,
        mode: 0o120_755,
    });

    assert_eq!(
        judge_path(folder(), link, UID),
        Err(Untrusted::OutsideFolder)
    );
}

#[test]
fn a_vm_folder_that_is_not_private_to_this_account_is_refused() {
    let shared = |kind, uid, mode| Some(Entry { kind, uid, mode });

    for folder in [
        shared(Kind::Folder, UID, 0o040_750),
        shared(Kind::Folder, UID, 0o040_702),
        shared(Kind::Folder, UID + 1, 0o040_700),
        shared(Kind::Link, UID, 0o120_700),
        shared(Kind::Other, UID, 0o100_700),
    ] {
        assert_eq!(
            judge_path(folder, socket(), UID),
            Err(Untrusted::FolderNotPrivate),
            "{folder:?}"
        );
    }
    assert_eq!(judge_path(None, socket(), UID), Err(Untrusted::NoFolder));
}

#[test]
fn a_station_path_that_is_no_socket_of_this_accounts_is_refused() {
    let entry = |kind, uid| Some(Entry { kind, uid, mode: 0 });

    assert_eq!(
        judge_path(folder(), entry(Kind::Other, UID), UID),
        Err(Untrusted::NotASocket)
    );
    assert_eq!(
        judge_path(folder(), entry(Kind::Socket, UID + 1), UID),
        Err(Untrusted::NotASocket)
    );
    assert_eq!(judge_path(folder(), None, UID), Err(Untrusted::NoSocket));
}

#[test]
fn a_listener_of_another_identity_or_account_is_refused() {
    assert_eq!(judge_peer(None, Some(80), UID), Err(Untrusted::NotAtlasVm));
    assert_eq!(
        judge_peer(Some((80, UID + 1)), Some(80), UID),
        Err(Untrusted::NotAtlasVm)
    );
}

#[test]
fn a_listener_whose_pid_is_not_the_one_launchd_runs_for_the_vm_is_refused() {
    assert_eq!(
        judge_peer(Some((81, UID)), Some(80), UID),
        Err(Untrusted::NotLaunchds)
    );
    assert_eq!(
        judge_peer(Some((81, UID)), None, UID),
        Err(Untrusted::NotLaunchds)
    );
    assert_eq!(judge_peer(Some((80, UID)), Some(80), UID), Ok(()));
}

/// A home short enough for a socket path in it, with `desktop()`'s VM
/// folder made private.
fn short_home() -> (tempfile::TempDir, Vms) {
    let home = tempfile::Builder::new().tempdir_in("/tmp").unwrap();
    let vms = Vms::under(home.path(), nix::unistd::getuid().as_raw());
    let folder = vms.folder(&desktop());
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o700)).unwrap();
    (home, vms)
}

#[test]
fn the_facts_on_disk_are_read_from_each_entry_never_followed() {
    let (_home, vms) = short_home();
    let (folder, uid) = (vms.folder(&desktop()), vms.uid);
    let station = folder.join("station");
    let _listening = std::os::unix::net::UnixListener::bind(&station).unwrap();

    assert_eq!(
        judge_path(Entry::at(&folder), Entry::at(&station), uid),
        Ok(())
    );

    std::fs::remove_file(&station).unwrap();
    let elsewhere = tempfile::Builder::new().tempdir_in("/tmp").unwrap();
    let outside = elsewhere.path().join("station");
    let _other = std::os::unix::net::UnixListener::bind(&outside).unwrap();
    std::os::unix::fs::symlink(&outside, &station).unwrap();
    assert_eq!(
        judge_path(Entry::at(&folder), Entry::at(&station), uid),
        Err(Untrusted::OutsideFolder)
    );
}

#[test]
fn a_socket_this_test_listens_on_is_not_atlas_vm_and_is_never_trusted() {
    let (_home, vms) = short_home();
    let station = vms.folder(&desktop()).join("station");
    let _listening = std::os::unix::net::UnixListener::bind(&station).unwrap();
    let system = Fake::new();
    *system.job.lock().unwrap() = Some(atlas_core::launchd::Loaded {
        pid: Some(std::process::id()),
        ..atlas_core::launchd::Loaded::default()
    });

    let refused = trusted_with(&vms, &system, signed_vm, &desktop(), "work").unwrap_err();

    assert!(
        matches!(
            refused,
            Refused::Untrusted {
                why: Untrusted::NotAtlasVm,
                ..
            }
        ),
        "{refused:?}"
    );
    let error = refused.into_error("work");
    assert_eq!(error.code, Code::Identity);
    assert!(
        error.error.starts_with("work was not dialled: "),
        "named by its name: {}",
        error.error
    );
    assert!(
        error.error.contains(&Untrusted::NotAtlasVm.to_string()),
        "{}",
        error.error
    );
}

#[test]
fn a_vm_folder_made_shared_is_refused_and_stays_refused_until_its_facts_change() {
    let (_home, vms) = short_home();
    let folder = vms.folder(&desktop());
    let _listening = std::os::unix::net::UnixListener::bind(folder.join("station")).unwrap();
    std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o755)).unwrap();
    let system = Fake::new();

    let Err(Refused::Untrusted { why, facts }) =
        trusted_with(&vms, &system, signed_vm, &desktop(), "work")
    else {
        panic!("a shared folder was trusted");
    };

    assert_eq!(why, Untrusted::FolderNotPrivate);
    assert_eq!(facts_with(&vms, &system, &desktop()).unwrap(), facts);
    std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o700)).unwrap();
    assert_ne!(facts_with(&vms, &system, &desktop()).unwrap(), facts);
}

#[test]
fn a_socket_made_again_or_another_runner_is_a_change_of_facts() {
    let (_home, vms) = short_home();
    let station = vms.folder(&desktop()).join("station");
    let listening = std::os::unix::net::UnixListener::bind(&station).unwrap();
    let system = Fake::new();
    let before = facts_with(&vms, &system, &desktop()).unwrap();

    drop(listening);
    std::fs::remove_file(&station).unwrap();
    let _again = std::os::unix::net::UnixListener::bind(&station).unwrap();
    let remade = facts_with(&vms, &system, &desktop()).unwrap();
    *system.job.lock().unwrap() = Some(atlas_core::launchd::Loaded {
        pid: Some(4242),
        ..atlas_core::launchd::Loaded::default()
    });
    let restarted = facts_with(&vms, &system, &desktop()).unwrap();

    assert_ne!(before, remade);
    assert_ne!(remade, restarted);
}

#[test]
fn a_station_socket_nothing_answers_on_is_said_unreached_by_the_vms_name() {
    let (_home, vms) = short_home();
    let station = vms.folder(&desktop()).join("station");
    // Bound and gone: the socket is there, and nothing listens on it.
    drop(std::os::unix::net::UnixListener::bind(&station).unwrap());
    let system = Fake::new();
    *system.job.lock().unwrap() = Some(atlas_core::launchd::Loaded {
        pid: Some(std::process::id()),
        ..atlas_core::launchd::Loaded::default()
    });

    let refused = trusted_with(&vms, &system, signed_vm, &desktop(), "work").unwrap_err();

    let Refused::Failed(error) = refused else {
        panic!("not a failure to reach it: {refused:?}");
    };
    assert_eq!(error.code, Code::Transport);
    assert_eq!(error.error, "Could not reach work.");
    assert!(!error.error.contains(desktop().as_str()));
}

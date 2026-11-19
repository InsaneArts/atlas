use super::*;
use atlas_session::{authority::Retired, clock::Continuous, generation::Generation};

fn holding() -> Holding {
    let authority = Authority::new(Generation(1), Arc::new(Continuous), Box::new(|_, _| {}));
    Holding {
        lanes: Arc::new(Lanes::new(authority.clone())),
        authority,
        lane: LaneId(7),
    }
}

#[test]
fn an_update_is_handed_over_only_while_its_lane_and_session_are_live() {
    assert!(holding().live());

    let cancelled = holding();
    cancelled.lanes.cancel(cancelled.lane);
    assert!(!cancelled.live(), "handed over from a cancelled lane");

    let ended = holding();
    ended.authority.retire(Retired::Revoked);
    assert!(!ended.live(), "handed over once its session ended");
}

/// An app bundle as an update stages one, its executable saying `says`.
fn bundle(says: &str) -> (tempfile::TempDir, std::path::PathBuf) {
    use std::os::unix::fs::PermissionsExt;
    let folder = tempfile::tempdir().unwrap();
    let bundle = folder.path().join("Atlas.app");
    std::fs::create_dir_all(bundle.join("Contents/MacOS")).unwrap();
    std::fs::write(bundle.join("Contents/Info.plist"), b"<plist/>").unwrap();
    let executable = bundle.join("Contents/MacOS/atlas");
    std::fs::write(&executable, format!("#!/bin/sh\necho {says}\n")).unwrap();
    std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o755)).unwrap();
    (folder, bundle)
}

#[test]
fn an_update_takes_the_staged_bundle_only_while_it_is_the_one_its_manifest_digest_names() {
    let (_folder, staged) = bundle("a");
    let expect = Snapshot::of(&staged).unwrap().digest;

    let taken = bound(&staged, &expect).unwrap();

    assert_eq!(taken.digest, expect);
    assert_eq!(taken, Snapshot::of(&staged).unwrap());
}

#[test]
fn another_bundle_swapped_in_where_the_update_was_staged_is_refused_as_changed() {
    let (_folder, staged) = bundle("a");
    let expect = Snapshot::of(&staged).unwrap().digest;
    let (_other, swapped) = bundle("b");
    std::fs::remove_dir_all(&staged).unwrap();
    std::fs::rename(&swapped, &staged).unwrap();

    assert_eq!(bound(&staged, &expect), Err(Unbound::Changed));
    let reply = changed();
    assert_eq!(reply["ok"], false, "{reply}");
    assert_eq!(reply["update"], "changed", "{reply}");
    assert_ne!(reply["uncertain"], true, "{reply}");
}

#[test]
fn a_staged_bundle_that_cannot_be_read_is_refused_in_its_own_words() {
    let folder = tempfile::tempdir().unwrap();

    let refused = bound(&folder.path().join("Atlas.app"), &"ab".repeat(32)).unwrap_err();

    assert!(matches!(refused, Unbound::Unreadable(_)), "{refused:?}");
}

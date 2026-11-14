use super::*;
use atlas_core::{
    desktop::{Desktop, DesktopId},
    files::Token,
    session::{key::PeerKey, paired::Paired},
    user::User,
};
use std::os::unix::fs::PermissionsExt;

fn paired() -> Enrolment {
    Enrolment::Paired(Paired {
        station: "f".repeat(64),
        guest: PeerKey::from_bytes([7; 32]),
        uid: 501,
        boot: "boot".to_owned(),
        change: Token::new(),
    })
}

fn desktop() -> DesktopId {
    Desktop::from(User { uid: 502 }).id()
}

/// A private folder of its own, holding the station's desktops folder.
fn private() -> tempfile::TempDir {
    let folder = tempfile::tempdir().unwrap();
    fs::set_permissions(folder.path(), fs::Permissions::from_mode(0o700)).unwrap();
    folder
}

fn mode(path: &Path) -> u32 {
    fs::symlink_metadata(path).unwrap().permissions().mode() & 0o777
}

#[test]
fn no_record_is_none_and_a_kept_one_reads_back_in_folders_made_private() {
    let folder = private();
    let desktops = folder.path().join("desktops");
    let file = PairingFile::of(&desktops, &desktop());

    assert_eq!(file.read().unwrap(), None);
    let record = paired();
    file.replace(None, &record).unwrap();

    assert_eq!(file.read().unwrap(), Some(record));
    let own = desktops.join(desktop().as_str());
    assert_eq!(mode(&own.join("pairing.json")), 0o600);
    assert_eq!(mode(&desktops), 0o700);
    assert_eq!(mode(&own), 0o700);
}

#[test]
fn a_record_that_changed_since_it_was_read_is_never_overwritten() {
    let folder = private();
    let file = PairingFile::of(folder.path().join("desktops"), &desktop());
    let first = paired();
    file.replace(None, &first).unwrap();
    let revoked = Enrolment::Revoked {
        change: Token::new(),
    };
    file.replace(Some(&first), &revoked).unwrap();

    // A completion that read the pairing before it was revoked.
    let late = file.replace(Some(&first), &paired()).unwrap_err();

    assert_eq!(late.code, Code::Busy);
    assert_eq!(file.read().unwrap(), Some(revoked));
}

#[test]
fn a_damaged_record_or_one_from_a_later_version_is_refused_not_absent() {
    let folder = private();
    let file = PairingFile::of(folder.path().join("desktops"), &desktop());
    file.replace(None, &paired()).unwrap();
    let path = folder
        .path()
        .join("desktops")
        .join(desktop().as_str())
        .join("pairing.json");
    let mut later = serde_json::to_value(paired()).unwrap();
    later["later"] = serde_json::json!(true);
    let mut unknown = serde_json::to_value(paired()).unwrap();
    unknown["state"] = serde_json::json!("trusted");
    for text in [
        "{\"state\":\"pai".to_owned(),
        later.to_string(),
        unknown.to_string(),
    ] {
        atlas_core::files::write_atomic(&path, text.as_bytes(), 0o600).unwrap();

        assert_eq!(file.read().unwrap_err().code, Code::Config, "{text}");
    }
    let unchanged = paired();
    atlas_core::files::write_atomic(&path, &serde_json::to_vec(&unchanged).unwrap(), 0o600)
        .unwrap();
    assert_eq!(file.read().unwrap(), Some(unchanged));
}

#[test]
fn a_record_others_may_change_or_one_in_a_shared_folder_is_refused() {
    let folder = private();
    let desktops = folder.path().join("desktops");
    let file = PairingFile::of(&desktops, &desktop());
    file.replace(None, &paired()).unwrap();
    let own = desktops.join(desktop().as_str());

    fs::set_permissions(own.join("pairing.json"), fs::Permissions::from_mode(0o620)).unwrap();
    assert!(file.read().is_err(), "a record others may write");
    fs::set_permissions(own.join("pairing.json"), fs::Permissions::from_mode(0o600)).unwrap();

    for shared in [&own, &desktops] {
        fs::set_permissions(shared, fs::Permissions::from_mode(0o770)).unwrap();
        assert!(file.read().is_err(), "{}", shared.display());
        assert!(file.replace(None, &paired()).is_err());
        fs::set_permissions(shared, fs::Permissions::from_mode(0o700)).unwrap();
    }
}

#[test]
fn a_link_in_place_of_either_folder_is_refused() {
    let folder = private();
    let real = private();
    // The desktops folder a link.
    let linked = folder.path().join("desktops");
    std::os::unix::fs::symlink(real.path(), &linked).unwrap();
    assert!(
        PairingFile::of(&linked, &desktop())
            .replace(None, &paired())
            .is_err()
    );

    // The desktop's own folder a link.
    let desktops = folder.path().join("others");
    fs::create_dir(&desktops).unwrap();
    fs::set_permissions(&desktops, fs::Permissions::from_mode(0o700)).unwrap();
    std::os::unix::fs::symlink(real.path(), desktops.join(desktop().as_str())).unwrap();
    let file = PairingFile::of(&desktops, &desktop());
    assert!(file.replace(None, &paired()).is_err());
    assert!(file.read().is_err());
}

#[test]
fn a_change_another_station_process_holds_waits_and_then_says_busy() {
    let folder = private();
    let file = PairingFile::of(folder.path().join("desktops"), &desktop());
    file.replace(None, &paired()).unwrap();
    let _held = file.lock().unwrap();

    let started = std::time::Instant::now();
    let waited =
        PairingFile::of(folder.path().join("desktops"), &desktop()).replace(None, &paired());

    assert_eq!(waited.unwrap_err().code, Code::Busy);
    assert!(started.elapsed() >= PATIENCE);
}

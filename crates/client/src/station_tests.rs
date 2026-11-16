use super::*;
use std::os::unix::fs::PermissionsExt;

fn vm() -> DesktopId {
    DesktopId::parse(&"a".repeat(24)).unwrap()
}

fn mode(path: &Path) -> u32 {
    std::fs::symlink_metadata(path)
        .unwrap()
        .permissions()
        .mode()
        & 0o777
}

#[test]
fn a_vms_station_folder_is_made_private_under_atlass_own_folder() {
    let home = tempfile::tempdir().unwrap();
    let station = Station::under(home.path());

    let folder = station.folder(&vm()).unwrap();

    assert_eq!(
        folder,
        home.path()
            .join("Library/Application Support/Atlas/Station")
            .join(format!("vm-{}", vm().as_str()))
    );
    assert_eq!(mode(&folder), 0o700);
    assert_eq!(mode(folder.parent().unwrap()), 0o700);
    assert_eq!(
        station.key_lock().unwrap(),
        folder.parent().unwrap().join(".key.lock")
    );
}

#[test]
fn a_vms_pairing_record_is_kept_in_its_station_folder() {
    use atlas_session::pairing::{Enrolment, StationRecords};
    let home = tempfile::tempdir().unwrap();
    let station = Station::under(home.path());
    let record = Enrolment::Revoked {
        change: atlas_core::files::Token::new(),
    };

    station
        .pairing(&vm())
        .unwrap()
        .replace(None, &record)
        .unwrap();

    let kept = station.folder(&vm()).unwrap().join("pairing.json");
    assert_eq!(mode(&kept), 0o600);
    assert_eq!(
        station.pairing(&vm()).unwrap().read().unwrap(),
        Some(record)
    );
}

#[test]
fn a_station_folder_others_may_read_or_a_link_in_its_place_is_refused() {
    let home = tempfile::tempdir().unwrap();
    let station = Station::under(home.path());
    let folder = station.folder(&vm()).unwrap();

    std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(station.folder(&vm()).unwrap_err().code, Code::State);

    std::fs::remove_dir(&folder).unwrap();
    let elsewhere = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(elsewhere.path(), &folder).unwrap();
    assert_eq!(station.folder(&vm()).unwrap_err().code, Code::State);
}

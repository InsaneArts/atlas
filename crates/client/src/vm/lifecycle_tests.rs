use super::*;
use std::os::unix::fs::PermissionsExt;

/// A lifecycle in a home of its own.
pub(super) fn lifecycle() -> (tempfile::TempDir, Lifecycle) {
    let home = tempfile::tempdir().unwrap();
    let lifecycle = Lifecycle::under(home.path());
    (home, lifecycle)
}

/// The Atlas a test's records say made a VM's items.
pub(super) fn provenance() -> Provenance {
    Provenance {
        program: PathBuf::from("/Applications/Atlas.app/Contents/MacOS/atlas"),
        requirement: "identifier \"com.insanearts.atlas\"".to_owned(),
    }
}

#[test]
fn the_lifecycle_is_kept_beside_the_vms_station_folders_never_in_one() {
    let (home, lifecycle) = lifecycle();
    let station = home
        .path()
        .join("Library/Application Support/Atlas/Station");

    let folder = lifecycle.folder().unwrap();

    assert_eq!(folder, station.join("lifecycle"));
    for private in [&station, &station.join("lifecycle")] {
        let mode = std::fs::metadata(private).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o700);
    }
}

#[test]
fn a_file_made_only_where_none_is_is_made_once_and_never_replaced() {
    let (_home, lifecycle) = lifecycle();
    let path = lifecycle.folder().unwrap().join("one");

    assert!(create(&path, &1).unwrap());
    assert!(!create(&path, &2).unwrap());

    assert_eq!(read::<u32>(&path, "One").unwrap(), Some(1));
    let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);
}

#[test]
fn a_record_that_cannot_be_read_is_never_taken_for_absent() {
    let (_home, lifecycle) = lifecycle();
    let path = lifecycle.folder().unwrap().join("broken");
    std::fs::write(&path, b"{").unwrap();

    assert!(read::<u32>(&path, "Broken").is_err());
    assert_eq!(
        read::<u32>(&path.with_file_name("none"), "None").unwrap(),
        None
    );
}

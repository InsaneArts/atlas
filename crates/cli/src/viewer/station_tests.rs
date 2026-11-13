use super::*;

#[test]
fn one_service_holds_the_account_and_records_where_it_is() {
    let home = tempfile::tempdir().unwrap();
    let settings = home.path().join("atlas.toml");
    let station = Station::claim_in(home.path(), &settings, 8765, "first").unwrap();

    let record: Value =
        serde_json::from_slice(&fs::read(home.path().join(FOLDER).join(RECORD)).unwrap()).unwrap();
    assert_eq!(record["port"], 8765);
    assert_eq!(record["incarnation"], "first");
    assert_eq!(record["protocol"], super::super::PROTOCOL);
    assert_eq!(record["settings"], json!(settings));

    let refused = Station::claim_in(home.path(), &settings, 8766, "second")
        .err()
        .unwrap();
    assert_eq!(refused.error, "Atlas already runs in the background");

    drop(station);
    assert!(!home.path().join(FOLDER).join(RECORD).exists());
    assert!(Station::claim_in(home.path(), &settings, 8766, "second").is_ok());
}

#[test]
fn a_service_never_removes_a_later_ones_record() {
    let home = tempfile::tempdir().unwrap();
    let settings = home.path().join("atlas.toml");
    let station = Station::claim_in(home.path(), &settings, 8765, "first").unwrap();
    let record = home.path().join(FOLDER).join(RECORD);
    fs::write(&record, json!({"incarnation": "later"}).to_string()).unwrap();

    drop(station);

    assert!(record.exists());
}

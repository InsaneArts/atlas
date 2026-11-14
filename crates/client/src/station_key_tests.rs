use super::*;
use atlas_session::keys::own_key;

#[test]
fn a_kept_key_of_the_wrong_length_is_damaged_never_replaced() {
    let error = decoded(&[7; KEY_LEN - 1]).unwrap_err();

    assert_eq!(error.code, Code::Identity);
    assert_eq!(decoded(&[7; KEY_LEN]).unwrap().as_bytes(), &[7; KEY_LEN]);
}

#[test]
#[ignore = "writes to the account's own Keychain; run by hand on a Mac"]
fn the_station_keeps_one_key_across_loads() {
    let service: &'static str =
        Box::leak(format!("com.atlas.tests.station-key.{}", std::process::id()).into_boxed_str());
    let item = Item {
        service,
        account: "station session key",
    };
    let store = StationKey::at(item);

    let first = own_key(&store).unwrap();
    let again = own_key(&store).unwrap();

    assert_eq!(first.public(), again.public());
    keychain::delete(item).unwrap();
}

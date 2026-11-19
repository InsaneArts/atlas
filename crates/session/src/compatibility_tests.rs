use super::*;
use crate::control;
use serde_json::json;
use tokio::io::{AsyncWriteExt, BufReader};

fn declaring(protocol: u32, capabilities: &[&str], version: &str) -> Declared {
    Declared {
        protocol,
        capabilities: capabilities.iter().map(|name| (*name).to_owned()).collect(),
        version: version.to_owned(),
    }
}

#[test]
fn a_guest_missing_a_needed_capability_is_usable_only_for_what_it_declares() {
    let guest = declaring(PROTOCOL, &[CHANGES, LEFTOVERS], "0.1.3");

    assert_eq!(judge(&guest, &[CHANGES]), Usable::Usable);
    assert_eq!(
        judge(&guest, &[CHANGES, STAGE]),
        Usable::MissingCapabilities {
            missing: vec![STAGE.to_owned()]
        }
    );
}

#[test]
fn another_protocol_is_incompatible_and_says_which_side_to_update() {
    let older = judge(&declaring(PROTOCOL - 1, &[], "0.1.0"), &[]);
    let newer = judge(&declaring(PROTOCOL + 1, &[], "9.0.0"), &[]);

    assert!(older.refusal().unwrap().error.contains("on the desktop"));
    assert!(newer.refusal().unwrap().error.contains("on this Mac"));
}

#[test]
fn capabilities_more_than_needed_or_unknown_never_make_a_guest_unusable() {
    let guest = declaring(PROTOCOL, &[STAGE, "teleport-v9"], "0.1.3");

    assert_eq!(judge(&guest, &[STAGE]), Usable::Usable);
}

#[test]
fn the_version_never_decides() {
    let older_with = declaring(PROTOCOL, &[STAGE], "0.0.1");
    let newer_without = declaring(PROTOCOL, &[], "99.0.0");

    assert_eq!(judge(&older_with, &[STAGE]), Usable::Usable);
    assert_ne!(judge(&newer_without, &[STAGE]), Usable::Usable);
}

/// What a station's hello reader makes of the guest's first line `line`.
async fn read_hello(line: serde_json::Value) -> atlas_core::error::Result<control::Hello> {
    let (mut guest, station) = tokio::io::duplex(1 << 16);
    let mut bytes = serde_json::to_vec(&line).unwrap();
    bytes.push(b'\n');
    guest.write_all(&bytes).await.unwrap();
    control::hello(&mut BufReader::new(station)).await
}

#[tokio::test]
async fn a_newer_guest_s_hello_with_a_field_this_mac_does_not_know_is_incompatible_not_malformed() {
    let hello = json!({"hello": {
        "generation": 4, "protocol": PROTOCOL + 1, "capabilities": [], "version": "9.0.0",
        "somethingNew": {"nested": true},
    }});

    let refused = read_hello(hello).await.unwrap_err();

    assert_eq!(refused.code, Code::Update, "{refused:?}");
}

#[tokio::test]
async fn a_hello_of_this_protocol_is_read_strictly() {
    let declared = json!({"hello": {
        "generation": 4, "protocol": PROTOCOL, "capabilities": [STAGE], "version": "0.1.3",
    }});
    let unknown = json!({"hello": {
        "generation": 4, "protocol": PROTOCOL, "capabilities": [], "version": "0.1.3",
        "somethingNew": 1,
    }});

    let hello = read_hello(declared).await.unwrap();
    let refused = read_hello(unknown).await.unwrap_err();

    assert_eq!(hello.declared.capabilities, [STAGE]);
    assert_eq!(refused.code, Code::Protocol);
}

#[tokio::test]
async fn a_hello_declaring_no_protocol_or_no_capabilities_is_malformed() {
    let no_protocol = json!({"hello": {"generation": 4, "capabilities": [], "version": "0.1.3"}});
    let no_capabilities = json!({"hello": {"generation": 4, "protocol": 1, "version": "0.1.3"}});

    assert_eq!(
        read_hello(no_protocol).await.unwrap_err().code,
        Code::Protocol
    );
    assert_eq!(
        read_hello(no_capabilities).await.unwrap_err().code,
        Code::Protocol
    );
}

/// A station meeting a guest whose hello is `hello`: whether it refused
/// it, and everything it said to the guest before the connection closed.
async fn met(
    hello: serde_json::Value,
) -> (
    atlas_core::error::Result<crate::standin::Controlling>,
    Vec<u8>,
) {
    use crate::{
        keys::Keypair,
        secure::{Purpose, connect_as_guest},
    };
    use atlas_core::{desktop::Desktop, user::User};
    use tokio::io::AsyncReadExt;
    let (guest, station) = (Keypair::generate().unwrap(), Keypair::generate().unwrap());
    let named = Desktop::from(User { uid: 502 }).id();
    let (guest_end, station_end) = tokio::io::duplex(1 << 16);
    let guest_side = async {
        let mut secure = connect_as_guest(
            guest_end,
            &guest,
            station.public(),
            &named,
            Purpose::Control,
        )
        .await
        .unwrap();
        let mut line = serde_json::to_vec(&hello).unwrap();
        line.push(b'\n');
        secure.write_all(&line).await.unwrap();
        secure.flush().await.unwrap();
        // Everything the station says until it goes; it must close, not
        // merely fall silent.
        let mut heard = Vec::new();
        let read = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            secure.read_to_end(&mut heard),
        )
        .await;
        let closed = match &read {
            Ok(Ok(_)) => true,
            Ok(Err(error)) => error.kind() == std::io::ErrorKind::UnexpectedEof,
            Err(_) => false,
        };
        assert!(closed, "the station left the connection open: {read:?}");
        heard
    };
    let station_side =
        crate::standin::station_control(station_end, &station, guest.public(), &named);
    let (heard, controlling) = tokio::join!(guest_side, station_side);
    (controlling, heard)
}

#[tokio::test]
async fn a_station_meeting_a_guest_of_another_protocol_says_nothing_and_closes() {
    let older = json!({"hello": {
        "generation": 2, "protocol": PROTOCOL - 1, "capabilities": [], "version": "0.0.9",
    }});
    let newer = json!({"hello": {
        "generation": 2, "protocol": PROTOCOL + 1, "capabilities": ["teleport-v1"],
        "version": "9.0.0", "somethingNew": [1, 2],
    }});

    for hello in [older, newer] {
        let (controlling, heard) = met(hello).await;

        assert_eq!(controlling.unwrap_err().code, Code::Update);
        assert!(heard.is_empty(), "the station said {heard:?}");
    }
}

#[tokio::test]
async fn a_hello_giving_a_field_twice_is_malformed_whatever_the_last_one_says() {
    let (mut guest, station) = tokio::io::duplex(1 << 16);
    let line = format!(
        "{{\"hello\":{{\"generation\":4,\"protocol\":{},\"protocol\":{PROTOCOL},\"capabilities\":[],\"version\":\"0.1.3\"}}}}\n",
        PROTOCOL + 1
    );
    guest.write_all(line.as_bytes()).await.unwrap();

    let refused = control::hello(&mut BufReader::new(station))
        .await
        .unwrap_err();

    assert_eq!(refused.code, Code::Protocol);
}

#[test]
fn only_a_guest_declaring_acknowledgements_acknowledges() {
    let acknowledging = declaring(PROTOCOL, &[CHANGES, ACKNOWLEDGED], "0.1.4");
    let older = declaring(PROTOCOL, &[CHANGES, STOP_REPORT], "0.1.3");
    let other = declaring(PROTOCOL + 1, &[ACKNOWLEDGED], "9.0.0");

    assert!(acknowledging.acknowledges());
    assert!(!older.acknowledges());
    assert!(!other.acknowledges());
}

#[test]
fn a_machine_guest_declares_exactly_these_names_in_this_order() {
    assert_eq!(
        MACHINE,
        [
            "changes-v1",
            "stop-report-v1",
            "acknowledged-v1",
            "machine-proof-v1",
            "leftovers-v1",
            "ended-v1",
            "stage-v1",
            "copies-v1",
            "input-lanes-v1",
            "mirrors-v1",
            "jobs-v1",
            "updates-v1",
            "environment-v1",
            "pictures-v1",
            "installed-v1",
            "native-check-v1",
        ]
    );
}

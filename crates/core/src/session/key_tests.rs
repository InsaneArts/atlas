use super::*;

#[test]
fn a_key_is_written_as_hex_and_read_back() {
    let key = PeerKey::from_bytes([0xab; 32]);

    let written = serde_json::to_value(key).unwrap();

    assert_eq!(written, serde_json::json!("ab".repeat(32)));
    assert_eq!(serde_json::from_value::<PeerKey>(written).unwrap(), key);
}

#[test]
fn anything_but_64_lowercase_hex_digits_is_not_a_key() {
    for text in [
        "AB".repeat(32),
        "ab".repeat(31),
        "ab".repeat(33),
        "zz".repeat(32),
    ] {
        assert!(PeerKey::from_hex(&text).is_none(), "{text}");
        assert!(serde_json::from_value::<PeerKey>(serde_json::json!(text)).is_err());
    }
}

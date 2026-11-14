use super::*;

#[test]
#[ignore = "writes to the account's own Keychain; run by hand on a Mac"]
fn a_record_kept_in_the_keychain_reads_back_and_is_replaced_whole() {
    let vault = Keychain::under(format!("com.atlas.tests.vault.{}", std::process::id()));

    assert_eq!(vault.read("record").unwrap(), None);
    vault.write("record", b"first").unwrap();
    vault.write("record", b"second").unwrap();
    assert_eq!(
        vault.read("record").unwrap().as_deref(),
        Some(&b"second"[..])
    );

    assert!(atlas_native::keychain::delete(vault.item("record")).unwrap());
}

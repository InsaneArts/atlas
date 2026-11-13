use super::*;

#[test]
fn accounts_resolve_through_directory_services() {
    let me = account_with_uid(nix::unistd::getuid().as_raw())
        .unwrap()
        .unwrap();
    assert_eq!(account_named(&me.name).unwrap(), Some(me.clone()));
    assert!(account_named("no-such-atlas-account").unwrap().is_none());
    assert!(account_named("bad\0name").is_err());
}

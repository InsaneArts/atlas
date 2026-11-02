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

#[test]
fn group_lists_name_the_admin_group_exactly() {
    assert_eq!(parse_admin_listed("20 12 80 61\n"), Some(true));

    // An ID merely starting with 80 is not the admin group.
    assert_eq!(parse_admin_listed("20 12 801 61\n"), Some(false));

    assert_eq!(parse_admin_listed("\n"), None);
}

#[test]
fn a_full_name_is_read_from_directory_services_answer_on_one_line_or_the_next() {
    assert_eq!(
        parse_real_name(b"RealName: Atlas\n").as_deref(),
        Some("Atlas")
    );
    assert_eq!(
        parse_real_name(b"RealName:\n Atlas Helper\n").as_deref(),
        Some("Atlas Helper")
    );
    assert_eq!(parse_real_name(b"RealName:\n"), None);
    assert_eq!(parse_real_name(b"No such key: RealName\n"), None);
}

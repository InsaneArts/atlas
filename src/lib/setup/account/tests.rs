use super::*;

#[test]
fn uid_lists_ignore_negative_system_ids_and_keep_nonnegative_ids() {
    let accounts = used_uids(
        "nobody                  -2\ndaemon                   1\n_unknown                99\nme                     501\n",
    )
    .unwrap();
    assert_eq!(accounts, [1, 99, 501]);
    let running = used_uids("   -2\n     1\n    99\n   502\n").unwrap();
    assert_eq!(running, [1, 99, 502]);
    assert_eq!(
        available_uid(&accounts, &running, |_| Ok(false)).unwrap(),
        503
    );
    assert_eq!(
        used_uids("root 0\nhigh 4294967295\n").unwrap(),
        [0, u32::MAX]
    );
}

#[test]
fn uid_lists_refuse_garbage_missing_ids_and_out_of_range_numbers() {
    for line in [
        "garbage",
        "broken NaN",
        "",
        "large 4294967296",
        "huge 9223372036854775808",
    ] {
        let listing = format!("nobody -2\ndaemon 1\n_unknown 99\n{line}\nme 501\n");
        let error = used_uids(&listing).unwrap_err();
        assert_eq!(
            error.to_string(),
            "Could not read a user ID in use",
            "{line}"
        );
    }
}

#[test]
fn uid_allocation_skips_directory_accounts_and_processes_and_refuses_lookup_errors() {
    let uid = available_uid(&[501], &[502], |uid| Ok(uid == 503)).unwrap();
    assert_eq!(uid, 504);
    let error = available_uid(&[], &[], |_| Err(problem("lookup failed")));
    assert!(error.is_err());
}

#[test]
fn new_account_names_are_short_and_plain() {
    assert!(validate_account_name("atlas").is_ok());
    assert!(validate_account_name("qa_helper-2").is_ok());
    assert!(validate_account_name("").is_err());
    assert!(validate_account_name("has space").is_err());
    assert!(validate_account_name("-dash").is_err());
    assert!(
        validate_account_name("503").is_err(),
        "macOS reads digits alone as a user ID"
    );
    assert!(validate_account_name("503a").is_ok());
    assert!(validate_account_name(&"x".repeat(32)).is_err());
}

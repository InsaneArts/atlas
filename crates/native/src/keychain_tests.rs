use super::*;

/// An item no other test or install uses, named for this process.
fn scratch(account: &str) -> (String, String) {
    (
        format!("com.atlas.tests.keychain.{}", std::process::id()),
        account.to_owned(),
    )
}

#[test]
fn a_query_names_a_generic_password_in_the_file_keychain() {
    let item = Item {
        service: "service",
        account: "account",
    };
    let query = query(item, &[]);
    // SAFETY: the keys are the framework's own constants.
    let (class, service, protected) =
        unsafe { (kSecClass, kSecAttrService, kSecUseDataProtectionKeychain) };
    assert_eq!(query.len(), 4);
    let class = query.get(class).unwrap().downcast::<CFString>().unwrap();
    assert_eq!(class.to_string(), "genp");
    let named = query.get(service).unwrap().downcast::<CFString>().unwrap();
    assert_eq!(named.to_string(), "service");
    let protected = query
        .get(protected)
        .unwrap()
        .downcast::<CFBoolean>()
        .unwrap();
    assert!(!protected.as_bool());
}

#[test]
fn a_refusal_names_what_was_refused_and_its_status() {
    let error = checked(-25293, "read").unwrap_err();
    assert_eq!(error.code, Code::Identity);
    assert!(error.error.contains("read"));
    assert!(error.error.contains("-25293"));
}

#[test]
#[ignore = "writes to the account's own Keychain; run by hand on a Mac"]
fn a_secret_written_reads_back_replaces_and_deletes() {
    let (service, account) = scratch("round-trip");
    let item = Item {
        service: &service,
        account: &account,
    };
    assert_eq!(read(item).unwrap(), None);
    write(item, b"first").unwrap();
    assert_eq!(read(item).unwrap().as_deref(), Some(&b"first"[..]));
    write(item, b"second").unwrap();
    assert_eq!(read(item).unwrap().as_deref(), Some(&b"second"[..]));
    assert!(delete(item).unwrap());
    assert!(!delete(item).unwrap());
    assert_eq!(read(item).unwrap(), None);
}

#[test]
#[ignore = "writes to the account's own Keychain; run by hand on a Mac"]
fn an_item_another_program_made_is_refused_without_asking() {
    let (service, account) = scratch("planted");
    let planted = atlas_core::spawn::status(
        std::process::Command::new("/usr/bin/security")
            .args(["add-generic-password", "-A", "-s", &service, "-a", &account])
            .args(["-w", "planted-planted-planted-planted!"]),
    )
    .unwrap();
    assert!(planted.success());
    let item = Item {
        service: &service,
        account: &account,
    };

    let read = read(item);
    let written = write(item, b"atlas");

    let removed =
        atlas_core::spawn::status(std::process::Command::new("/usr/bin/security").args([
            "delete-generic-password",
            "-s",
            &service,
            "-a",
            &account,
        ]))
        .unwrap();
    assert!(removed.success());
    assert_eq!(read.unwrap_err().code, Code::Identity);
    assert_eq!(written.unwrap_err().code, Code::Identity);
}

#[test]
#[ignore = "writes to the account's own Keychain; run by hand on a Mac"]
fn an_item_another_program_made_for_this_one_alone_is_refused() {
    let (service, account) = scratch("planted-for-us");
    let this = std::env::current_exe().unwrap();
    let planted = atlas_core::spawn::status(
        std::process::Command::new("/usr/bin/security")
            .args(["add-generic-password", "-s", &service, "-a", &account])
            .arg("-T")
            .arg(&this)
            .args(["-w", "planted-planted-planted-planted!"]),
    )
    .unwrap();
    assert!(planted.success());
    let item = Item {
        service: &service,
        account: &account,
    };

    let read = read(item);

    let removed =
        atlas_core::spawn::status(std::process::Command::new("/usr/bin/security").args([
            "delete-generic-password",
            "-s",
            &service,
            "-a",
            &account,
        ]))
        .unwrap();
    assert!(removed.success());
    assert_eq!(read.unwrap_err().code, Code::Identity);
}

#[test]
#[ignore = "writes to the account's own Keychain; run by hand on a Mac"]
fn a_secret_added_once_is_never_replaced_by_another_add() {
    let (service, account) = scratch("add-once");
    let item = Item {
        service: &service,
        account: &account,
    };

    assert!(add(item, b"first").unwrap());
    assert!(!add(item, b"second").unwrap());

    assert_eq!(read(item).unwrap().as_deref(), Some(&b"first"[..]));
    assert!(delete(item).unwrap());
}

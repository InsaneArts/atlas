use super::*;

fn opie() -> Identity {
    Identity {
        uid: 502,
        name: "opie".into(),
        generated_uid: "6DB4B7AF-BE3C-486C-BD46-1B4558946919".into(),
    }
}

#[test]
fn only_a_generated_uid_of_macos_form_is_read() {
    assert_eq!(
        parse_generated_uid("GeneratedUID: 6DB4B7AF-BE3C-486C-BD46-1B4558946919\n"),
        Some("6DB4B7AF-BE3C-486C-BD46-1B4558946919".into())
    );
    for text in [
        "",
        "GeneratedUID:",
        "GeneratedUID: 6DB4B7AF-BE3C-486C-BD46",
        "GeneratedUID: 6DB4B7AF-BE3C-486C-BD46-1B455894691Z",
        "GeneratedUID: 6DB4B7AF BE3C 486C BD46 1B4558946919",
        "RealName: Opie",
    ] {
        assert_eq!(parse_generated_uid(text), None, "{text}");
    }
}

#[test]
fn a_recorded_identity_is_read_back_only_for_its_own_user_id() {
    let home = tempfile::tempdir().unwrap();
    assert_eq!(on_file_in(home.path(), 502), OnFile::Nothing);

    remember_in(home.path(), &opie(), false).unwrap();
    assert_eq!(on_file_in(home.path(), 502), OnFile::Recorded(opie()));
    assert_eq!(on_file_in(home.path(), 503), OnFile::Nothing);

    fs::write(path_in(home.path(), 504), br#"{"uid": 502}"#).unwrap();
    assert_eq!(on_file_in(home.path(), 504), OnFile::Unreadable);
}

fn reused() -> Identity {
    Identity {
        generated_uid: "11111111-2222-3333-4444-555555555555".into(),
        ..opie()
    }
}

#[test]
fn explicitly_choosing_an_unrecorded_helper_records_its_identity() {
    let home = tempfile::tempdir().unwrap();
    assert_eq!(on_file_in(home.path(), 502), OnFile::Nothing);
    remember_in(home.path(), &opie(), true).unwrap();
    assert_eq!(
        standing(&on_file_in(home.path(), 502), Some(&opie())),
        Standing::Same(opie())
    );
}

#[test]
fn an_account_that_took_the_helpers_user_id_is_not_the_helper() {
    let recorded = OnFile::Recorded(opie());
    assert_eq!(standing(&recorded, Some(&opie())), Standing::Same(opie()));
    assert_eq!(
        standing(&recorded, Some(&reused())),
        Standing::Replaced,
        "same name, new account"
    );
    let renamed = Identity {
        name: "ann".into(),
        ..opie()
    };
    assert_eq!(standing(&recorded, Some(&renamed)), Standing::Replaced);
    assert_eq!(standing(&recorded, None), Standing::Unknown);
    assert_eq!(
        standing(&OnFile::Unreadable, Some(&opie())),
        Standing::Unknown
    );
    assert_eq!(
        standing(&OnFile::Nothing, Some(&opie())),
        Standing::Unrecorded(opie())
    );
}

#[test]
fn a_record_of_another_account_is_replaced_only_by_a_new_choice() {
    let home = tempfile::tempdir().unwrap();
    remember_in(home.path(), &opie(), false).unwrap();

    // The user ID went to another account: repairing never records it.
    assert!(remember_in(home.path(), &reused(), false).is_err());
    assert_eq!(on_file_in(home.path(), 502), OnFile::Recorded(opie()));
    remember_in(home.path(), &opie(), false).unwrap();

    // An unreadable record is never overwritten by a repair either.
    fs::write(path_in(home.path(), 502), b"{").unwrap();
    assert!(remember_in(home.path(), &opie(), false).is_err());

    // The person chose it as the new helper.
    remember_in(home.path(), &reused(), true).unwrap();
    assert_eq!(on_file_in(home.path(), 502), OnFile::Recorded(reused()));
}

#[test]
fn this_accounts_own_identity_is_read_from_its_directory() {
    let me = nix::unistd::getuid().as_raw();
    let account = super::super::account_with_uid(me).unwrap().unwrap();
    let identity = current(&account).unwrap();
    assert_eq!(identity.uid, me);
    assert_eq!(identity.name, account.name);
}

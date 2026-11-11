use super::*;

#[test]
fn an_account_named_with_its_user_id_keeps_both() {
    assert_eq!(
        account("atlas".to_owned(), 503),
        Some(ConsoleUser {
            name: "atlas".to_owned(),
            uid: 503,
        })
    );
    assert_eq!(
        account("loginwindow".to_owned(), 0),
        Some(ConsoleUser {
            name: "loginwindow".to_owned(),
            uid: 0,
        })
    );
}

#[test]
fn an_account_named_without_its_user_id_is_no_account() {
    assert_eq!(account("atlas".to_owned(), NO_UID), None);
}

#[test]
#[ignore = "needs a login on this Mac and a shell that may read the system's dynamic store"]
fn macos_names_an_account_while_someone_is_logged_in() {
    let user = named().expect("macOS names an account");

    assert!(!user.name.is_empty());
    assert_ne!(user.uid, NO_UID);
}

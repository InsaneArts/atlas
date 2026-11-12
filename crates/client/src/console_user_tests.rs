use super::*;

const PERSON: u32 = 501;
const HELPER: u32 = 503;

fn account(name: &str, uid: u32) -> Option<ConsoleUser> {
    Some(ConsoleUser {
        name: name.to_owned(),
        uid,
    })
}

fn login(uid: u32, on_screen: bool) -> Option<Login> {
    Some(Login { uid, on_screen })
}

#[test]
fn the_login_on_the_screen_is_rightly_named_when_macos_names_its_account() {
    assert_eq!(
        Named::of(login(PERSON, true), account("person", PERSON), PERSON),
        Named::This
    );
}

#[test]
fn another_account_named_while_this_login_is_on_the_screen_is_told_by_name() {
    assert_eq!(
        Named::of(login(PERSON, true), account("atlas", HELPER), PERSON),
        Named::Another {
            name: "atlas".to_owned()
        }
    );
}

#[test]
fn the_login_window_named_while_this_login_is_on_the_screen_is_another_account() {
    assert_eq!(
        Named::of(login(PERSON, true), account("loginwindow", 0), PERSON),
        Named::Another {
            name: "loginwindow".to_owned()
        }
    );
}

#[test]
fn a_login_off_the_screen_says_nothing_of_the_account_named() {
    for named in [
        account("atlas", HELPER),
        account("person", PERSON),
        account("loginwindow", 0),
        None,
    ] {
        assert_eq!(
            Named::of(login(PERSON, false), named, PERSON),
            Named::Unknown
        );
    }
}

#[test]
fn a_shell_switched_to_another_account_learns_nothing_from_the_login_it_runs_in() {
    for on_screen in [true, false] {
        assert_eq!(
            Named::of(login(PERSON, on_screen), account("person", PERSON), HELPER),
            Named::Unknown
        );
    }
}

#[test]
fn what_could_not_be_read_is_unknown_and_never_another_account() {
    assert_eq!(Named::of(login(PERSON, true), None, PERSON), Named::Unknown);
    assert_eq!(
        Named::of(None, account("atlas", HELPER), PERSON),
        Named::Unknown
    );
    assert_eq!(Named::of(None, None, PERSON), Named::Unknown);
}

#[test]
fn the_named_account_serializes_as_a_tagged_state() {
    for (named, expected) in [
        (Named::This, serde_json::json!({"state": "this"})),
        (
            Named::Another {
                name: "atlas".to_owned(),
            },
            serde_json::json!({"state": "another", "name": "atlas"}),
        ),
        (Named::Unknown, serde_json::json!({"state": "unknown"})),
    ] {
        assert_eq!(serde_json::to_value(named).unwrap(), expected);
    }
}

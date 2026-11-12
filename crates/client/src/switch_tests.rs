use super::*;
use atlas_core::user::logins::Blocker;

fn console_proof() -> Value {
    json!({"uid": 503, "hasGraphicAccess": true, "onConsole": true})
}

#[test]
fn an_off_console_or_unknown_session_cannot_lock_or_switch_accounts() {
    for state in [json!(false), Value::Null, json!("true")] {
        let mut proof = console_proof();
        proof["onConsole"] = state;
        let error = lock_screen_using(
            503,
            || Ok(proof),
            || panic!("an off-console session must never lock"),
            || panic!("a refused lock must never be awaited"),
        )
        .unwrap_err();
        assert_eq!(error.code, Code::NotReady);
        assert_eq!(
            error.error,
            "Atlas can't switch yet: this login must be on screen at the Mac to switch accounts or lock the screen."
        );
    }
}

#[test]
fn this_login_is_on_screen_only_when_its_own_graphical_session_says_so() {
    assert!(on_screen(503, || Ok(console_proof())).unwrap());
    for state in [json!(false), Value::Null, json!("true")] {
        let mut proof = console_proof();
        proof["onConsole"] = state;
        assert!(
            !on_screen(503, || Ok(proof)).unwrap(),
            "a login at the login window or behind another is not on screen"
        );
    }
    let mut other = console_proof();
    other["uid"] = json!(501);
    assert_eq!(
        on_screen(503, || Ok(other)).unwrap_err().code,
        Code::Identity,
        "another login's session proves nothing about this one"
    );
}

#[test]
fn a_lock_requires_this_accounts_verified_graphical_session() {
    for (key, value) in [
        ("uid", json!(501)),
        ("uid", Value::Null),
        ("hasGraphicAccess", json!(false)),
        ("hasGraphicAccess", Value::Null),
    ] {
        let mut proof = console_proof();
        proof[key] = value;
        let error = lock_screen_using(
            503,
            || Ok(proof),
            || panic!("an unverified session must never lock"),
            || panic!("a refused lock must never be awaited"),
        )
        .unwrap_err();
        assert_eq!(error.code, Code::Identity);
    }
    let error = lock_screen_using(
        503,
        || {
            Err(Error::new(
                Code::Identity,
                "No graphical session dictionary",
            ))
        },
        || panic!("a failed session probe must never lock"),
        || panic!("a refused lock must never be awaited"),
    )
    .unwrap_err();
    assert_eq!(error.error, "No graphical session dictionary");
}

#[test]
fn an_on_console_session_is_checked_before_locking_and_success_requires_confirmation() {
    use std::cell::RefCell;

    for confirmed in [false, true] {
        let calls = RefCell::new(Vec::new());
        let result = lock_screen_using(
            503,
            || {
                calls.borrow_mut().push("session");
                Ok(console_proof())
            },
            || calls.borrow_mut().push("lock"),
            || {
                calls.borrow_mut().push("confirm");
                confirmed
            },
        );
        assert_eq!(*calls.borrow(), ["session", "lock", "confirm"]);
        if confirmed {
            assert_eq!(result.unwrap(), json!({"ok": true, "locked": true}));
        } else {
            assert_eq!(result.unwrap_err().code, Code::Switch);
        }
    }
}

#[test]
fn only_ordinary_logins_are_offered() {
    let listing =
        "_spotlight 89\nroot 0\ndaemon 1\n_unknown 99\natlastest 502\ntheiskaa 501\nnobody -2\n";
    assert_eq!(
        parse_accounts(listing).unwrap(),
        vec![("atlastest".into(), 502), ("theiskaa".into(), 501)]
    );
}

#[test]
fn account_lists_refuse_garbage_instead_of_omitting_an_account() {
    for line in ["garbage", "broken NaN", "large 4294967296"] {
        let listing = format!("nobody -2\ndaemon 1\n_unknown 99\n{line}\natlas 502\n");
        assert!(
            listed(&listing, 501, Some(502), |_| None).is_err(),
            "{line}"
        );
    }
}

#[test]
fn the_helper_is_listed_first_and_this_login_never() {
    let listing = "zed 504\nme 501\natlas 503\nann 502\n_hidden 600\n";
    let named = |name: &str| (name == "atlas").then(|| "Atlas".to_owned());
    let accounts = listed(listing, 501, Some(503), named).unwrap();
    let order: Vec<(&str, bool)> = accounts
        .iter()
        .map(|account| (account.name.as_str(), account.helper))
        .collect();

    assert_eq!(
        order,
        [("atlas", true), ("ann", false), ("zed", false)],
        "the helper first, then by user ID, never this login"
    );
    assert_eq!(accounts[0].real_name.as_deref(), Some("Atlas"));

    let without = listed(listing, 501, None, |_| None).unwrap();
    assert!(without.iter().all(|account| !account.helper));
    assert_eq!(without[0].uid, 502);
}

#[test]
fn this_mac_lists_only_other_ordinary_logins() {
    let accounts = accounts(Some(u32::MAX)).unwrap();
    let me = nix::unistd::getuid().as_raw();
    assert!(accounts.iter().all(|account| account.uid != me));
    assert!(
        accounts
            .iter()
            .all(|account| account.uid >= atlas_core::account::FIRST_USER_UID)
    );
}

fn helper() -> Target {
    Target {
        uid: 503,
        helper: true,
        back: false,
        logged_in: false,
        exists: true,
        home_owned: true,
        administrator: Some(false),
        on_console: false,
    }
}

/// The person's own desktop, switched back to from the helper.
fn back() -> Target {
    Target {
        uid: 501,
        helper: false,
        back: true,
        logged_in: true,
        administrator: Some(true),
        ..helper()
    }
}

fn refusal(target: &Target, blockers: Option<&[Blocker]>) -> String {
    let error = ready_to_switch(target, blockers).unwrap_err();
    assert_eq!(error.code, "not-ready");
    error.error
}

#[test]
fn a_new_login_waits_while_another_could_hang_it() {
    assert!(ready_to_switch(&helper(), Some(&[])).is_ok());

    let cases = [
        (
            Blocker::Orphan { uid: 502 },
            "Atlas can't switch yet: a deleted account is still logged in.",
        ),
        (
            Blocker::HelperLeftover { uid: 503 },
            "Atlas can't switch yet: a deleted account is still logged in.",
        ),
        (
            Blocker::StuckLogin { uid: 504 },
            "Atlas can't switch yet: another account is still logging in.",
        ),
        (
            Blocker::AccountUpdating,
            "Atlas can't switch yet: macOS is still setting up an account.",
        ),
    ];
    for (blocker, words) in cases {
        assert_eq!(refusal(&helper(), Some(&[blocker])), words);
    }

    assert_eq!(
        refusal(&helper(), None),
        "Atlas can't switch yet: macOS didn't list its logins. Try again."
    );
}

#[test]
fn switching_back_to_the_persons_own_running_desktop_passes_what_holds_a_new_login() {
    for blocker in [
        Blocker::Orphan { uid: 502 },
        Blocker::StuckLogin { uid: 504 },
        Blocker::AccountUpdating,
    ] {
        assert!(
            ready_to_switch(&back(), Some(&[blocker])).is_ok(),
            "{blocker:?}"
        );
    }
}

#[test]
fn a_helpers_running_dock_never_lets_a_switch_pass_a_leftover() {
    let leftover = [Blocker::HelperLeftover { uid: 503 }];
    let running = Target {
        logged_in: true,
        ..helper()
    };
    assert_eq!(
        refusal(&running, Some(&leftover)),
        "Atlas can't switch yet: a deleted account is still logged in."
    );
    assert_eq!(
        refusal(&running, Some(&[Blocker::Orphan { uid: 502 }])),
        "Atlas can't switch yet: a deleted account is still logged in.",
        "the helper is never switched back to"
    );
    assert_eq!(
        refusal(&back(), Some(&leftover)),
        "Atlas can't switch yet: a deleted account is still logged in.",
        "not even when switching back"
    );
}

#[test]
fn only_the_persons_own_verified_desktop_counts_as_switching_back() {
    let orphan = [Blocker::Orphan { uid: 502 }];
    let cases = [
        Target {
            back: false,
            ..back()
        },
        Target {
            logged_in: false,
            ..back()
        },
        Target {
            exists: false,
            ..back()
        },
        Target {
            helper: true,
            ..back()
        },
    ];
    for target in cases {
        assert!(
            ready_to_switch(&target, Some(&orphan)).is_err(),
            "{target:?}"
        );
    }

    // Its own login stuck, or unknown logins: it waits too.
    assert!(ready_to_switch(&back(), Some(&[Blocker::StuckLogin { uid: 501 }])).is_err());
    assert!(ready_to_switch(&back(), None).is_err());
}

#[test]
fn a_helper_job_names_the_person_it_works_for() {
    use atlas_daemon::user::setup::{Agent, plist};

    let agent = Agent {
        label: "com.insanearts.atlas.helper".into(),
        program: "/Applications/Atlas.app/Contents/MacOS/atlas".into(),
        uid: 503,
        controller_uid: 501,
        socket: "/Users/opie/s.sock".into(),
    };
    assert_eq!(
        controller_in(&plist(&agent, std::path::Path::new("/tmp"))),
        Some(501)
    );
    assert_eq!(controller_in("<plist></plist>"), None);
}

#[test]
fn the_helper_must_be_a_standard_account_with_its_own_home_off_screen() {
    let cases = [
        (
            Target {
                exists: false,
                ..helper()
            },
            "Atlas can't switch yet: the helper account no longer exists.",
        ),
        (
            Target {
                home_owned: false,
                ..helper()
            },
            "Atlas can't switch yet: the helper's home folder is missing or isn't its own.",
        ),
        (
            Target {
                administrator: None,
                ..helper()
            },
            "Atlas can't switch yet: the helper must be a standard account, not an administrator.",
        ),
        (
            Target {
                on_console: true,
                ..helper()
            },
            "Atlas can't switch yet: the helper is already on screen.",
        ),
    ];
    for (target, words) in cases {
        assert_eq!(refusal(&target, Some(&[])), words);
    }
}

#[test]
fn the_lock_calls_exist_on_this_macos() {
    // Resolved without calling: calling would change the person's screen.
    assert!(login_symbol(c"SACLockScreenImmediate").is_ok());
    assert!(login_symbol(c"SACSwitchToLoginWindow").is_ok());
    assert_eq!(login_symbol(c"SACNoSuchCall").unwrap_err().code, "switch");
}

#[tokio::test(start_paused = true)]
async fn the_person_is_back_only_on_two_looks_in_a_row_a_settle_apart() {
    let looks = std::cell::RefCell::new(vec![true, false, true, true].into_iter());
    let started = tokio::time::Instant::now();
    until_back_using(|| Ok(looks.borrow_mut().next().unwrap()), SETTLE * 10)
        .await
        .unwrap();
    assert_eq!(
        looks.borrow_mut().next(),
        None,
        "a look off screen starts the count again"
    );
    assert_eq!(started.elapsed(), SETTLE * 3);
}

#[tokio::test(start_paused = true)]
async fn a_session_that_cannot_be_verified_fails_at_once_without_waiting() {
    let started = tokio::time::Instant::now();
    let error = until_back_using(
        || {
            Err(Error::new(
                Code::Identity,
                "No graphical session dictionary",
            ))
        },
        SETTLE * 10,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, Code::Identity);
    assert_eq!(started.elapsed(), std::time::Duration::ZERO);
}

#[tokio::test(start_paused = true)]
async fn a_person_who_stays_away_is_refused_once_the_limit_passes() {
    let started = tokio::time::Instant::now();
    let error = until_back_using(|| Ok(false), SETTLE * 4)
        .await
        .unwrap_err();
    assert_eq!(error.code, Code::NotReady);
    assert_eq!(started.elapsed(), SETTLE * 4);
}

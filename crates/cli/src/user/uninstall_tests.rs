use super::{log_out::*, *};
use atlas_core::error::{Code, Error};
use std::fs;

#[test]
fn interrupted_retirement_keeps_every_unacknowledged_mutation_pending() {
    if !atlas_core::test_support::isolated(
        "user::uninstall::tests::interrupted_retirement_keeps_every_unacknowledged_mutation_pending",
    ) {
        return;
    }
    let requests = [
        atlas_daemon::user::setup::Request::ForgetOpener { uid: 7001 },
        atlas_daemon::user::setup::Request::Retire {
            uid: 7001,
            label: atlas_daemon::user::setup::LABEL.into(),
            sha256: "a".repeat(64),
            missing_ok: true,
        },
    ];
    for request in requests {
        let dir = tempfile::tempdir().unwrap();
        let controller = Controller::at(7001, &dir.path().join("state"));
        let held = controller.acquire(true).unwrap();
        let claimed_before_dispatch = std::cell::Cell::new(false);
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            retirement_request(&controller, &request, || {
                claimed_before_dispatch.set(controller.pending().unwrap().len() == 1);
                fs::write(dir.path().join("changed"), b"retired").unwrap();
                panic!("the worker changed a file but its caller received no acknowledgement");
            })
        }));
        assert!(outcome.is_err());
        assert!(dir.path().join("changed").exists());
        assert_eq!(controller.unresolved().unwrap().len(), 1);
        assert!(claimed_before_dispatch.get());
        drop(held);
        assert!(controller.acquire(false).is_err());
        let held = controller.acquire(true).unwrap();
        controller.clear().unwrap();
        retirement_request(&controller, &request, || Ok(json!({"ok": true}))).unwrap();
        assert!(
            controller.pending().unwrap().is_empty(),
            "acknowledged retirement settles its own claim"
        );
        drop(held);
        assert!(controller.acquire(false).is_ok());
    }
}

#[test]
fn root_ends_the_login_only_after_the_helpers_own_atlas_could_not() {
    assert_eq!(after(Way::Ask, Came::Gone, false), Next::Done);
    assert_eq!(after(Way::Ask, Came::Stayed, false), Next::Try(Way::Root));
    assert_eq!(after(Way::Ask, Came::Held, true), Next::Try(Way::Force));
    assert_eq!(after(Way::Ask, Came::Held, false), Next::AskToForce);
    assert_eq!(after(Way::Force, Came::Gone, false), Next::Done);
    assert_eq!(after(Way::Force, Came::Stayed, false), Next::Try(Way::Root));
    assert_eq!(after(Way::Force, Came::Held, true), Next::Try(Way::Root));
    assert_eq!(after(Way::Root, Came::Gone, false), Next::Done);

    // Every way failed: stop, never go on to remove anything.
    assert_eq!(after(Way::Root, Came::Stayed, true), Next::Stop);
    assert_eq!(after(Way::Root, Came::Held, true), Next::Stop);
}

#[test]
fn the_helper_is_the_account_recorded_only_while_it_still_is() {
    let opie = Identity {
        uid: 502,
        name: "opie".into(),
        generated_uid: "6DB4B7AF-BE3C-486C-BD46-1B4558946919".into(),
    };
    let reused = Identity {
        generated_uid: "11111111-2222-3333-4444-555555555555".into(),
        ..opie.clone()
    };
    let same = Standing::Same(opie.clone());
    assert_eq!(retiring(Some(&same)), Some(&opie));
    assert_eq!(retiring(Some(&Standing::Replaced)), None);
    assert_eq!(retiring(Some(&Standing::Unknown)), None, "unreadable");
    assert_eq!(retiring(Some(&Standing::Unrecorded(reused))), None);
}

#[test]
fn only_the_recorded_helpers_job_is_retired() {
    let opie = Identity {
        uid: 502,
        name: "opie".into(),
        generated_uid: "6DB4B7AF-BE3C-486C-BD46-1B4558946919".into(),
    };
    let verified = Standing::Same(opie.clone());
    assert_eq!(retiring(Some(&verified)), Some(&opie));
    assert_eq!(retiring(Some(&Standing::Replaced)), None, "left alone");
    assert_eq!(retiring(None), None, "gone");
    assert_eq!(
        retiring(Some(&Standing::Unrecorded(opie))),
        None,
        "never taken for the helper unrecorded"
    );
}

#[test]
fn only_the_strict_look_says_the_helper_is_gone() {
    assert_eq!(strictly(Came::Gone, false), Came::Stayed, "root said so");
    assert_eq!(strictly(Came::Stayed, true), Came::Gone);
    assert_eq!(strictly(Came::Held, false), Came::Held);
    assert_eq!(strictly(Came::Gone, true), Came::Gone);
}

#[test]
fn uninstall_says_the_home_folder_stays_private_and_how_to_open_it() {
    assert!(home_note("atlas", Ok(None), false).is_empty());
    let kept = home_note("atlas", Ok(Some(0)), false);
    assert_eq!(kept.len(), 2);
    assert!(kept[1].ends_with("chmod -a \"user:atlas deny list,search\" ~"));
    let deleted = home_note("atlas", Ok(Some(1)), true);
    assert!(deleted[2].contains("chmod -a# 1 ~"));
    let unread = home_note("atlas", Err(Error::new(Code::Io, "timed out")), true);
    assert_eq!(unread.len(), 1);
    assert!(unread[0].ends_with("chmod -a \"user:atlas deny list,search\" ~"));
}

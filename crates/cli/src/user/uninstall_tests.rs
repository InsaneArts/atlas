use super::*;
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

use super::*;
use atlas_daemon::identity::Identity;

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

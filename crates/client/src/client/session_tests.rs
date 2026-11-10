use super::*;
use crate::test_support::client_in;
use atlas_control::Controller;

#[test]
fn a_successful_recovery_allows_the_next_normal_acquisition() {
    let dir = tempfile::tempdir().unwrap();
    let mut client = client_in(dir.path());
    client.controller = Controller::at(60001, &dir.path().join("state"));
    let ownership = client.controller.acquire(true).unwrap();
    drop(client.controller.quarantine().unwrap());
    let claim = client.controller.quarantine().unwrap();
    // The verified recovery clears abandoned work while retaining its
    // own live claim, which the restart success path must then settle.
    client.controller.clear().unwrap();
    client
        .settle_restart(&claim, Ok(json!({"ok": true, "restarted": true})))
        .unwrap();
    drop(claim);
    drop(ownership);
    assert!(client.controller.pending().unwrap().is_empty());
    assert!(client.controller.acquire(false).is_ok());

    let other = client.controller.quarantine().unwrap();
    let claim = client.controller.quarantine().unwrap();
    client.controller.clear().unwrap();
    client
        .settle_restart(&claim, Ok(json!({"ok": true, "restarted": true})))
        .unwrap();
    drop(claim);
    assert_eq!(client.controller.pending().unwrap(), [other.generation()]);
    assert!(client.controller.acquire(false).is_err());
}

/// A login is gone only once it is seen gone twice in a row and its
/// daemon stopped answering; a look in between that sees it again
/// starts the count over.
#[tokio::test(start_paused = true)]
async fn a_login_is_gone_only_when_seen_gone_twice_and_silent() {
    use std::{cell::RefCell, collections::VecDeque};

    let pause = Duration::from_millis(1);
    let limit = Duration::from_secs(5);
    let run = |looks: &[bool], hellos: &[bool]| {
        let looks = RefCell::new(looks.iter().copied().collect::<VecDeque<_>>());
        let hellos = RefCell::new(hellos.iter().copied().collect::<VecDeque<_>>());
        let seen = RefCell::new(0);
        async move {
            let gone = gone_within(
                || {
                    *seen.borrow_mut() += 1;
                    let look = looks.borrow_mut().pop_front().unwrap_or(false);
                    async move { look }
                },
                || {
                    let hello = hellos.borrow_mut().pop_front().unwrap_or(false);
                    async move { hello }
                },
                limit,
                pause,
            )
            .await;
            (gone, seen.into_inner())
        }
    };

    assert_eq!(run(&[true, true, true], &[]).await, (true, 3));
    assert_eq!(
        run(&[true, false, true, true, true], &[]).await,
        (true, 5),
        "a login seen again starts the count over"
    );
    assert_eq!(
        run(&[true, true, true, true, true], &[true, true]).await,
        (true, 5),
        "waits for the daemon to go quiet, looking all the while"
    );
    assert_eq!(
        run(&[true, true, false, true, true, true], &[true, false]).await,
        (true, 6),
        "a login back after the second look starts the count over"
    );
    assert_eq!(
        run(&[true, true, false, true, true, true], &[false, false]).await,
        (true, 6),
        "a login back while the daemon was asked starts the count over"
    );

    // Back after the second look, then its daemon goes quiet: still
    // logged in, never gone.
    let back = Duration::from_millis(50);
    let looks = std::cell::Cell::new(0);
    let hellos = std::cell::Cell::new(0);
    let returned = gone_within(
        || {
            looks.set(looks.get() + 1);
            let look = looks.get() <= 2;
            async move { look }
        },
        || {
            hellos.set(hellos.get() + 1);
            let hello = hellos.get() == 1;
            async move { hello }
        },
        back,
        pause,
    )
    .await;
    assert!(!returned, "a login that came back is not gone");

    // It logs in again while the daemon is still being asked, and the
    // daemon then says nothing: still logged in.
    let again = std::cell::Cell::new(false);
    let returned = gone_within(
        || {
            let look = !again.get();
            async move { look }
        },
        || {
            again.set(true);
            async move {
                tokio::time::sleep(pause).await;
                false
            }
        },
        back,
        pause,
    )
    .await;
    assert!(!returned, "back while its daemon was asked");

    let short = Duration::from_millis(50);
    let stays = gone_within(|| async { false }, || async { false }, short, pause).await;
    assert!(!stays, "a login that stays is not gone");
    let answering = gone_within(|| async { true }, || async { true }, short, pause).await;
    assert!(!answering, "a daemon that keeps answering is not gone");
}

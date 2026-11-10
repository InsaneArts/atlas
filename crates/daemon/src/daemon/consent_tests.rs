use super::*;

#[tokio::test]
async fn a_recording_grant_is_picked_up_after_the_restart_cooldown_expires() {
    let elapsed = std::cell::Cell::new(Duration::ZERO);
    let restarted = std::cell::Cell::new(false);
    watch_recording(
        || {
            elapsed.set(elapsed.get() + RECORDING_WATCH);
            assert!(elapsed.get() <= RECORDING_RESTART_GAP);
            std::future::ready(())
        },
        || false,
        || std::future::ready(true),
        || within(Some(elapsed.get()), RECORDING_RESTART_GAP),
        || restarted.set(true),
    )
    .await;
    assert!(restarted.get());
    assert_eq!(elapsed.get(), RECORDING_RESTART_GAP);
}

/// The prompt option is passed by name; it is the exported constant.
#[test]
#[allow(
    unsafe_code,
    reason = "FFI: the framework's constants are extern statics"
)]
fn the_prompt_option_is_the_framework_constant() {
    // SAFETY: reading an immutable framework constant in a test.
    let key = unsafe { objc2_application_services::kAXTrustedCheckOptionPrompt };
    assert_eq!(key.to_string(), "AXTrustedCheckOptionPrompt");
}

/// `asking`'s nudges over `count` passes, the window guiding as given.
fn passes(asking: &mut Asking, guided: bool, settings_after: u32, count: usize) -> Vec<Nudge> {
    (0..count)
        .map(|_| asking.pass(guided, settings_after))
        .collect()
}

#[test]
fn a_restart_for_screen_recording_waits_a_minute_after_the_last_one() {
    let gap = RECORDING_RESTART_GAP;
    assert!(!within(None, gap), "never restarted");
    assert!(within(Some(Duration::from_secs(5)), gap), "just restarted");
    assert!(!within(Some(gap + Duration::from_secs(1)), gap));
}

#[test]
fn while_the_setup_window_guides_the_daemon_only_watches() {
    let mut asking = Asking::default();
    assert!(
        passes(&mut asking, true, 15, 30)
            .iter()
            .all(|nudge| *nudge == Nudge::Wait)
    );
    assert!(matches!(asking.request, ConsentRequest::Unasked));
}

#[test]
fn without_the_window_the_daemon_waits_asks_once_then_opens_the_page_once() {
    let mut asking = Asking::default();
    let quiet = QUIET_PASSES as usize;
    let mut expected = vec![Nudge::Wait; quiet - 1];
    expected.push(Nudge::Request);
    expected.extend(vec![Nudge::Wait; 14]);
    expected.push(Nudge::OpenSettings);
    expected.push(Nudge::Wait);
    assert_eq!(passes(&mut asking, false, 15, expected.len()), expected);

    let mut recording = Asking::default();
    let nudges = passes(&mut recording, false, 1, quiet + 2);
    assert_eq!(
        nudges[quiet - 1..],
        [Nudge::Request, Nudge::OpenSettings, Nudge::Wait]
    );
}

#[test]
fn a_window_opening_late_at_login_still_guides_before_any_prompt() {
    let mut asking = Asking::default();
    let early = passes(&mut asking, false, 15, QUIET_PASSES as usize - 1);
    assert!(early.iter().all(|nudge| *nudge == Nudge::Wait));
    assert_eq!(
        asking.pass(true, 15),
        Nudge::Wait,
        "the window came in time"
    );
    let again = passes(&mut asking, false, 15, QUIET_PASSES as usize);
    assert_eq!(
        again.last(),
        Some(&Nudge::Request),
        "closing it brings the request back"
    );
}

#[test]
fn closing_the_window_midway_brings_the_page_back_once_its_time_went_by() {
    let mut asking = Asking::default();
    passes(&mut asking, false, 15, QUIET_PASSES as usize);
    assert!(matches!(asking.request, ConsentRequest::Requested(_)));
    assert!(
        passes(&mut asking, true, 15, 20)
            .iter()
            .all(|nudge| *nudge == Nudge::Wait)
    );
    let after = passes(&mut asking, false, 15, QUIET_PASSES as usize);
    assert_eq!(after.last(), Some(&Nudge::OpenSettings));
}

#[test]
fn the_set_up_notice_names_the_account_to_switch_back_to() {
    assert_eq!(
        done_message(Some("Alex")),
        "Screen Recording and Accessibility are on. Switch back to Alex — Atlas keeps running there."
    );
    assert_eq!(
        done_message(None),
        "Screen Recording and Accessibility are on. Switch back to your own account — Atlas keeps \
             running there."
    );
    assert!(
        !switch_back("Alex").contains(';'),
        "one plain sentence, no semicolon"
    );
}

/// What a held arrival did, in order.
type Said = std::sync::Arc<std::sync::Mutex<Vec<&'static str>>>;

/// A notice that says it began, waits until `released` lets it go, as an
/// account lookup or the notice app's launch may, and is noted in `said`.
fn held_notice(
    said: &Said,
    began: tokio::sync::oneshot::Sender<()>,
    released: std::sync::mpsc::Receiver<()>,
) -> impl FnOnce() + Send + 'static {
    let said = said.clone();
    move || {
        let _ = began.send(());
        released.recv().unwrap();
        said.lock().unwrap().push("notice");
    }
}

/// Run `watcher` on a runtime of one thread, in a thread of its own, so the
/// test can let a held notice go even when that runtime stalls.
fn on_one_thread(
    watcher: impl std::future::Future<Output = ()> + Send + 'static,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(watcher);
    })
}

#[test]
fn a_blocked_arrival_notice_leaves_the_watcher_responsive_and_restarts_after_it() {
    let said = Said::default();
    let (release, released) = std::sync::mpsc::channel();
    let (answered, responsive) = std::sync::mpsc::channel();
    let (began, beginning) = tokio::sync::oneshot::channel();
    let notice = held_notice(&said, began, released);
    let restarted = said.clone();
    let watcher = on_one_thread(async move {
        let arrival = tokio::spawn(async move {
            let restart = move || restarted.lock().unwrap().push("restart");
            announce(notice, || false, &restart).await;
        });
        beginning.await.unwrap();
        let _ = answered.send(());
        arrival.await.unwrap();
    });
    let responsive = responsive.recv_timeout(Duration::from_secs(10));
    release.send(()).unwrap();
    watcher.join().unwrap();
    responsive.expect("the runtime stalled while the notice was prepared");
    assert_eq!(*said.lock().unwrap(), ["notice", "restart"]);
}

#[test]
fn stopping_the_watcher_never_waits_for_a_notice_and_shows_it_once() {
    let said = Said::default();
    let (release, released) = std::sync::mpsc::channel();
    let (answered, responsive) = std::sync::mpsc::channel();
    let (began, beginning) = tokio::sync::oneshot::channel();
    let notice = held_notice(&said, began, released);
    let (shown, showing) = tokio::sync::oneshot::channel();
    let notice = move || {
        notice();
        let _ = shown.send(());
    };
    let restarted = said.clone();
    let watcher = on_one_thread(async move {
        let arrival = tokio::spawn(async move {
            let restart = move || restarted.lock().unwrap().push("restart");
            announce(notice, || false, &restart).await;
        });
        beginning.await.unwrap();
        arrival.abort();
        let stopped = arrival.await.is_err_and(|error| error.is_cancelled());
        let _ = answered.send(stopped);
        showing.await.unwrap();
    });
    let stopped = responsive.recv_timeout(Duration::from_secs(10));
    release.send(()).unwrap();
    watcher.join().unwrap();
    assert!(
        stopped.expect("stopping waited for the notice"),
        "the watcher was stopped"
    );
    assert_eq!(
        *said.lock().unwrap(),
        ["notice"],
        "shown once, never restarted"
    );
}

#[test]
fn a_notice_is_remembered_for_its_login_only() {
    let home = tempfile::tempdir().unwrap();
    assert!(!shown_in(home.path(), 1158), "none shown yet");

    std::fs::create_dir_all(home.path().join(crate::helper_setup::ROOT)).unwrap();
    std::fs::write(notice_record(home.path()), "1158").unwrap();
    assert!(shown_in(home.path(), 1158));
    assert!(!shown_in(home.path(), 2001), "a later login shows its own");
}

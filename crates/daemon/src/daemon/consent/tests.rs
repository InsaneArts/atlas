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

#[test]
fn a_notice_is_remembered_for_its_login_only() {
    let home = tempfile::tempdir().unwrap();
    assert!(!shown_in(home.path(), 1158), "none shown yet");

    std::fs::create_dir_all(home.path().join(crate::helper_setup::ROOT)).unwrap();
    std::fs::write(notice_record(home.path()), "1158").unwrap();
    assert!(shown_in(home.path(), 1158));
    assert!(!shown_in(home.path(), 2001), "a later login shows its own");
}

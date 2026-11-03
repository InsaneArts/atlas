use super::*;
use std::time::Instant;

fn picture(key: &str) -> SharedFrame {
    SharedFrame {
        png: Vec::new(),
        rgb: Some(Arc::new(atlas_core::buffer::Buffer::zeroed(12).unwrap())),
        pixels: (2, 2),
        metrics: None,
        key: key.into(),
        display: (100.0, 100.0),
    }
}
fn state() -> watch::Sender<State> {
    watch::channel(State {
        revision: 0,
        frame: Some(Arc::new(picture("verified"))),
        controlling: true,
        paused: false,
        phase: ControlPhase::Controlling,
        view: viewport::Viewport::default(),
        diagnostics: 0,
        input_ack_ms: None,
        last_frame_error: None,
        message: "You have control".into(),
    })
    .0
}

#[tokio::test]
async fn a_late_picture_keeps_the_same_verified_frame_and_lease_without_replaying_input() {
    let states = state();
    let original = states.borrow().frame.clone().unwrap();
    let mut session = Session::new("http://127.0.0.1:1/".into(), false);
    session.lease = Some("existing".into());
    session.queue.push_back((
        "verified".into(),
        json!({"type":"pointer","phase":"up","button":"left","x":1,"y":1}),
    ));
    let mut good = Some(Instant::now());
    received(
        &mut session,
        &states,
        &mut good,
        Err(Error::new(Code::StaleFrame, "late capture")),
    );
    assert_eq!(session.lease.as_deref(), Some("existing"));
    assert!(!session.releasing);
    assert!(session.operations.is_empty());
    assert_eq!(
        session.queue.len(),
        1,
        "A pending mouse-up must not be dropped by capture jitter"
    );
    assert!(Arc::ptr_eq(
        &original,
        states.borrow().frame.as_ref().unwrap()
    ));
    assert!(states.borrow().controlling);
    received(&mut session, &states, &mut good, Ok(picture("fresh")));
    assert_eq!(states.borrow().frame.as_ref().unwrap().key, "fresh");
    assert!(states.borrow().message.starts_with("You have control"));
}

#[tokio::test]
async fn a_sustained_capture_gap_clears_the_picture_and_cancels_ownership() {
    let states = state();
    let mut session = Session::new("http://127.0.0.1:1/".into(), false);
    session.lease = Some("existing".into());
    let mut good = Some(Instant::now() - Duration::from_secs(3));
    received(
        &mut session,
        &states,
        &mut good,
        Err(Error::new(Code::StaleFrame, "late capture")),
    );
    assert!(states.borrow().frame.is_none());
    assert!(!states.borrow().controlling);
    assert!(session.releasing);
    assert!(good.is_none());
}

#[tokio::test]
async fn identity_permission_and_connection_errors_never_receive_the_jitter_grace_period() {
    for code in [
        Code::SessionChanged,
        Code::Identity,
        Code::OnConsole,
        Code::Permission,
        Code::Transport,
    ] {
        let states = state();
        let mut session = Session::new("http://127.0.0.1:1/".into(), false);
        session.lease = Some("existing".into());
        let mut good = Some(Instant::now());
        received(
            &mut session,
            &states,
            &mut good,
            Err(Error::new(code, "specific failure")),
        );
        assert!(states.borrow().frame.is_none());
        assert!(session.releasing);
        assert!(states.borrow().message.contains("specific failure"));
    }
}

#[tokio::test]
async fn fresh_observation_after_a_revocation_never_automatically_takes_control() {
    let states = state();
    let mut session = Session::new("http://127.0.0.1:1/".into(), false);
    let mut good = None;
    received(
        &mut session,
        &states,
        &mut good,
        Err(Error::new(Code::StaleFrame, "no valid picture yet")),
    );
    received(&mut session, &states, &mut good, Ok(picture("new")));
    assert!(session.lease.is_none());
    assert!(!states.borrow().controlling);
    assert!(session.operations.is_empty());
}

#[tokio::test]
async fn recovery_keeps_the_first_interruption_reason_until_another_outage_starts() {
    let states = state();
    let mut session = Session::new("http://127.0.0.1:1/".into(), false);
    let mut good = Some(Instant::now());
    received(
        &mut session,
        &states,
        &mut good,
        Err(Error::new(Code::Cancelled, "source stopped")),
    );
    received(
        &mut session,
        &states,
        &mut good,
        Err(Error::new(Code::Busy, "waiting for capture slot")),
    );
    received(&mut session, &states, &mut good, Ok(picture("recovered")));
    assert_eq!(
        states.borrow().last_frame_error.as_deref(),
        Some("cancelled: source stopped")
    );
    assert!(states.borrow().message.starts_with("Watching"));
    received(
        &mut session,
        &states,
        &mut good,
        Err(Error::new(Code::Transport, "reader closed")),
    );
    assert_eq!(
        states.borrow().last_frame_error.as_deref(),
        Some("transport: reader closed")
    );
}

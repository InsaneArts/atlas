//! All input stays in flight until the test explicitly acknowledges it.
//! Release has an independent route and never waits for that acknowledgement.
use super::service::Service;
use super::*;

fn held(service: &Service) -> Session {
    let mut session = Session::new(service.url.clone(), false);
    session.lease = Some("same-person-lease".into());
    session
}

fn input(session: &mut Session, states: &watch::Sender<State>, action: Value) {
    session.command(
        Command::Input {
            frame: "observed".into(),
            action,
        },
        states,
    );
}

fn pointer(phase: &str) -> Value {
    json!({"type":"pointer","phase":phase,"button":"left","x":10,"y":20,"modifiers":[]})
}

async fn completed(session: &mut Session, states: &watch::Sender<State>, operation: Operation) {
    let result = tokio::time::timeout(Duration::from_secs(2), session.operations.join_next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(result.0, operation);
    assert!(result.1.is_ok());
    session.completed(result, states);
}

#[tokio::test]
async fn blurring_drops_unsent_typing_and_a_late_key_reply_cannot_dispatch_more() {
    let mut service = Service::holding_input().await;
    let states = controlling();
    let mut session = held(&service);
    input(&mut session, &states, json!({"type":"key","key":"a"}));
    let key = service.received("/input").await;
    input(
        &mut session,
        &states,
        json!({"type":"type","text":"unsent"}),
    );
    assert!(!session.queue.is_empty());
    session.command(Command::FocusPause(true), &states);
    assert!(session.queue.is_empty());
    assert!(session.paused && states.borrow().paused);
    assert_eq!(session.lease.as_deref(), Some("same-person-lease"));
    assert!(!session.releasing, "an ordinary key is allowed to finish");
    input(
        &mut session,
        &states,
        json!({"type":"type","text":"blurred"}),
    );
    key.ok();
    completed(&mut session, &states, Operation::Input).await;
    assert!(session.queue.is_empty());
    assert!(session.operations.is_empty());
    service.no_more_requests();
}

#[tokio::test]
async fn focus_gain_keeps_the_original_lease_without_acquiring_or_replaying_input() {
    let mut service = Service::holding_input().await;
    let states = controlling();
    let mut session = held(&service);
    session.command(Command::FocusPause(true), &states);
    session.command(Command::Take("new-picture".into()), &states);
    input(&mut session, &states, json!({"type":"key","key":"return"}));
    session.command(Command::FocusPause(false), &states);
    assert!(!session.paused && !states.borrow().paused);
    assert!(states.borrow().controlling);
    assert_eq!(session.lease.as_deref(), Some("same-person-lease"));
    assert!(session.queue.is_empty());
    assert!(session.operations.is_empty());
    service.no_more_requests();
    input(&mut session, &states, json!({"type":"key","key":"tab"}));
    let request = service.received("/input").await;
    assert_eq!(request.body["lease"], "same-person-lease");
    assert_eq!(request.body["actions"][0]["key"], "tab");
    request.ok();
    completed(&mut session, &states, Operation::Input).await;
    service.no_more_requests();
}

#[tokio::test]
async fn an_acknowledged_mouse_down_is_released_when_blur_discards_its_queued_up() {
    let mut service = Service::holding_input().await;
    let states = controlling();
    let mut session = held(&service);
    input(&mut session, &states, pointer("down"));
    service.received("/input").await.ok();
    completed(&mut session, &states, Operation::Input).await;
    assert_ne!(session.held_buttons, 0);
    input(&mut session, &states, json!({"type":"key","key":"a"}));
    let key = service.received("/input").await;
    input(&mut session, &states, pointer("up"));
    assert_eq!(session.queue.len(), 1);
    session.command(Command::FocusPause(true), &states);
    service.received("/control/release").await;
    assert!(session.releasing && session.queue.is_empty());
    assert!(
        session.pending,
        "release must not wait for the earlier input reply"
    );
    completed(&mut session, &states, Operation::Release).await;
    key.ok();
    completed(&mut session, &states, Operation::Input).await;
    assert!(session.lease.is_none());
    assert_eq!(
        session.held_buttons, 0,
        "late input cannot restore a released button"
    );
    assert!(!states.borrow().controlling);
    service.no_more_requests();
}

#[tokio::test]
async fn every_inflight_pointer_gesture_is_cancelled_even_when_down_and_up_net_to_zero() {
    for actions in [
        vec![pointer("down")],
        vec![pointer("down"), pointer("up")],
        vec![pointer("move")],
    ] {
        let mut service = Service::holding_input().await;
        let states = controlling();
        let mut session = held(&service);
        session.queue.extend(
            actions
                .iter()
                .cloned()
                .map(|action| ("observed".into(), action)),
        );
        session.send_next();
        let request = service.received("/input").await;
        assert_eq!(request.body["actions"], json!(actions));
        session.command(Command::FocusPause(true), &states);
        assert!(
            session.releasing,
            "a pointer phase can be held inside an unfinished request"
        );
        service.received("/control/release").await;
        completed(&mut session, &states, Operation::Release).await;
        request.ok();
        completed(&mut session, &states, Operation::Input).await;
        assert!(!states.borrow().controlling);
        assert!(session.lease.is_none());
        assert_eq!(session.held_buttons, 0);
    }
}

#[tokio::test]
async fn focus_gain_after_release_or_unconfirmed_cleanup_never_restores_control() {
    for uncertain in [false, true] {
        let mut service = Service::holding_input().await;
        let states = controlling();
        let mut session = held(&service);
        session.paused = true;
        session.releasing = true;
        change(&states, |state| {
            state.controlling = false;
            state.paused = true;
        });
        session.completed(
            (
                Operation::Release,
                if uncertain {
                    Err(Error::new(Code::Uncertain, "cleanup unknown").uncertain())
                } else {
                    Ok(json!({"ok":true}))
                },
            ),
            &states,
        );
        session.command(Command::FocusPause(false), &states);
        assert!(!states.borrow().controlling);
        assert!(session.lease.is_none());
        assert!(session.operations.is_empty());
        assert_eq!(session.cleanup_error.is_some(), uncertain);
        service.no_more_requests();
    }
}

#[tokio::test]
async fn paused_sessions_ignore_new_take_and_input_but_give_back_still_reaches_cleanup() {
    let mut service = Service::holding_input().await;
    let states = controlling();
    let mut session = Session::new(service.url.clone(), false);
    session.command(Command::FocusPause(true), &states);
    session.command(Command::Take("observed".into()), &states);
    assert!(
        session.operations.is_empty(),
        "paused observers cannot acquire control"
    );
    session.lease = Some("same-person-lease".into());
    input(&mut session, &states, json!({"type":"key","key":"return"}));
    assert!(session.queue.is_empty() && session.operations.is_empty());
    session.command(Command::GiveBack, &states);
    service.received("/control/release").await;
    completed(&mut session, &states, Operation::Release).await;
    assert!(session.lease.is_none());
    assert!(!states.borrow().controlling);
}

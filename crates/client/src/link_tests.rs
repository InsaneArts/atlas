use super::*;
use atlas_core::files::Token;
use atlas_session::{
    authority::Retired,
    standin::{
        link::{Conditions, Link as Wire},
        serve::{Answer, Reached, reach},
    },
};
use serde_json::json;

/// A stand-in across a healthy link that answers each request with its
/// lane's kind, and the link to it.
async fn reached() -> (Reached, Link) {
    let answer: Answer =
        Arc::new(|header, request| json!({"ok": true, "kind": header.kind, "request": request}));
    let reached = reach(&Arc::new(Wire::new(Conditions::default())), answer).await;
    let link = Link::Session(Session {
        desktop: reached.desktop.clone(),
        timeout: Duration::from_secs(5),
    });
    (reached, link)
}

/// Wait until the stand-in heard `lane` cancelled.
async fn cancelled(reached: &Reached, lane: LaneId) {
    tokio::time::timeout(Duration::from_secs(5), async {
        while !reached.live.lanes.cancelled(lane) {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("the cancel arrives");
}

#[tokio::test]
async fn a_read_goes_on_a_lane_that_reads_and_its_reply_comes_back() {
    let (_reached, link) = reached().await;

    let reply = link.call(&json!({"op": "hello"})).await.unwrap();

    assert_eq!(reply["kind"], "read");
    assert_eq!(reply["request"]["op"], "hello");
}

#[tokio::test]
async fn a_change_is_refused_before_any_lane_opens_with_or_without_its_id() {
    let (reached, link) = reached().await;
    let change = Token::new();

    let named = link
        .call(&json!({"op": "run", "argv": ["true"], "change": change}))
        .await;
    let unnamed = link.call(&json!({"op": "sound", "muted": true})).await;
    let stopping = link
        .call(&json!({"op": "job", "action": "stop-all", "reason": "person"}))
        .await;

    for refused in [named, unnamed, stopping] {
        assert_eq!(refused.unwrap_err().code, Code::Platform);
    }
    assert!(reached.admitted.lock().unwrap().is_empty());
}

#[tokio::test]
async fn asking_what_a_change_came_to_is_a_read() {
    let (reached, link) = reached().await;

    let reply = link
        .call(&json!({"op": "outcome", "change": Token::new()}))
        .await
        .unwrap();

    assert_eq!(reply["kind"], "read");
    assert_eq!(reached.admitted.lock().unwrap()[0].kind, LaneKind::Read);
}

#[tokio::test]
async fn a_desktop_that_never_answers_the_handshake_times_out_within_the_request() {
    let wire = Arc::new(Wire::new(Conditions::default()));
    let answer: Answer = Arc::new(|_, _| json!({"ok": true}));
    let reached = reach(&wire, answer).await;
    let link = Link::Session(Session {
        desktop: reached.desktop.clone(),
        timeout: Duration::from_millis(300),
    });
    wire.set(Conditions {
        partitioned: true,
        ..Conditions::default()
    });
    let started = std::time::Instant::now();

    let error = link.call(&json!({"op": "hello"})).await.unwrap_err();

    assert_eq!(error.code, Code::Transport);
    assert!(started.elapsed() < Duration::from_secs(2));
}

#[tokio::test]
async fn a_lane_of_a_retired_generation_is_refused() {
    let (reached, link) = reached().await;
    reached.live.authority.retire(Retired::Revoked);

    assert!(link.call(&json!({"op": "hello"})).await.is_err());
    assert!(reached.admitted.lock().unwrap().is_empty());
}

#[tokio::test]
async fn a_request_given_up_before_its_reply_is_cancelled_over_control() {
    let (reached, link) = reached().await;

    let (sent, unanswered) = link.send(&json!({"op": "hello"})).await.unwrap();
    drop(sent);
    drop(unanswered);

    let admitted = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Some(header) = reached.admitted.lock().unwrap().first() {
                return header.lane;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    cancelled(&reached, admitted).await;
}

#[tokio::test]
async fn a_request_answered_whole_cancels_nothing() {
    let (reached, link) = reached().await;

    link.call(&json!({"op": "hello"})).await.unwrap();
    // A later cancel, which this one would have queued ahead of, arrives
    // alone.
    let later = LaneId(99);
    reached.desktop.cancel(later);
    cancelled(&reached, later).await;

    let lane = reached.admitted.lock().unwrap()[0].lane;
    assert!(!reached.live.lanes.cancelled(lane));
}

#[tokio::test]
async fn the_helper_logins_socket_is_refused_for_a_machine_desktop() {
    let (_reached, link) = reached().await;

    let error = link.socket().unwrap_err();

    assert_eq!(error.code, Code::Platform);
}

//! Viewer mutations are fake HTTP requests. Tests explicitly acknowledge each
//! one, so viewport publication cannot hide an unfinished helper operation.
use super::*;
use axum::{Json, Router, extract::Request, http::StatusCode, routing::post};
use tokio::sync::oneshot;

#[derive(Debug)]
struct Asked {
    route: String,
    body: Value,
    reply: oneshot::Sender<(StatusCode, Value)>,
}

impl Asked {
    fn answer(self, status: StatusCode, body: Value) {
        self.reply.send((status, body)).unwrap();
    }

    fn ok(self) {
        self.answer(StatusCode::OK, json!({"ok":true}));
    }

    fn unknown(self) {
        self.answer(
            StatusCode::SERVICE_UNAVAILABLE,
            json!({"ok":false,"code":"uncertain","error":"Native acknowledgment is missing","uncertain":true}),
        );
    }
}

struct Service {
    url: String,
    requests: mpsc::UnboundedReceiver<Asked>,
    server: tokio::task::JoinHandle<()>,
}

impl Service {
    async fn start() -> Self {
        let (sent, requests) = mpsc::unbounded_channel();
        let router = Router::new().fallback(post(move |request: Request| {
            let sent = sent.clone();
            async move {
                let route = request.uri().path().to_owned();
                let bytes = axum::body::to_bytes(request.into_body(), 4096)
                    .await
                    .unwrap();
                let body = serde_json::from_slice(&bytes).unwrap();
                let (reply, result) = oneshot::channel();
                sent.send(Asked { route, body, reply }).unwrap();
                let (status, body) = result.await.unwrap();
                (status, Json(body))
            }
        }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/", listener.local_addr().unwrap());
        let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        Self {
            url,
            requests,
            server,
        }
    }

    async fn received(&mut self, route: &str) -> Asked {
        let request = tokio::time::timeout(Duration::from_secs(2), self.requests.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(request.route, route);
        request
    }

    fn quiet(&mut self, session: &Session) {
        assert!(
            session.operations.is_empty(),
            "No deferred request may still reach the helper"
        );
        assert!(matches!(
            self.requests.try_recv(),
            Err(mpsc::error::TryRecvError::Empty)
        ));
    }
}

impl Drop for Service {
    fn drop(&mut self) {
        self.server.abort();
    }
}

fn states(controlling: bool) -> watch::Sender<State> {
    watch::channel(State {
        revision: 0,
        frame: Some(Arc::new(SharedFrame {
            png: Vec::new(),
            rgb: Some(Arc::new(atlas_core::buffer::Buffer::zeroed(12).unwrap())),
            pixels: (2, 2),
            metrics: None,
            key: "observed".into(),
            display: (1000.0, 800.0),
        })),
        controlling,
        paused: false,
        phase: if controlling {
            ControlPhase::Controlling
        } else {
            ControlPhase::Watching
        },
        message: String::new(),
        view: viewport::Viewport::default(),
        diagnostics: 0,
        input_ack_ms: None,
        last_frame_error: None,
    })
    .0
}

fn held(service: &Service) -> Session {
    let mut session = Session::new(service.url.clone(), false);
    session.lease = Some("original-person-lease".into());
    session
}

fn input(session: &mut Session, states: &watch::Sender<State>, generation: u64, action: Value) {
    session.command(
        Command::InputView {
            frame: "observed".into(),
            action,
            generation,
        },
        states,
    );
}

async fn completed(
    session: &mut Session,
    states: &watch::Sender<State>,
    expected: Operation,
) -> bool {
    let completion = tokio::time::timeout(Duration::from_secs(2), session.operations.join_next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(completion.0, expected);
    let unknown = completion.1.as_ref().is_err_and(|error| error.uncertain);
    session.completed(completion, states);
    unknown
}

#[tokio::test]
async fn releasing_before_input_finishes_cannot_publish_a_new_view_or_replay_queued_input() {
    let mut service = Service::start().await;
    let states = states(true);
    let mut session = held(&service);
    input(
        &mut session,
        &states,
        0,
        json!({"type":"pointer","phase":"down","button":"left","x":10,"y":20}),
    );
    let inflight = service.received("/input").await;
    input(
        &mut session,
        &states,
        0,
        json!({"type":"type","text":"never replay"}),
    );
    assert_eq!(session.queue.len(), 1);
    session.command(Command::View(viewport::Change::ZoomIn), &states);
    assert!(session.queue.is_empty());
    assert!(!states.borrow().controlling);
    let release = service.received("/control/release").await;
    assert_eq!(release.body["lease"], "original-person-lease");
    release.ok();
    assert!(!completed(&mut session, &states, Operation::Release).await);
    assert!(session.pending && session.lease.is_none());
    assert_eq!(states.borrow().view.generation, 0);
    assert!(session.pending_view.is_some());
    session.command(Command::TakeView("observed".into(), 0), &states);
    inflight.ok();
    assert!(!completed(&mut session, &states, Operation::Input).await);
    assert_eq!(states.borrow().view.label(), "150%");
    assert_eq!(states.borrow().view.generation, 1);
    assert_eq!(states.borrow().phase, ControlPhase::Watching);
    assert_eq!(
        session.held_buttons, 0,
        "A late Down reply cannot resurrect a released gesture"
    );
    assert!(session.pending_view.is_none());
    service.quiet(&session);
}

#[tokio::test]
async fn a_view_change_during_acquisition_releases_the_late_lease_before_publishing() {
    let mut service = Service::start().await;
    let states = states(false);
    let mut session = Session::new(service.url.clone(), false);
    session.command(Command::TakeView("observed".into(), 0), &states);
    let acquire = service.received("/control/acquire").await;
    session.command(Command::View(viewport::Change::ZoomIn), &states);
    assert!(session.pending && session.acquire_cancelled);
    assert_eq!(states.borrow().view.generation, 0);
    acquire.answer(
        StatusCode::OK,
        json!({"ok":true,"lease":"late-person-lease"}),
    );
    assert!(!completed(&mut session, &states, Operation::Acquire).await);
    assert!(!states.borrow().controlling);
    assert_eq!(states.borrow().view.generation, 0);
    let release = service.received("/control/release").await;
    assert_eq!(release.body["lease"], "late-person-lease");
    release.ok();
    assert!(!completed(&mut session, &states, Operation::Release).await);
    assert_eq!(states.borrow().view.label(), "150%");
    assert_eq!(states.borrow().phase, ControlPhase::Watching);
    assert!(session.lease.is_none() && session.pending_view.is_none());
    service.quiet(&session);
}

#[tokio::test]
async fn an_unconfirmed_release_keeps_the_old_view_and_blocks_taking_control() {
    let mut service = Service::start().await;
    let states = states(true);
    let mut session = held(&service);
    session.command(Command::View(viewport::Change::ZoomIn), &states);
    service.received("/control/release").await.unknown();
    assert!(completed(&mut session, &states, Operation::Release).await);
    assert_eq!(states.borrow().view.generation, 0);
    assert_eq!(states.borrow().phase, ControlPhase::Uncertain);
    assert!(session.cleanup_error.is_some());
    assert!(session.pending_view.is_some());
    session.command(Command::TakeView("observed".into(), 0), &states);
    session.command(Command::View(viewport::Change::ZoomIn), &states);
    assert_eq!(states.borrow().view.label(), "Fit");
    assert_eq!(session.pending_view.unwrap().label(), "200%");
    service.quiet(&session);
}

#[tokio::test]
async fn an_unknown_input_outcome_blocks_view_publication_even_after_cleanup_was_acknowledged() {
    let mut service = Service::start().await;
    let states = states(true);
    let mut session = held(&service);
    input(
        &mut session,
        &states,
        0,
        json!({"type":"key","key":"return"}),
    );
    let inflight = service.received("/input").await;
    session.command(Command::View(viewport::Change::ZoomIn), &states);
    service.received("/control/release").await.ok();
    assert!(!completed(&mut session, &states, Operation::Release).await);
    inflight.unknown();
    assert!(completed(&mut session, &states, Operation::Input).await);
    assert_eq!(states.borrow().view.generation, 0);
    assert_eq!(states.borrow().phase, ControlPhase::Uncertain);
    assert!(session.pending_view.is_some() && session.cleanup_error.is_some());
    service.quiet(&session);
}

#[tokio::test]
async fn old_presented_generations_cannot_acquire_or_send_input_after_a_new_view_is_applied() {
    let mut service = Service::start().await;
    let states = states(false);
    let mut session = Session::new(service.url.clone(), false);
    session.command(Command::View(viewport::Change::ZoomIn), &states);
    let generation = states.borrow().view.generation;
    assert_eq!(generation, 1);
    session.command(Command::TakeView("observed".into(), 0), &states);
    service.quiet(&session);
    session.command(Command::TakeView("observed".into(), generation), &states);
    let acquire = service.received("/control/acquire").await;
    assert_eq!(acquire.body["frame"], "observed");
    acquire.answer(
        StatusCode::OK,
        json!({"ok":true,"lease":"new-person-lease"}),
    );
    assert!(!completed(&mut session, &states, Operation::Acquire).await);
    assert!(states.borrow().controlling);
    let action = json!({"type":"move","x":760,"y":420});
    input(&mut session, &states, 0, action.clone());
    assert!(session.queue.is_empty());
    service.quiet(&session);
    input(&mut session, &states, generation, action.clone());
    let request = service.received("/input").await;
    assert_eq!(request.body["frame"], "observed");
    assert_eq!(request.body["lease"], "new-person-lease");
    assert_eq!(request.body["actions"], json!([action]));
    request.ok();
    assert!(!completed(&mut session, &states, Operation::Input).await);
    service.quiet(&session);
}

#[tokio::test]
async fn rapid_view_requests_coalesce_while_one_cleanup_remains_in_flight() {
    let mut service = Service::start().await;
    let states = states(true);
    let mut session = held(&service);
    let changes = [
        viewport::Change::ZoomIn,
        viewport::Change::ZoomIn,
        viewport::Change::PanRight,
        viewport::Change::PanDown,
        viewport::Change::ZoomIn,
    ];
    let expected = changes
        .iter()
        .fold(viewport::Viewport::default(), |view, change| {
            view.changed(*change)
        });
    session.command(Command::View(changes[0]), &states);
    let release = service.received("/control/release").await;
    for change_view in &changes[1..] {
        session.command(Command::View(*change_view), &states);
    }
    assert_eq!(
        session.operations.len(),
        1,
        "Cleanup is serialized despite repeated local view changes"
    );
    assert_eq!(states.borrow().view.generation, 0);
    assert_eq!(session.pending_view, Some(expected));
    release.ok();
    assert!(!completed(&mut session, &states, Operation::Release).await);
    assert_eq!(states.borrow().view, expected);
    assert_eq!(states.borrow().view.label(), "300%");
    assert!(session.pending_view.is_none());
    service.quiet(&session);
}

#[tokio::test]
async fn read_only_zoom_and_pan_never_acquire_control_or_mutate_the_helper() {
    let mut service = Service::start().await;
    let states = states(false);
    let mut session = Session::new(service.url.clone(), true);
    for change_view in [
        viewport::Change::ZoomIn,
        viewport::Change::PanRight,
        viewport::Change::PanDown,
        viewport::Change::ZoomIn,
        viewport::Change::Fit,
    ] {
        session.command(Command::View(change_view), &states);
    }
    let generation = states.borrow().view.generation;
    assert!(generation > 0);
    assert_eq!(states.borrow().view.label(), "Fit");
    session.command(Command::TakeView("observed".into(), generation), &states);
    input(
        &mut session,
        &states,
        generation,
        json!({"type":"key","key":"return"}),
    );
    session.command(Command::Stop, &states);
    assert!(session.lease.is_none() && session.queue.is_empty());
    assert!(!states.borrow().controlling);
    service.quiet(&session);
}

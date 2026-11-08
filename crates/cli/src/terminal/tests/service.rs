//! A loopback viewer service. Each request reaches the test with its route
//! and body, and waits for the test's answer unless its route is answered at
//! once.
use super::*;
use axum::{Json, Router, extract::Request, http::StatusCode, routing::post};
use tokio::sync::oneshot;

#[derive(Debug)]
pub(super) struct Asked {
    pub(super) route: String,
    pub(super) body: Value,
    reply: oneshot::Sender<(StatusCode, Value)>,
}

impl Asked {
    pub(super) fn answer(self, status: StatusCode, body: Value) {
        self.reply.send((status, body)).unwrap();
    }

    pub(super) fn ok(self) {
        self.answer(StatusCode::OK, json!({"ok":true}));
    }

    pub(super) fn unknown(self) {
        self.answer(
            StatusCode::SERVICE_UNAVAILABLE,
            json!({"ok":false,"code":"uncertain","error":"Native acknowledgment is missing","uncertain":true}),
        );
    }
}

pub(super) struct Service {
    pub(super) url: String,
    requests: mpsc::UnboundedReceiver<Asked>,
    server: tokio::task::JoinHandle<()>,
}

impl Service {
    /// Every request waits until the test answers it.
    pub(super) async fn start() -> Self {
        Self::answering(|_| false).await
    }

    /// Input waits until the test answers it; every other route is
    /// answered at once, so release never waits for input.
    pub(super) async fn holding_input() -> Self {
        Self::answering(|route| route != "/input").await
    }

    async fn answering(at_once: fn(&str) -> bool) -> Self {
        let (sent, requests) = mpsc::unbounded_channel();
        let router = Router::new().fallback(post(move |request: Request| {
            let sent = sent.clone();
            async move {
                let route = request.uri().path().to_owned();
                let bytes = axum::body::to_bytes(request.into_body(), 4096)
                    .await
                    .unwrap();
                let body = serde_json::from_slice(&bytes).unwrap();
                let now = at_once(&route);
                let (reply, result) = oneshot::channel();
                sent.send(Asked { route, body, reply }).unwrap();
                if now {
                    return (StatusCode::OK, Json(json!({"ok":true})));
                }
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

    pub(super) async fn received(&mut self, route: &str) -> Asked {
        let request = tokio::time::timeout(Duration::from_secs(2), self.requests.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(request.route, route);
        request
    }

    pub(super) fn no_more_requests(&mut self) {
        assert!(matches!(
            self.requests.try_recv(),
            Err(mpsc::error::TryRecvError::Empty)
        ));
    }

    pub(super) fn quiet(&mut self, session: &Session) {
        assert!(
            session.operations.is_empty(),
            "No deferred request may still reach the helper"
        );
        self.no_more_requests();
    }
}

impl Drop for Service {
    fn drop(&mut self) {
        self.server.abort();
    }
}

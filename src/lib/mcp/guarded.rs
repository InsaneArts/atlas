//! Private per-view contexts for hosts that do not filter app-only tools.
//! Authority never appears in resources or public result fields.
use super::*;
use std::sync::{Mutex as Lock, atomic::AtomicUsize};

pub(super) const ARGUMENT: &str = "_atlasView";
const META: &str = "com.insanearts.atlas/view";
const LIMIT: usize = 16;
const IDLE: Duration = Duration::from_secs(15 * 60);

pub(super) struct Reply {
    pub value: Value,
    // Kept until the response has been written (or its task unwinds).
    _reservation: Option<Reservation>,
}

impl Reply {
    pub fn new(value: Value) -> Self {
        Self {
            value,
            _reservation: None,
        }
    }
    pub fn delivered(&mut self) {
        if let Some(reservation) = &mut self._reservation {
            reservation.settled = true;
        }
    }
}

struct Entry {
    token: String,
    child: Arc<Server>,
    used: Instant,
    closing: bool,
    active: Arc<AtomicUsize>,
    // A timed-out acquire may have reached the viewer without returning a lease.
    // Never infer acknowledgement from held=None or discard this context on TTL.
    uncertain: Arc<AtomicBool>,
    cleaning: Arc<AtomicBool>,
}

struct Cleaning(Arc<AtomicBool>);
impl Drop for Cleaning {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

struct Reservation {
    active: Arc<AtomicUsize>,
    uncertain: Arc<AtomicBool>,
    mutation: bool,
    settled: bool,
}
impl Drop for Reservation {
    fn drop(&mut self) {
        // Publish uncertainty before the last reservation disappears; the
        // reaper must observe both in this order on every architecture.
        if self.mutation && !self.settled {
            self.uncertain.store(true, Ordering::SeqCst);
        }
        self.active.fetch_sub(1, Ordering::SeqCst);
    }
}

#[derive(Default)]
pub(super) struct Registry {
    entries: Lock<Vec<Entry>>,
}

impl Registry {
    fn locked(&self) -> std::sync::MutexGuard<'_, Vec<Entry>> {
        self.entries
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn is_empty(&self) -> bool {
        self.locked().is_empty()
    }

    fn child(root: &Server) -> Arc<Server> {
        Arc::new(Server {
            client: root.client.clone(),
            config: root.config.clone(),
            port: root.port,
            read_only: root.read_only,
            app_views: OnceLock::new(),
            views: AtomicBool::new(true),
            last: Mutex::new(None),
            framed: Mutex::new(None),
            shared: Mutex::new(None),
            held: Mutex::new(None),
            icons: Icons::default(),
        })
    }

    fn matches(left: &str, right: &str) -> bool {
        left.len() == crate::control::TOKEN_BYTES * 2
            && left.len() == right.len()
            && left
                .bytes()
                .zip(right.bytes())
                .fold(0_u8, |difference, (a, b)| difference | (a ^ b))
                == 0
    }

    fn reserve(
        &self,
        root: &Server,
        token: Option<&Value>,
        bootstrap: bool,
        mutation: bool,
    ) -> Option<(Arc<Server>, Reservation, Option<String>)> {
        let mut entries = self.locked();
        let now = Instant::now();
        for entry in entries.iter_mut() {
            if now.duration_since(entry.used) >= IDLE {
                entry.closing = true;
            }
        }
        let minted = if bootstrap && token.is_none() {
            if entries.len() >= LIMIT {
                return None;
            }
            entries.push(Entry {
                token: crate::control::token(),
                child: Self::child(root),
                used: now,
                closing: false,
                active: Arc::new(AtomicUsize::new(0)),
                uncertain: Arc::new(AtomicBool::new(false)),
                cleaning: Arc::new(AtomicBool::new(false)),
            });
            Some(entries.len() - 1)
        } else {
            None
        };
        let index = minted.or_else(|| {
            let token = token?.as_str()?;
            entries
                .iter()
                .position(|entry| Self::matches(&entry.token, token))
        })?;
        let entry = &mut entries[index];
        if entry.closing {
            return None;
        }
        entry.used = now;
        entry.active.fetch_add(1, Ordering::SeqCst);
        Some((
            entry.child.clone(),
            Reservation {
                active: entry.active.clone(),
                uncertain: entry.uncertain.clone(),
                mutation,
                settled: false,
            },
            minted.map(|_| entry.token.clone()),
        ))
    }

    pub async fn call(&self, root: &Server, id: &Value, params: &Value) -> Reply {
        let name = params["name"].as_str().unwrap_or_default();
        if !root.offers(name) {
            return Reply::new(failure(id, -32602, "Unknown tool"));
        }
        let token = params["arguments"].get(ARGUMENT);
        let guarded = root.views.load(Ordering::SeqCst) && !root.read_only;
        let mutation = !LOOKING.contains(&name);
        let bootstrap = name == "show_screen" && params["arguments"]["_atlasOpen"] == true;
        if guarded && (token.is_some() || bootstrap || mutation) {
            let Some((child, reservation, minted)) = self.reserve(root, token, bootstrap, mutation)
            else {
                return Reply::new(failure(
                    id,
                    -32602,
                    "This view's authority is missing, expired or unavailable",
                ));
            };
            if name == "view_close" {
                let mut entries = self.locked();
                if let Some(entry) = entries
                    .iter_mut()
                    .find(|entry| Arc::ptr_eq(&entry.child, &child))
                {
                    entry.closing = true;
                }
                return Reply {
                    value: success(
                        id,
                        json!({"content":[],"structuredContent":{"closed":true}}),
                    ),
                    _reservation: Some(reservation),
                };
            }
            let result = child.call(params).await;
            if mutation
                && (result["structuredContent"]["error"]["uncertain"] == true
                    || result["structuredContent"]["error"]["code"] == "menubar")
            {
                reservation.uncertain.store(true, Ordering::SeqCst);
            }
            let mut result = result;
            if let Some(token) = minted {
                result["_meta"][META] = json!(token);
                result["structuredContent"]["guardAppControls"] = json!(true);
                result["structuredContent"]["canControl"] = json!(false);
                result["structuredContent"]["canResume"] = json!(false);
                result["structuredContent"]["control"] = json!(false);
            }
            return Reply {
                value: success(id, result),
                _reservation: Some(reservation),
            };
        }
        if token.is_some() || mutation {
            return Reply::new(failure(id, -32602, "This view has no control authority"));
        }
        let mut result = root.call(params).await;
        result["structuredContent"]["canControl"] = json!(false);
        result["structuredContent"]["canResume"] = json!(false);
        result["structuredContent"]["control"] = json!(false);
        result["structuredContent"]["guardAppControls"] = json!(guarded);
        Reply::new(success(id, result))
    }

    pub async fn sweep(&self, shutdown: bool) {
        let candidates = {
            let mut entries = self.locked();
            entries
                .iter_mut()
                .filter_map(|entry| {
                    if shutdown || entry.used.elapsed() >= IDLE {
                        entry.closing = true;
                    }
                    if !entry.closing
                        || entry.cleaning.load(Ordering::SeqCst)
                        || entry.active.load(Ordering::SeqCst) != 0
                    {
                        return None;
                    }
                    entry.cleaning.store(true, Ordering::SeqCst);
                    Some((entry.child.clone(), Cleaning(entry.cleaning.clone())))
                })
                .collect::<Vec<_>>()
        };
        for (child, _cleaning) in candidates {
            // give_back retains a refused lease. A missing held lease alone
            // does not settle an unknown acquire; sticky uncertainty stays.
            if child.held.lock().await.is_some() {
                let _ = child.give_back().await;
            }
            let held = child.held.lock().await.is_some();
            let mut entries = self.locked();
            entries.retain_mut(|entry| {
                if !Arc::ptr_eq(&entry.child, &child) {
                    return true;
                }
                held || entry.uncertain.load(Ordering::SeqCst)
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root(path: &std::path::Path) -> Server {
        let root = super::super::tests::server(path);
        root.app_views.get_or_init(|| Arc::new(Registry::default()));
        root.views.store(true, Ordering::SeqCst);
        root
    }

    #[tokio::test]
    async fn bootstrap_credentials_are_private_and_public_calls_never_claim_control() {
        let dir = tempfile::tempdir().unwrap();
        let root = root(dir.path());
        let registry = root.app_views.get().unwrap();
        let public = registry
            .call(&root, &json!(1), &json!({"name":"show_screen"}))
            .await;
        assert!(
            registry.is_empty(),
            "ordinary model calls allocate no authority"
        );
        assert_eq!(
            public.value["result"]["structuredContent"]["canControl"],
            false
        );
        assert_eq!(
            public.value["result"]["structuredContent"]["control"],
            false
        );
        let opened = registry
            .call(
                &root,
                &json!(2),
                &json!({"name":"show_screen","arguments":{"_atlasOpen":true}}),
            )
            .await;
        let token = opened.value["result"]["_meta"][META].as_str().unwrap();
        assert_eq!(token.len(), crate::control::TOKEN_BYTES * 2);
        let mut visible = opened.value.clone();
        visible["result"].as_object_mut().unwrap().remove("_meta");
        assert!(!visible.to_string().contains(token));
        for flag in ["canControl", "canResume", "control"] {
            assert_eq!(visible["result"]["structuredContent"][flag], false);
        }
        assert!(!screen_contents().to_string().contains(token));
        assert!(!root.tools().to_string().contains(token));
        let (child, _reservation, minted) = registry
            .reserve(&root, Some(&json!(token)), false, false)
            .unwrap();
        assert!(minted.is_none());
        assert!(child.app_views.get().is_none());
        assert!(!child.controlling().await);
        assert_eq!(child.client.actor, root.client.actor);
        assert!(child.client.controller.pending().unwrap().is_empty());
    }

    #[tokio::test]
    async fn every_mutation_is_refused_before_reaching_a_child_without_valid_authority() {
        let dir = tempfile::tempdir().unwrap();
        let root = root(dir.path());
        let registry = root.app_views.get().unwrap();
        let names = root.tools();
        for tool in names
            .as_array()
            .unwrap()
            .iter()
            .filter(|tool| !LOOKING.contains(&tool["name"].as_str().unwrap()))
        {
            for arguments in [
                json!({}),
                json!({ARGUMENT:7}),
                json!({ARGUMENT:"bad"}),
                json!({ARGUMENT:"0".repeat(48)}),
            ] {
                let refused = registry
                    .call(
                        &root,
                        &json!(1),
                        &json!({"name":tool["name"],"arguments":arguments}),
                    )
                    .await;
                assert_eq!(refused.value["error"]["code"], -32602);
            }
        }
        assert!(registry.is_empty());
        assert!(!dir.path().join("state").exists());
        let refused = registry
            .call(
                &root,
                &json!(1),
                &json!({"name":"screen_frame","arguments":{ARGUMENT:"bad"}}),
            )
            .await;
        assert_eq!(
            refused.value["error"]["code"], -32602,
            "bad read tokens cannot fall back"
        );
    }

    #[tokio::test]
    async fn guarded_cli_and_read_only_sessions_never_offer_mutations() {
        let dir = tempfile::tempdir().unwrap();
        let mut root = root(dir.path());
        root.views.store(false, Ordering::SeqCst);
        let names = root
            .tools()
            .as_array()
            .unwrap()
            .iter()
            .map(|tool| tool["name"].as_str().unwrap().to_owned())
            .collect::<Vec<_>>();
        assert_eq!(names, ["atlas_status", "show_screen", "screen_frame"]);
        root.views.store(true, Ordering::SeqCst);
        root.read_only = true;
        for tool in root.tools().as_array().unwrap() {
            assert!(LOOKING.contains(&tool["name"].as_str().unwrap()));
        }
        let registry = root.app_views.get().unwrap();
        let reply = registry
            .call(
                &root,
                &json!(1),
                &json!({"name":"show_screen","arguments":{"_atlasOpen":true}}),
            )
            .await;
        assert!(reply.value["result"]["_meta"][META].is_null());
        assert!(registry.is_empty());
    }

    #[tokio::test]
    async fn contexts_are_bounded_and_tokens_cannot_cross_processes_or_expiry() {
        let dir = tempfile::tempdir().unwrap();
        let root = root(dir.path());
        let registry = root.app_views.get().unwrap();
        let mut tokens = Vec::new();
        for _ in 0..LIMIT {
            let (_, reservation, token) = registry.reserve(&root, None, true, false).unwrap();
            tokens.push(token.unwrap());
            drop(reservation);
        }
        assert!(registry.reserve(&root, None, true, false).is_none());
        assert!(
            Registry::default()
                .reserve(&root, Some(&json!(tokens[0])), false, false)
                .is_none()
        );
        registry.locked()[0].used = Instant::now() - IDLE - Duration::from_secs(1);
        assert!(
            registry
                .reserve(&root, Some(&json!(tokens[0])), false, false)
                .is_none()
        );
        registry.sweep(false).await;
        assert_eq!(registry.locked().len(), LIMIT - 1);
        assert!(registry.reserve(&root, None, true, false).is_some());
    }

    #[tokio::test]
    async fn expiry_waits_for_inflight_acquisition_and_retains_unknown_outcomes() {
        let dir = tempfile::tempdir().unwrap();
        let root = root(dir.path());
        let registry = root.app_views.get().unwrap();
        let (_, reservation, token) = registry.reserve(&root, None, true, true).unwrap();
        registry.locked()[0].used = Instant::now() - IDLE - Duration::from_secs(1);
        registry.sweep(false).await;
        assert_eq!(registry.locked().len(), 1);
        assert!(
            registry
                .reserve(&root, Some(&json!(token)), false, false)
                .is_none()
        );
        drop(reservation); // cancelled acquire: Held is empty but outcome is unknown
        registry.sweep(true).await;
        assert_eq!(registry.locked().len(), 1);
        assert!(registry.locked()[0].uncertain.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn response_delivery_and_teardown_preserve_other_views() {
        let dir = tempfile::tempdir().unwrap();
        let root = root(dir.path());
        let registry = root.app_views.get().unwrap();
        let (first, reservation, token) = registry.reserve(&root, None, true, true).unwrap();
        let (second, second_reservation, _) = registry.reserve(&root, None, true, false).unwrap();
        assert!(!Arc::ptr_eq(&first, &second));
        assert!(!std::ptr::eq(&first.held, &second.held));
        let mut delivered = Reply {
            value: json!({}),
            _reservation: Some(reservation),
        };
        delivered.delivered();
        drop(delivered);
        let mut closed = registry
            .call(
                &root,
                &json!(1),
                &json!({"name":"view_close","arguments":{ARGUMENT:token}}),
            )
            .await;
        registry.sweep(false).await;
        assert_eq!(
            registry.locked().len(),
            2,
            "reply handling retains its reservation"
        );
        closed.delivered();
        drop(closed);
        registry.sweep(false).await;
        assert_eq!(registry.locked().len(), 1);
        assert!(Arc::ptr_eq(&registry.locked()[0].child, &second));
        drop(second_reservation);
        registry.sweep(true).await;
        assert!(registry.is_empty(), "EOF cleans settled contexts");

        let (_, reservation, _) = registry.reserve(&root, None, true, true).unwrap();
        drop(Reply {
            value: json!({}),
            _reservation: Some(reservation),
        });
        registry.sweep(true).await;
        assert!(
            !registry.is_empty(),
            "a failed response write cannot claim acknowledgement"
        );
    }

    #[tokio::test]
    async fn refused_cleanup_keeps_its_lease_and_retries_without_touching_another_view() {
        use axum::{Router, routing::post};
        let dir = tempfile::tempdir().unwrap();
        let root = root(dir.path());
        let registry = root.app_views.get().unwrap();
        let (first, reservation, _) = registry.reserve(&root, None, true, false).unwrap();
        let (second, other, _) = registry.reserve(&root, None, true, false).unwrap();
        drop(reservation);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let refuse = Arc::new(AtomicBool::new(true));
        let response = refuse.clone();
        let app = Router::new().route(
            "/view/control/release",
            post(move || {
                let response = response.clone();
                async move {
                    if response.load(Ordering::SeqCst) {
                        (
                            axum::http::StatusCode::SERVICE_UNAVAILABLE,
                            axum::Json(
                                json!({"ok":false,"code":"busy","error":"fixture refuses release"}),
                            ),
                        )
                    } else {
                        (axum::http::StatusCode::OK, axum::Json(json!({"ok":true})))
                    }
                }
            }),
        );
        let serving = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        *first.held.lock().await = Some(Held {
            url: format!("http://{address}/view/"),
            lease: "fixture-first".into(),
            kept: Instant::now(),
        });
        *second.held.lock().await = Some(Held {
            url: "http://127.0.0.1:1/other/".into(),
            lease: "fixture-second".into(),
            kept: Instant::now(),
        });
        registry.locked()[0].closing = true;
        registry.sweep(false).await;
        assert_eq!(
            first.held.lock().await.as_ref().unwrap().lease,
            "fixture-first"
        );
        assert_eq!(registry.locked().len(), 2);
        refuse.store(false, Ordering::SeqCst);
        registry.sweep(false).await;
        assert!(first.held.lock().await.is_none());
        assert_eq!(registry.locked().len(), 1);
        assert_eq!(
            second.held.lock().await.as_ref().unwrap().lease,
            "fixture-second"
        );
        drop(other);
        serving.abort();
        let _ = serving.await;
    }
}

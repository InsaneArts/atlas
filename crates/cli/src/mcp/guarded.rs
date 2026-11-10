//! Private per-view contexts for hosts that do not filter app-only tools.
//! Authority never appears in resources or public result fields.
use super::{
    INVALID_PARAMS, Server, failure,
    resources::{Access, Tool},
    success,
};
use atlas_core::error::{Code, Error, Result};
use serde_json::{Value, json};
use std::{
    sync::{
        Arc, Mutex, MutexGuard,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

pub(super) const ARGUMENT: &str = "_atlasView";
/// The argument that asks for a new context as the view opens.
const OPEN: &str = "_atlasOpen";
const META: &str = "com.insanearts.atlas/view";
const LIMIT: usize = 16;
const IDLE: Duration = Duration::from_secs(15 * 60);

/// `tool`'s catalog entry with the private arguments a view sends it.
pub(super) fn catalog(tool: Tool) -> Value {
    let mut listed = tool.catalog();
    let schema = &mut listed["inputSchema"];
    schema["properties"][ARGUMENT] = json!({"type": "string"});
    if tool == Tool::ShowScreen {
        schema["properties"][OPEN] = json!({"type": "boolean"});
    }
    if tool == Tool::CloseView {
        schema["required"] = json!([ARGUMENT]);
    }
    listed
}

/// An RPC refusal of a tool call.
fn refused(id: &Value, message: &str) -> Reply {
    Reply::new(failure(id, INVALID_PARAMS, message))
}

/// Mark a result as giving this client no control of its own.
fn without_control(result: &mut Value, guard_app_controls: bool) {
    let facts = &mut result["structuredContent"];
    facts["canControl"] = json!(false);
    facts["canResume"] = json!(false);
    facts["control"] = json!(false);
    facts["guardAppControls"] = json!(guard_app_controls);
}

pub(super) struct Reply {
    pub(super) value: Value,
    // Kept until the response has been written (or its task unwinds).
    reservation: Option<Reservation>,
}

impl Reply {
    pub(super) fn new(value: Value) -> Self {
        Self {
            value,
            reservation: None,
        }
    }

    pub(super) fn delivered(&mut self) {
        if let Some(reservation) = &mut self.reservation {
            reservation.settled = true;
        }
    }
}

/// What a context's calls and its cleanup share outside the registry's lock.
#[derive(Default)]
struct Usage {
    active: AtomicUsize,
    // A timed-out acquire may have reached the viewer without returning a lease.
    // Never infer acknowledgement from held=None or discard this context on TTL.
    uncertain: AtomicBool,
    cleaning: AtomicBool,
}

struct Entry {
    token: String,
    child: Arc<Server>,
    used: Instant,
    closing: bool,
    usage: Arc<Usage>,
}

impl Entry {
    /// A new context for a view of `root`.
    fn new(root: &Server, used: Instant) -> Self {
        let mut child = Server::new(
            root.client.clone(),
            root.config.clone(),
            root.port,
            root.read_only,
            root.host_session.clone(),
        );
        child.stream = root.stream.clone();
        child.tiles = root.tiles.clone();
        child.picture_tiles = root.picture_tiles.clone();
        child.decoded = root.decoded.clone();
        child.picture_decoded = root.picture_decoded.clone();
        child.views.store(true, Ordering::Relaxed);
        Self {
            token: atlas_core::files::token(),
            child: Arc::new(child),
            used,
            closing: false,
            usage: Arc::default(),
        }
    }
}

struct Cleaning(Arc<Usage>);

impl Drop for Cleaning {
    fn drop(&mut self) {
        self.0.cleaning.store(false, Ordering::SeqCst);
    }
}

struct Reservation {
    usage: Arc<Usage>,
    mutation: bool,
    settled: bool,
}

impl Drop for Reservation {
    fn drop(&mut self) {
        // Publish uncertainty before the last reservation disappears; the
        // reaper must observe both in this order on every architecture.
        if self.mutation && !self.settled {
            self.usage.uncertain.store(true, Ordering::SeqCst);
        }
        self.usage.active.fetch_sub(1, Ordering::SeqCst);
    }
}

#[derive(Default)]
pub(super) struct Registry {
    entries: Mutex<Vec<Entry>>,
}

impl Registry {
    fn locked(&self) -> MutexGuard<'_, Vec<Entry>> {
        self.entries
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub(super) fn is_empty(&self) -> bool {
        self.locked().is_empty()
    }

    fn matches(left: &str, right: &str) -> bool {
        left.len() == atlas_core::files::TOKEN_BYTES * 2
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
        let minting = bootstrap && token.is_none();
        let entry = if minting {
            if entries.len() >= LIMIT {
                return None;
            }
            entries.push(Entry::new(root, now));
            entries.last_mut()?
        } else {
            let token = token?.as_str()?;
            entries
                .iter_mut()
                .find(|entry| Self::matches(&entry.token, token))?
        };
        if entry.closing {
            return None;
        }
        entry.used = now;
        entry.usage.active.fetch_add(1, Ordering::SeqCst);
        Some((
            entry.child.clone(),
            Reservation {
                usage: entry.usage.clone(),
                mutation,
                settled: false,
            },
            minting.then(|| entry.token.clone()),
        ))
    }

    /// Answer a tool call, inside a view's context when it needs one.
    pub(super) async fn call(&self, root: &Server, id: &Value, params: &Value) -> Reply {
        let Some(tool) = root.offered(params["name"].as_str().unwrap_or_default()) else {
            return refused(id, "Unknown tool");
        };
        let arguments = &params["arguments"];
        let token = arguments.get(ARGUMENT);
        let guarded = root.can_control();
        let mutation = tool.policy().1 == Access::Change;
        let bootstrap = tool == Tool::ShowScreen && arguments[OPEN] == true;
        if !guarded || (token.is_none() && !bootstrap && !mutation) {
            if token.is_some() || mutation {
                return refused(id, "This view has no control authority");
            }
            let mut result = root.call(tool, arguments).await;
            without_control(&mut result, guarded);
            return Reply::new(success(id, result));
        }

        let Some((child, reservation, minted)) = self.reserve(root, token, bootstrap, mutation)
        else {
            return refused(
                id,
                "This view's authority is missing, expired or unavailable",
            );
        };
        let result = if tool == Tool::CloseView {
            self.close(&child);
            json!({"content": [], "structuredContent": {"closed": true}})
        } else {
            let outcome = child.run(tool, arguments).await;
            // Only a failure that says so leaves a mutation unknown: a status
            // read failing after an acknowledged effect is presentation.
            let unknown = outcome.as_ref().is_err_and(|error| error.uncertain);
            if mutation && unknown {
                reservation.usage.uncertain.store(true, Ordering::SeqCst);
            }
            let mut result = child.presented(outcome).await;
            if let Some(token) = minted {
                result["_meta"][META] = json!(token);
                without_control(&mut result, true);
            }
            result
        };
        Reply {
            value: success(id, result),
            reservation: Some(reservation),
        }
    }

    /// Mark `child`'s context for the next sweep to end.
    fn close(&self, child: &Arc<Server>) {
        if let Some(entry) = self
            .locked()
            .iter_mut()
            .find(|entry| Arc::ptr_eq(&entry.child, child))
        {
            entry.closing = true;
        }
    }

    /// End every context, sweeping again while cleanup is unconfirmed.
    pub(super) async fn shut_down(&self) -> Result<()> {
        let mut cause = None;
        for _ in 0..3 {
            cause = self.sweep(true).await.or(cause);
            if self.is_empty() {
                return Ok(());
            }
        }
        let unconfirmed = Error::new(
            Code::Menubar,
            "A view's cleanup is unconfirmed; Atlas retains pending native work for recovery",
        );
        Err(match cause {
            Some(cause) => unconfirmed.with_source(cause),
            None => unconfirmed,
        })
    }

    /// Clean up every context that is closing and idle. Why the last cleanup
    /// failed, when one did: the next sweep tries it again.
    pub(super) async fn sweep(&self, shutdown: bool) -> Option<Error> {
        let candidates = {
            let mut entries = self.locked();
            entries
                .iter_mut()
                .filter_map(|entry| {
                    if shutdown || entry.used.elapsed() >= IDLE {
                        entry.closing = true;
                    }
                    let usage = &entry.usage;
                    if !entry.closing
                        || usage.cleaning.load(Ordering::SeqCst)
                        || usage.active.load(Ordering::SeqCst) != 0
                    {
                        return None;
                    }
                    usage.cleaning.store(true, Ordering::SeqCst);
                    Some((entry.child.clone(), Cleaning(usage.clone())))
                })
                .collect::<Vec<_>>()
        };
        let mut failure = None;
        for (child, _cleaning) in candidates {
            // give_back retains a refused lease. A missing held lease alone
            // does not settle an unknown acquire; sticky uncertainty stays.
            if child.held.lock().await.is_some() {
                // Failure leaves the lease or sticky uncertainty for the next sweep.
                failure = child.give_back().await.err().or(failure);
            }
            let held = child.held.lock().await.is_some();
            self.locked().retain(|entry| {
                !Arc::ptr_eq(&entry.child, &child)
                    || held
                    || entry.usage.uncertain.load(Ordering::SeqCst)
            });
        }
        failure
    }
}

#[cfg(test)]
#[path = "guarded_tests.rs"]
mod tests;

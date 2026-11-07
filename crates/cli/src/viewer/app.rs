//! The viewer as the menu bar app's own: serving it until the person quits,
//! opening its viewer window, and rendering its panel for visual checks.
use super::{
    MENUBAR_RECORD, Viewer,
    http::{CONNECTIONS, accept, bind},
    local::{address, post_local, running},
    settings::remove_record,
    unanswered, unpoisoned,
};
use crate::menubar::{App, Line};
use atlas_client::client::Client;
use atlas_core::{
    error::{Code, Error, Result},
    files,
};
use serde_json::{Value, json};
use std::{path::Path, sync::Arc, time::Duration};

impl Viewer {
    /// Pass `line` to the app this viewer serves.
    pub(super) fn send(&self, line: Line) -> Result<()> {
        unpoisoned(&self.app)
            .as_ref()
            .ok_or_else(|| Error::new(Code::Menubar, "No menu bar is attached to this viewer"))?
            .send(line)
            .map_err(|_| Error::new(Code::Menubar, "The menu bar is not running"))
    }
}

/// Serve the viewer for the menu bar app until the person quits it, or
/// until interrupted, terminated or hung up: nothing to report then. When
/// the menu bar already runs on `port`, it says so instead.
/// `ready` receives the redacted startup record after the app launches and its
/// private record is saved, before accepting requests. Binding, launching or
/// recording failures are returned without reporting readiness.
///
/// # Errors
///
/// Returns configuration, listener, app-launch or service failures. Disconnect and shutdown
/// cleanup retain unknown input outcomes and never acknowledge them from a timeout.
pub async fn serve(
    client: Client,
    port: u16,
    program: crate::menubar::Program,
    ready: impl FnOnce(&Value),
) -> Result<Value> {
    use tokio::signal::unix::{SignalKind, signal};

    let output = client.output_dir()?.to_path_buf();
    let path = output.join(MENUBAR_RECORD);

    let (listener, viewer) = match bind(client, port, Some(&program.config)).await {
        Ok(bound) => bound,
        Err(error) if error.code == Code::PortInUse => {
            return blocking(move || already_running(&output, port, error)).await;
        }
        Err(error) => return Err(error),
    };

    // Stopping it any of these ways releases control and removes its record.
    let mut terminated = signal(SignalKind::terminate())?;
    let mut hung_up = signal(SignalKind::hangup())?;

    let (lines, receiver) = tokio::sync::mpsc::unbounded_channel();
    *unpoisoned(&viewer.app) = Some(lines);

    // The frame service lives as long as this viewer and follows its helper.
    // Without it the app reads pictures over this viewer's address as before.
    let (pipe_stop, pipe) = keep_pipe(viewer.clone());

    // The record names a viewer with an app; none is left when the app
    // does not start.
    let mut app = App::launch(&program, &viewer.url, receiver).await?;
    let record = json!({
        "url": viewer.url,
        "pid": std::process::id(),
        "startedAt": atlas_core::time::timestamp().to_string(),
    });
    files::write_private(&path, &serde_json::to_vec_pretty(&record)?)?;
    *unpoisoned(&viewer.record) = Some((path, record.clone()));

    // The URL is a bearer capability: it stays in the private record and
    // with the app, never in output.
    let mut shown = record.clone();
    shown["url"] = json!("held by the menu bar");

    ready(&shown);

    let expiry = expire_control(viewer.clone());

    // Changes in progress in the helper (taking control, input, recovery)
    // end first, however long the helper takes, and no more begin; then
    // control ends. Only then are open connections given a few seconds.
    let shutdown_view = viewer.clone();
    accept(listener, viewer.clone(), CONNECTIONS, async move {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = terminated.recv() => {}
            _ = hung_up.recv() => {}
            () = app.exited() => {}
        }

        shutdown_view.close().await;
        let _ = shutdown_view.release(None).await;
    })
    .await;

    expiry.abort();
    viewer.close().await;
    let _ = viewer.release(None).await;
    let _ = pipe_stop.send(());
    let _ = pipe.await;

    // The record goes with this viewer, unless another has replaced it.
    if let Some((path, _)) = unpoisoned(&viewer.record).take() {
        remove_record(&path);
    }

    Ok(Value::Null)
}

/// Keep a frame service for the helper `viewer` serves now, until told to
/// stop: when its settings name another helper, the old service ends, its
/// committed pictures are forgotten and a new one starts. A service that
/// cannot start is tried again each second.
fn keep_pipe(
    viewer: Arc<Viewer>,
) -> (
    tokio::sync::oneshot::Sender<()>,
    tokio::task::JoinHandle<()>,
) {
    let (stop, mut stopped) = tokio::sync::oneshot::channel::<()>();
    let task = tokio::spawn(async move {
        let mut running: Option<(Arc<Client>, crate::frame_pipe::service::Service)> = None;
        let mut tick = tokio::time::interval(Duration::from_secs(1));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                _ = &mut stopped => break,
                _ = tick.tick() => {}
            }
            let client = viewer.client();
            let same = |serving: &Arc<Client>| {
                serving.transport.peer_uid == client.transport.peer_uid
                    && serving.transport.path == client.transport.path
            };
            if running.as_ref().is_some_and(|(serving, _)| same(serving)) {
                continue;
            }
            if let Some((_, old)) = running.take() {
                unpoisoned(&viewer.pipe).take();
                // Stopped first, so no late commit of the old helper's
                // pictures is kept after they are forgotten.
                old.stop().await;
                viewer.forget_committed();
            }
            running = start_pipe(&viewer, client.clone())
                .await
                .map(|service| (client, service));
        }
        unpoisoned(&viewer.pipe).take();
        if let Some((_, service)) = running {
            service.stop().await;
        }
    });
    (stop, task)
}

/// Start the frame service for `client`'s helper, its committed pictures
/// kept for input, and offer it on `viewer`'s address; none when it cannot
/// start, as when another service already serves this helper.
async fn start_pipe(
    viewer: &Arc<Viewer>,
    client: Arc<Client>,
) -> Option<crate::frame_pipe::service::Service> {
    let held = Arc::downgrade(viewer);
    let witness: crate::frame_pipe::broker::Witness = Arc::new(move |key, epoch, at| {
        if let Some(viewer) = held.upgrade() {
            viewer.remember_committed(key, epoch, at);
        }
    });
    let service = crate::frame_pipe::service::start(
        client,
        viewer.frame_width,
        witness,
        viewer.input_sent.clone(),
    )
    .await
    .ok()?;
    *unpoisoned(&viewer.pipe) = Some(service.descriptor().clone());
    Some(service)
}

impl Viewer {
    /// End control whose lease ran out. False once shutdown began, whose own
    /// release ends control instead.
    pub(super) async fn end_expired(&self) -> bool {
        // Ending expired control is a change like any other.
        let Some(_changing) = self.changing().await else {
            return false;
        };

        let expired = {
            let mut lease = self.lease.lock().await;
            lease.expire();
            lease.expired()
        };
        if expired {
            let _ = self.release(None).await;
        }
        true
    }
}

/// End control whose lease ran out, in a task of its own, until shutdown
/// begins.
fn expire_control(viewer: Arc<Viewer>) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_millis(100)).await;
            if !viewer.end_expired().await {
                break;
            }
        }
    })
}

/// Run `work`, which waits on the local viewer's socket (for seconds at
/// most), off the async runtime.
async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T> + Send + 'static,
) -> Result<T> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|error| unanswered().with_source(error))?
}

/// The port is taken. When the menu bar holds it, that succeeds: it is
/// already running, as its process.
fn already_running(output: &Path, port: u16, taken: Error) -> Result<Value> {
    let Some(bar) = running(&output.join(MENUBAR_RECORD), port) else {
        return Err(taken);
    };

    Ok(json!({"ok": true, "alreadyRunning": true, "running": true, "pid": bar["pid"]}))
}

/// The private address of the menu bar's viewer, starting the menu bar
/// first when it is not running.
pub(crate) async fn launched_url(config: &Path, port: u16, output: &Path) -> Result<String> {
    let (_, record) = crate::menubar::launch(config, port, output).await?;
    address(&record)
        .map(str::to_owned)
        .ok_or_else(|| Error::new(Code::Menubar, "The menu bar's record names no viewer"))
}

/// Open the viewer window in the menu bar app, starting the menu bar first
/// when it is not running. It prints nothing.
pub(crate) async fn open_viewer(config: &Path, port: u16, output: &Path) -> Result<()> {
    let url = launched_url(config, port, output).await?;
    blocking(move || post_local(&url, "app/viewer", &json!({}))).await?;
    Ok(())
}

/// [`open_viewer`], saying that the viewer is open.
///
/// # Errors
///
/// Returns viewer configuration, launch or readiness failures. A failed launch can leave
/// owned work requiring cleanup.
pub async fn monitor(config: &Path, port: u16, output: &Path) -> Result<Value> {
    open_viewer(config, port, output).await?;
    Ok(json!({"ok": true, "viewer": "open"}))
}

/// Render the menu bar panel, fed by a private viewer on a free port, in the
/// light and dark appearances. For visual checks; it never takes control.
///
/// # Errors
///
/// Returns viewer, app-launch, timeout or rendering failures. A successful process exit
/// alone does not prove a later interactive viewer works.
pub async fn render_menubar(
    client: Client,
    program: &crate::menubar::Program,
    directory: &Path,
) -> Result<Value> {
    let (listener, viewer) = bind(client, 0, None).await?;
    let server = tokio::spawn(accept(
        listener,
        viewer.clone(),
        CONNECTIONS,
        std::future::pending(),
    ));
    let mut paths = Vec::new();
    let mut outcome = Ok(());

    for (name, dark) in [("light", false), ("dark", true)] {
        let path = directory.join(format!("menubar-{name}.png"));
        outcome = crate::menubar::render(program, &viewer.url, &path, dark).await;
        if outcome.is_err() {
            break;
        }

        paths.push(path);
    }

    server.abort();
    let _ = viewer.release(None).await;
    outcome?;

    Ok(json!({"ok": true, "paths": paths}))
}

//! The viewer as the menu bar app's own: serving it until the person quits,
//! opening its viewer window, and rendering its panel for visual checks.
use super::{
    MENUBAR_RECORD, Viewer,
    http::{CONNECTIONS, accept, bind},
    local::{post_local, running},
    settings::remove_record,
    unanswered, unpoisoned,
};
use crate::{
    client::Client,
    error::{Code, Error, Result},
    files,
    menubar::{App, Line},
};
use serde_json::{Value, json};
use std::{path::Path, time::Duration};

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
pub async fn serve(client: Client, port: u16, program: crate::menubar::Program) -> Result<Value> {
    use tokio::signal::unix::{SignalKind, signal};

    let output = client.output_dir()?.to_path_buf();
    let path = output.join(MENUBAR_RECORD);

    let (listener, viewer) = match bind(client, port, Some(&program.config)).await {
        Ok(bound) => bound,
        Err(error) if error.code == Code::PortInUse => {
            let output = output.clone();
            return blocking(move || already_running(&output, port, error)).await;
        }
        Err(error) => return Err(error),
    };

    // Stopping it any of these ways releases control and removes its record.
    let mut terminated = signal(SignalKind::terminate())?;
    let mut hung_up = signal(SignalKind::hangup())?;

    let (lines, receiver) = tokio::sync::mpsc::unbounded_channel();
    *unpoisoned(&viewer.app) = Some(lines);

    // The record names a viewer with an app; none is left when the app
    // does not start.
    let mut app = App::launch(&program, &viewer.url, receiver).await?;
    let record = json!({
        "url": viewer.url,
        "pid": std::process::id(),
        "startedAt": crate::client::timestamp().to_string(),
    });
    files::write_private(&path, &serde_json::to_vec_pretty(&record)?)?;
    *unpoisoned(&viewer.record) = Some((path, record.clone()));

    // The URL is a bearer capability: it stays in the private record and
    // with the app, never in output.
    let mut shown = record.clone();
    shown["url"] = json!("held by the menu bar");

    if crate::ui::human() {
        println!("The menu bar is running. Quit it from its menu, or press Ctrl-C here.");
    } else {
        println!("{shown}");
    }

    let timer_view = viewer.clone();
    let expiry = tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_millis(100)).await;

            // Ending expired control is a change like any other; once
            // shutdown began, its own release ends control.
            let Some(_changing) = timer_view.changing().await else {
                break;
            };

            let expired = {
                let mut lease = timer_view.lease.lock().await;
                lease.expire();
                lease.expired()
            };
            if expired {
                let _ = timer_view.release(None).await;
            }
        }
    });

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

    // The record goes with this viewer, unless another has replaced it.
    if let Some((path, _)) = unpoisoned(&viewer.record).take() {
        remove_record(&path);
    }

    Ok(Value::Null)
}

/// Run `work`, which waits on the local viewer's socket (for seconds at
/// most), off the async runtime.
async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T> + Send + 'static,
) -> Result<T> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|_| unanswered())?
}

/// The port is taken. When the menu bar holds it, that succeeds: it is
/// already running, as its process.
fn already_running(output: &Path, port: u16, taken: Error) -> Result<Value> {
    let Some(bar) = running(&output.join(MENUBAR_RECORD), port) else {
        return Err(taken);
    };

    Ok(json!({"ok": true, "alreadyRunning": true, "running": true, "pid": bar["pid"]}))
}

/// Open the viewer window in the menu bar app, starting the menu bar first
/// when it is not running. It prints nothing.
pub(crate) async fn open_viewer(config: &Path, port: u16, output: &Path) -> Result<()> {
    let (_, record) = crate::menubar::launch(config, port, output).await?;
    let url = record["url"]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| Error::new(Code::Menubar, "The menu bar's record names no viewer"))?;
    blocking(move || post_local(&url, "app/viewer", &json!({}))).await?;
    Ok(())
}

/// [`open_viewer`], saying that the viewer is open.
pub async fn monitor(config: &Path, port: u16, output: &Path) -> Result<Value> {
    open_viewer(config, port, output).await?;
    Ok(json!({"ok": true, "viewer": "open"}))
}

/// Render the menu bar panel, fed by a private viewer on a free port, in the
/// light and dark appearances. For visual checks; it never takes control.
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

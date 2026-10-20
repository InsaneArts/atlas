//! The helper daemon's socket: one request per connection, answered with
//! protocol v1 replies.
use crate::{
    daemon::{
        facts,
        protocol::{self, MAX_REQUEST, Request, failure},
        run,
    },
    error::{Error, Result},
    native::{apps, capture, endpoint, windows},
    session::validate_target_uid,
};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use std::{path::PathBuf, sync::Arc, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWrite, AsyncWriteExt},
    net::UnixStream,
    signal::unix::{SignalKind, signal},
    sync::{Notify, Semaphore, mpsc, watch},
    task::JoinSet,
};

/// Connections served at once.
const CONNECTIONS: usize = 32;

/// Longest a single reply write may take; a client that stops reading is
/// dropped rather than holding a connection slot.
const WRITE: Duration = Duration::from_secs(10);

/// Longest the daemon waits for running commands to end when it stops.
const STOP: Duration = Duration::from_secs(6);

/// Longest the runtime waits for blocking work once serving has ended.
const SHUTDOWN: Duration = Duration::from_secs(2);

/// How often a running command checks that the helper is still off screen.
const CONSOLE_CHECK: Duration = Duration::from_secs(1);

/// Takes a screenshot: the real display in the daemon, a synthetic image in
/// tests, so ordinary tests never capture the desktop they run on.
type Capture = fn(Option<u32>) -> Result<capture::Shot>;

struct Daemon {
    /// launchd's session type, read once at start.
    manager: String,
    capture: Capture,
    /// Whether the helper's session is on the console right now.
    on_console: fn() -> bool,
    restarting: Notify,
    /// Set when the daemon stops, so running commands are ended first.
    stopping: watch::Sender<bool>,
}

impl Daemon {
    fn new(manager: String, capture: Capture, on_console: fn() -> bool) -> Self {
        Self {
            manager,
            capture,
            on_console,
            restarting: Notify::new(),
            stopping: watch::Sender::new(false),
        }
    }

    /// Completes once the daemon has begun to stop.
    async fn stopped(&self) {
        let _ = self
            .stopping
            .subscribe()
            .wait_for(|stopping| *stopping)
            .await;
    }

    /// Completes once the helper is on the console, checked about once a
    /// second.
    async fn on_screen(&self) {
        loop {
            tokio::time::sleep(CONSOLE_CHECK).await;

            if (self.on_console)() {
                return;
            }
        }
    }
}

/// Refuse to start outside the helper's own logged-in graphical session.
fn require_graphical_session(uid: u32, manager: &str) -> Result<()> {
    let proof = crate::native::session_info()?;
    let refused = |why: &str| {
        Error::new(
            "identity",
            format!("The helper daemon must run in the helper's graphical login: {why}"),
        )
    };

    if manager != "Aqua" {
        return Err(refused(&format!("launchd session type is {manager}")));
    }

    match facts::session_owner() {
        Some((owner, true)) if owner == i64::from(uid) => {}
        Some((_, true)) => return Err(refused("the session belongs to another user")),
        Some((_, false)) => return Err(refused("the login has not finished")),
        None => return Err(refused("the session's owner is unknown")),
    }

    if proof["hasGraphicAccess"] != true {
        return Err(refused("the session has no graphics access"));
    }

    Ok(())
}

/// Serve protocol v1 on `socket` as the helper `uid`, for the controller
/// `controller_uid`. Returns when stopped; a `restart` request ends it with
/// an error so launchd starts a fresh daemon.
pub async fn serve(uid: u32, controller_uid: u32, socket: PathBuf) -> Result<()> {
    validate_target_uid(uid, controller_uid)?;

    if nix::unistd::getuid().as_raw() != uid {
        return Err(Error::new(
            "identity",
            "The helper daemon must run as the configured helper account",
        ));
    }

    let manager = facts::manager_name();
    require_graphical_session(uid, &manager)?;
    facts::type_exactly();

    let daemon = Arc::new(Daemon::new(
        manager,
        capture::main_display,
        facts::on_console,
    ));

    let (listener, _endpoint) = endpoint::bind(
        &socket,
        uid,
        "helper daemon",
        endpoint::Directory::Private {
            controller: controller_uid,
        },
    )
    .await?;

    // A Screen Recording grant reaches this process only through a restart.
    let restarter = daemon.clone();
    let consent = tokio::spawn(facts::request_consent_on_arrival(
        controller_uid,
        move || {
            restarter.restarting.notify_one();
        },
    ));

    let slots = Arc::new(Semaphore::new(CONNECTIONS));
    let mut tasks = JoinSet::new();

    // Both listeners exist before the loop, so a signal that arrives between
    // two passes is still delivered.
    let mut interrupt = signal(SignalKind::interrupt())?;
    let mut terminate = signal(SignalKind::terminate())?;

    let outcome = loop {
        tokio::select! {
            // Stopped from outside (macOS's Quit & Reopen after a new
            // permission, say): exit unsuccessfully so launchd starts it
            // again. Uninstalling unloads the job first, so it stays down.
            _ = interrupt.recv() => {
                break Err(Error::new("restart", "Interrupted; launchd starts it again"));
            }
            _ = terminate.recv() => {
                break Err(Error::new("restart", "Stopped; launchd starts it again"));
            }
            () = daemon.restarting.notified() => {
                break Err(Error::new("restart", "Restarting at the controller's request"));
            }
            Some(_) = tasks.join_next(), if !tasks.is_empty() => {}
            accepted = listener.accept() => {
                let Ok((stream, _)) = accepted else {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                    continue;
                };

                let Ok(peer) = stream.peer_cred() else {
                    continue;
                };
                if peer.uid() != controller_uid && peer.uid() != uid {
                    continue;
                }

                let Ok(slot) = slots.clone().try_acquire_owned() else {
                    // Nothing ran: say so, rather than hang up in a way a
                    // client must treat as uncertain.
                    endpoint::turn_away(stream, &failure(
                        "busy",
                        "The helper is serving too many requests; try again",
                    ));
                    continue;
                };

                let daemon = daemon.clone();
                let controller = peer.uid() == controller_uid;
                tasks.spawn(async move {
                    let _slot = slot;
                    let _ = handle(stream, daemon, controller).await;
                });
            }
        }
    };

    // Running commands are ended (TERM, then KILL) before the daemon exits,
    // so a restart never leaves one unsupervised.
    daemon.stopping.send_replace(true);
    let drained =
        tokio::time::timeout(STOP, async { while tasks.join_next().await.is_some() {} }).await;
    if drained.is_err() {
        tasks.abort_all();
        while tasks.join_next().await.is_some() {}
    }

    consent.abort();
    outcome
}

/// Serve on a Tokio runtime in a worker thread while this (main) thread runs
/// the Core Foundation run loop, which AppKit needs to keep its view of
/// running applications current.
pub fn serve_blocking(uid: u32, controller_uid: u32, socket: PathBuf) -> Result<()> {
    use objc2_core_foundation::{CFRunLoop, CFString};

    // Run-loop modes are compared by name; this is kCFRunLoopDefaultMode.
    let default_mode = CFString::from_static_str("kCFRunLoopDefaultMode");
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;

    let worker = std::thread::spawn(move || {
        let outcome = runtime.block_on(serve(uid, controller_uid, socket));
        // Dropping the runtime would wait for blocking work (a capture or
        // app listing stuck in a framework call) without limit.
        runtime.shutdown_timeout(SHUTDOWN);
        outcome
    });

    while !worker.is_finished() {
        CFRunLoop::run_in_mode(Some(&default_mode), 0.25, false);
    }

    worker
        .join()
        .map_err(|_| Error::new("daemon", "The helper daemon stopped unexpectedly"))?
}

/// Write one reply line, bounded in time.
async fn write<W: AsyncWrite + Unpin>(stream: &mut W, reply: &Value) -> Result<()> {
    let mut line = serde_json::to_vec(reply)?;
    line.push(b'\n');

    tokio::time::timeout(WRITE, async {
        stream.write_all(&line).await?;
        stream.flush().await
    })
    .await
    .map_err(|_| Error::new("transport", "The client stopped reading"))??;

    Ok(())
}

/// An error as a protocol reply.
fn refused(error: Error) -> Value {
    failure(error.code, error.error)
}

/// Whether `request` may run in the session's current state: commands and
/// captures never run while the helper is on the console, where they would
/// act on a desktop a person is using.
fn allowed(request: &Request, on_console: bool) -> Option<Value> {
    let acts = matches!(
        request,
        Request::Run { .. } | Request::Screenshot { .. } | Request::Logout { .. }
    );

    (acts && on_console).then(|| {
        failure(
            "on-console",
            "The helper is on the screen; switch back to your own account first",
        )
    })
}

/// Ask macOS to log this login out, as its Apple menu's Log Out does but
/// without the confirmation: apps quit as at any log-out, and one may hold
/// it up (asking to save). loginwindow answers at once; only its error is a
/// refusal. A request that never got an answer may still be carried out.
async fn ask_to_log_out() -> Value {
    let asked = tokio::process::Command::new("/usr/bin/osascript")
        .args(["-e", "tell application \"loginwindow\" to «event aevtrlgo»"])
        .stdin(std::process::Stdio::null())
        .kill_on_drop(true)
        .output();

    let unanswered = |why: &str| {
        let mut reply = failure(
            "logout",
            format!("macOS did not answer the log-out request ({why})"),
        );
        reply["uncertain"] = json!(true);
        reply
    };

    match tokio::time::timeout(Duration::from_secs(5), asked).await {
        Ok(Ok(output)) if output.status.success() => json!({"ok": true, "op": "logout"}),
        // Stopped by a signal: the log-out itself may have ended it.
        Ok(Ok(output)) if output.status.code().is_none() => unanswered("it was stopped"),
        Ok(Ok(output)) => {
            eprintln!(
                "osascript refused the log-out: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
            failure("logout", "macOS refused to log the helper out")
        }
        Ok(Err(_)) => failure("logout", "The log-out request could not start"),
        Err(_) => unanswered("it may be asking in the helper"),
    }
}

/// End this login at once, without asking its apps. Its process goes with
/// the session, so how it went is only logged.
fn force_log_out() {
    let uid = nix::unistd::getuid().as_raw();
    let started = std::process::Command::new("/bin/launchctl")
        .args(["bootout", &format!("gui/{uid}")])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .spawn();
    match started {
        Ok(mut child) => {
            std::thread::spawn(move || {
                if let Ok(status) = child.wait()
                    && !status.success()
                {
                    eprintln!("launchctl bootout gui/{uid} failed: {status}");
                }
            });
        }
        Err(error) => eprintln!("launchctl bootout gui/{uid} did not start: {error}"),
    }
}

/// Windows that lie entirely outside the main display.
fn off_display_windows() -> Vec<Value> {
    let (x, y, width, height) = facts::display_bounds();
    windows::windows()
        .unwrap_or_default()
        .into_iter()
        .filter(|window| window.layer == 0)
        .filter(|window| {
            let b = &window.bounds;
            b.x + b.width <= x || b.y + b.height <= y || b.x >= x + width || b.y >= y + height
        })
        .map(
            |window| json!({"number": window.number, "owner": window.owner, "title": window.title}),
        )
        .collect()
}

async fn screenshot(daemon: &Daemon, max_dimension: Option<u32>) -> Value {
    let capture = daemon.capture;
    let taken = tokio::task::spawn_blocking(move || {
        capture(max_dimension).map(|shot| (shot, off_display_windows()))
    })
    .await;
    match taken {
        Ok(Ok((shot, off_display))) => json!({
            "ok": true,
            "png": STANDARD.encode(&shot.png),
            "width": shot.width,
            "height": shot.height,
            "scale": shot.scale,
            "offDisplayWindows": off_display,
        }),
        Ok(Err(error)) => refused(error),
        Err(_) => failure("capture", "The capture failed"),
    }
}

/// Serve one request. `controller` says whether it came from the controller:
/// the helper's own processes (an app under test among them) may only say
/// hello, as the setup window does there, never act with Atlas's
/// permissions.
async fn handle(stream: UnixStream, daemon: Arc<Daemon>, controller: bool) -> Result<()> {
    let (mut reader, mut writer) = stream.into_split();
    let request = match endpoint::read_line(&mut reader, MAX_REQUEST).await {
        Ok(line) => protocol::parse(&line),
        Err(error) => Err(refused(error)),
    };

    let request = request.and_then(|request| {
        if controller || matches!(request, Request::Hello) {
            Ok(request)
        } else {
            Err(failure(
                "permission",
                "Only the account Atlas works for may ask the helper that",
            ))
        }
    });

    let request = request.and_then(|request| match allowed(&request, (daemon.on_console)()) {
        Some(refusal) => Err(refusal),
        None => Ok(request),
    });
    let reply = match request {
        Err(reply) => reply,
        Ok(Request::Hello) => facts::hello(&daemon.manager),
        Ok(Request::Apps) => match tokio::task::spawn_blocking(apps::running).await {
            Ok(apps) => json!({"ok": true, "apps": apps}),
            Err(_) => failure("apps", "Running applications could not be listed"),
        },
        Ok(Request::Screenshot { max_dimension }) => screenshot(&daemon, max_dimension).await,
        Ok(Request::Run { argv, timeout }) => {
            // Events reach the client through their own writer, so a slow
            // client never stalls supervision of the command.
            let (events, mut queue) = mpsc::channel(run::EVENTS);
            let delivery = tokio::spawn(async move {
                while let Some(event) = queue.recv().await {
                    write(&mut writer, &event).await?;
                }

                Ok::<_, Error>(writer)
            });

            // The client never sends after its request, so any read result
            // means it left. A command never goes on while a person may be
            // using the helper's desktop.
            let cancel = async {
                let mut byte = [0; 1];
                tokio::select! {
                    _ = reader.read(&mut byte) => run::Cancel::Left,
                    () = daemon.stopped() => run::Cancel::Stopping,
                    () = daemon.on_screen() => run::Cancel::OnConsole,
                }
            };

            let reply = run::run(&argv, timeout, events, cancel).await;
            let mut writer = delivery
                .await
                .map_err(|_| Error::new("transport", "The event writer failed"))??;

            return write(&mut writer, &reply).await;
        }
        Ok(Request::Restart) => {
            let written = write(&mut writer, &json!({"ok": true, "op": "restart"})).await;
            daemon.restarting.notify_one();
            return written;
        }
        // Answered before the log-out ends this process with the session.
        Ok(Request::Logout { force: true }) => {
            let written = write(&mut writer, &json!({"ok": true, "op": "logout"})).await;
            force_log_out();
            return written;
        }
        Ok(Request::Logout { force: false }) => ask_to_log_out().await,
    };

    write(&mut writer, &reply).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::{Transport, run_output};
    use tokio::io::{AsyncBufReadExt, BufReader};

    /// The run loop mode is passed by name; it is the default mode.
    #[test]
    #[allow(unsafe_code)]
    fn the_run_loop_mode_is_the_default_mode() {
        // SAFETY: reading an immutable framework constant in a test.
        let mode = unsafe { objc2_core_foundation::kCFRunLoopDefaultMode }.unwrap();
        assert_eq!(mode.to_string(), "kCFRunLoopDefaultMode");
    }

    fn synthetic(_: Option<u32>) -> Result<capture::Shot> {
        Ok(capture::Shot {
            png: b"\x89PNG synthetic".to_vec(),
            width: 64,
            height: 36,
            scale: 1.0,
        })
    }

    fn daemon(on_console: bool) -> Arc<Daemon> {
        Arc::new(Daemon::new(
            "Aqua".into(),
            synthetic,
            if on_console { || true } else { || false },
        ))
    }

    /// Send one request line over a socket pair and collect every reply line.
    async fn exchange(daemon: Arc<Daemon>, request: &str) -> Vec<Value> {
        exchange_as(daemon, request, true).await
    }

    /// [`exchange`] from the controller, or from the helper's own account.
    async fn exchange_as(daemon: Arc<Daemon>, request: &str, controller: bool) -> Vec<Value> {
        let (mut client, server) = UnixStream::pair().unwrap();
        let served = tokio::spawn(handle(server, daemon, controller));

        client
            .write_all(format!("{request}\n").as_bytes())
            .await
            .unwrap();

        let mut lines = BufReader::new(client).lines();
        let mut replies = Vec::new();

        while let Some(line) = lines.next_line().await.unwrap() {
            let value: Value = serde_json::from_str(&line).unwrap();
            let last = value.get("ok").is_some();
            replies.push(value);
            if last {
                break;
            }
        }

        served.await.unwrap().unwrap();
        replies
    }

    #[tokio::test]
    async fn the_helpers_own_processes_may_only_say_hello() {
        let hello = exchange_as(daemon(false), r#"{"op":"hello"}"#, false).await;
        assert_eq!(hello[0]["op"], "hello");

        for request in [
            r#"{"op":"run","argv":["/bin/echo","hi"]}"#,
            r#"{"op":"screenshot"}"#,
            r#"{"op":"apps"}"#,
            r#"{"op":"restart"}"#,
            r#"{"op":"logout","force":true}"#,
        ] {
            let reply = &exchange_as(daemon(false), request, false).await[0];
            assert_eq!(reply["code"], "permission", "{request}");
            assert_eq!(reply["uncertain"], false, "{request}");
        }
    }

    #[tokio::test]
    async fn requests_are_answered_on_the_wire() {
        let hello = exchange(daemon(false), r#"{"op":"hello"}"#).await;
        assert_eq!(hello[0]["op"], "hello");
        assert_eq!(hello[0]["session"]["managerName"], "Aqua");

        let refused = exchange(daemon(false), r#"{"op":"nope"}"#).await;
        assert_eq!(refused[0]["error"], "unknown op 'nope'");

        let ran = exchange(
            daemon(false),
            r#"{"op":"run","argv":["/bin/echo","hi"],"timeoutMs":3000}"#,
        )
        .await;
        assert_eq!(ran[0]["event"], "started");
        assert_eq!(ran[1], json!({"event": "output", "data": "aGkK"}));
        assert_eq!(ran[2]["exitCode"], 0);

        let shot = exchange(daemon(false), r#"{"op":"screenshot","maxDimension":64}"#).await;
        assert_eq!(
            (&shot[0]["width"], &shot[0]["height"]),
            (&json!(64), &json!(36))
        );
        assert!(shot[0]["offDisplayWindows"].is_array());

        let apps = exchange(daemon(false), r#"{"op":"apps"}"#).await;
        assert!(
            apps[0]["apps"]
                .as_array()
                .is_some_and(|apps| !apps.is_empty())
        );
    }

    #[tokio::test]
    async fn nothing_acts_while_the_helper_is_on_screen() {
        for request in [
            r#"{"op":"run","argv":["/bin/echo","hi"]}"#,
            r#"{"op":"screenshot"}"#,
        ] {
            let reply = exchange(daemon(true), request).await;
            assert_eq!(reply.len(), 1, "{request}");
            assert_eq!(reply[0]["code"], "on-console", "{request}");
        }

        let hello = exchange(daemon(true), r#"{"op":"hello"}"#).await;
        assert_eq!(hello[0]["ok"], true, "facts stay available");
    }

    /// The reply shapes clients and the live checks read.
    #[tokio::test]
    async fn replies_keep_their_shapes() {
        let keys = |value: &Value| {
            let mut keys: Vec<_> = value.as_object().unwrap().keys().cloned().collect();
            keys.sort();
            keys
        };

        let hello = &exchange(daemon(false), r#"{"op":"hello"}"#).await[0];
        assert_eq!(
            keys(hello),
            [
                "daemon",
                "display",
                "ok",
                "op",
                "permissions",
                "session",
                "user"
            ]
        );
        assert_eq!(keys(&hello["daemon"]), ["pid", "protocol", "version"]);
        assert_eq!(keys(&hello["display"]), ["height", "scale", "width"]);
        assert_eq!(keys(&hello["user"]), ["home", "name", "uid"]);

        let shot = &exchange(daemon(false), r#"{"op":"screenshot"}"#).await[0];
        assert_eq!(
            keys(shot),
            ["height", "offDisplayWindows", "ok", "png", "scale", "width"]
        );

        let ran = exchange(daemon(false), r#"{"op":"run","argv":["/bin/echo"]}"#).await;
        assert_eq!(keys(&ran[0]), ["event", "pid"]);
        assert_eq!(
            keys(ran.last().unwrap()),
            ["durationMs", "exitCode", "ok", "signal", "timedOut"]
        );

        let refusal = &exchange(daemon(false), r#"{"op":"run","argv":[]}"#).await[0];
        assert_eq!(keys(refusal), ["code", "error", "ok", "uncertain"]);

        let apps = &exchange(daemon(false), r#"{"op":"apps"}"#).await[0];
        assert_eq!(
            keys(&apps["apps"][0]),
            [
                "active",
                "bundleId",
                "executable",
                "hidden",
                "name",
                "pid",
                "policy"
            ]
        );
    }

    #[tokio::test]
    async fn restart_answers_then_asks_the_daemon_to_exit() {
        let daemon = daemon(false);
        let notified = daemon.clone();
        let waiting = tokio::spawn(async move { notified.restarting.notified().await });

        tokio::task::yield_now().await;
        let reply = exchange(daemon, r#"{"op":"restart"}"#).await;
        assert_eq!(reply[0], json!({"ok": true, "op": "restart"}));

        tokio::time::timeout(Duration::from_secs(2), waiting)
            .await
            .unwrap()
            .unwrap();
    }

    /// A client that hangs up ends its quiet command; so does a stopping
    /// daemon.
    #[tokio::test]
    async fn quiet_commands_end_when_the_client_or_daemon_leaves() {
        for leave in ["client", "daemon"] {
            let daemon = daemon(false);
            let (mut client, server) = UnixStream::pair().unwrap();
            let served = tokio::spawn(handle(server, daemon.clone(), true));

            client
                .write_all(
                    b"{\"op\":\"run\",\"argv\":[\"/bin/sleep\",\"30\"],\"timeoutMs\":60000}\n",
                )
                .await
                .unwrap();

            let mut lines = BufReader::new(client).lines();
            let started: Value =
                serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();
            let group = started["pid"].as_i64().unwrap() as i32;

            match leave {
                "client" => drop(lines),
                _ => {
                    daemon.stopping.send_replace(true);

                    // The client still listens: it hears that the command
                    // was cut short, with an outcome nobody can know.
                    let reply: Value =
                        serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();
                    assert_eq!(
                        (&reply["code"], &reply["uncertain"]),
                        (&json!("restart"), &json!(true))
                    );
                }
            }

            tokio::time::timeout(Duration::from_secs(8), served)
                .await
                .expect(leave)
                .unwrap()
                .ok();

            let group = nix::unistd::Pid::from_raw(group);
            assert!(nix::sys::signal::killpg(group, None).is_err(), "{leave}");
        }
    }

    /// A running command stops when the helper comes on screen, and the
    /// client hears that its outcome is unknown.
    #[tokio::test]
    async fn a_command_stops_when_the_helper_comes_on_screen() {
        static SHOWN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

        let daemon = Arc::new(Daemon::new("Aqua".into(), synthetic, || {
            SHOWN.load(std::sync::atomic::Ordering::SeqCst)
        }));
        let (mut client, server) = UnixStream::pair().unwrap();
        let served = tokio::spawn(handle(server, daemon, true));

        client
            .write_all(b"{\"op\":\"run\",\"argv\":[\"/bin/sleep\",\"30\"],\"timeoutMs\":60000}\n")
            .await
            .unwrap();

        let mut lines = BufReader::new(client).lines();
        let started: Value =
            serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();
        let group = started["pid"].as_i64().unwrap() as i32;

        SHOWN.store(true, std::sync::atomic::Ordering::SeqCst);

        let line = tokio::time::timeout(Duration::from_secs(8), lines.next_line())
            .await
            .expect("the command was stopped")
            .unwrap()
            .unwrap();
        let reply: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(
            (&reply["code"], &reply["uncertain"]),
            (&json!("on-console"), &json!(true))
        );

        served.await.unwrap().unwrap();

        let group = nix::unistd::Pid::from_raw(group);
        assert!(nix::sys::signal::killpg(group, None).is_err());
    }

    /// The client's own transport reads this daemon's replies.
    #[tokio::test]
    async fn the_client_transport_understands_the_daemon() {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("d.sock");
        let listener = tokio::net::UnixListener::bind(&socket).unwrap();
        let daemon = daemon(false);

        tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                tokio::spawn(handle(stream, daemon.clone(), true));
            }
        });

        let transport = Transport {
            path: socket,
            timeout: Duration::from_secs(10),
            peer_uid: nix::unistd::getuid().as_raw(),
        };

        let reply = transport
            .call(&json!({"op": "run", "argv": ["/bin/sh", "-c", "printf ok; exit 4"], "timeoutMs": 3000}))
            .await
            .unwrap();
        assert_eq!(run_output(&reply, 64).unwrap(), b"ok");
        assert_eq!(
            (&reply["exitCode"], &reply["timedOut"]),
            (&json!(4), &json!(false))
        );
        crate::client::validate_completion(&json!({"op": "run"}), &reply).unwrap();

        let hello = transport.call(&json!({"op": "hello"})).await.unwrap();
        assert_eq!(hello["daemon"]["pid"], std::process::id());
    }
}

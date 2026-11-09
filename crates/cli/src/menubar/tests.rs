use super::*;
use nix::{
    errno::Errno,
    sys::{
        signal::{Signal, kill},
        stat::Mode,
        wait::{WaitPidFlag, WaitStatus, waitpid},
    },
    unistd::{Pid, mkfifo},
};
use std::{
    cell::Cell,
    io::Write,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
};

struct Recording {
    directory: tempfile::TempDir,
    program: Program,
    owned: Cell<Option<atlas_core::process::Instance>>,
}

impl Recording {
    fn new(body: &str) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let executable = directory.path().join("recording child");
        let config = directory.path().join("atlas config.toml");
        fs::write(&config, b"").unwrap();
        fs::write(
            &executable,
            format!(
                r#"#!/bin/sh
set -eu
umask 077
directory=${{0%/*}}
for arg do printf '%s\0' "$arg"; done > "$directory/argv"
printf '%s' "$ATLAS_CLI" > "$directory/cli"
printf '%s' "$ATLAS_CONFIG" > "$directory/config"
printf '%s' "${{XPC_SERVICE_NAME+x}}" > "$directory/xpc"
printf '%s' "$$" > "$directory/pid"
: > "$directory/started"
{body}
"#
            ),
        )
        .unwrap();
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
        for name in ["release", "hold"] {
            mkfifo(&directory.path().join(name), Mode::S_IRUSR | Mode::S_IWUSR).unwrap();
        }
        Self {
            directory,
            program: Program {
                executable,
                config: fs::canonicalize(config).unwrap(),
            },
            owned: Cell::new(None),
        }
    }

    fn reader() -> Self {
        Self::new(
            r#"IFS= read -r url
printf '%s\n' "$url" > "$directory/url"
: > "$directory/ready"
while IFS= read -r line; do printf '%s\n' "$line"; done > "$directory/rest"
: > "$directory/eof""#,
        )
    }

    async fn wait_for(&self, name: &str) {
        tokio::time::timeout(Duration::from_secs(10), async {
            while !self.directory.path().join(name).exists() {
                // Poll a child's explicit signal, never infer readiness from a delay.
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("the recording child must signal within the watchdog");
    }

    async fn started(&self) -> atlas_core::process::Instance {
        self.wait_for("started").await;
        let pid = fs::read_to_string(self.directory.path().join("pid"))
            .unwrap()
            .parse()
            .unwrap();
        let info = atlas_core::process::info(pid).unwrap().unwrap();
        assert!(info.owned_by(nix::unistd::getuid().as_raw()));
        assert_eq!(info.ppid, std::process::id() as i32);
        self.owned.set(Some(info.instance));
        fs::write(
            self.directory.path().join("identity.json"),
            serde_json::to_vec(&info.instance).unwrap(),
        )
        .unwrap();
        info.instance
    }

    async fn release(&self) {
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                match fs::OpenOptions::new()
                    .write(true)
                    .custom_flags(nix::fcntl::OFlag::O_NONBLOCK.bits())
                    .open(self.directory.path().join("release"))
                {
                    Ok(mut writer) => {
                        writer.write_all(b"close\n").unwrap();
                        break;
                    }
                    Err(error) if error.raw_os_error() == Some(Errno::ENXIO as i32) => {
                        tokio::time::sleep(Duration::from_millis(5)).await;
                    }
                    Err(error) => panic!("cannot release the recording child: {error}"),
                }
            }
        })
        .await
        .expect("the child must open its release pipe within the watchdog");
    }

    async fn reaped(&self) {
        let instance = self.owned.get().unwrap();
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                match waitpid(Pid::from_raw(instance.pid), Some(WaitPidFlag::WNOHANG)) {
                    Ok(WaitStatus::StillAlive) => {
                        tokio::time::sleep(Duration::from_millis(5)).await;
                    }
                    Ok(WaitStatus::Exited(..) | WaitStatus::Signaled(..)) => break,
                    // Tokio may reap its dropped child before this kernel wait.
                    Err(Errno::ECHILD) => {
                        assert!(
                            atlas_core::process::info(instance.pid)
                                .unwrap()
                                .is_none_or(|info| info.instance != instance)
                        );
                        break;
                    }
                    status => panic!("unexpected child wait: {status:?}"),
                }
            }
        })
        .await
        .expect("the owned child must be reaped within the watchdog");
        self.owned.set(None);
    }

    fn assert_reaped(&self) {
        let instance = self.owned.get().unwrap();
        assert_eq!(
            waitpid(Pid::from_raw(instance.pid), Some(WaitPidFlag::WNOHANG)),
            Err(Errno::ECHILD),
            "the startup worker must reap its child before releasing exclusion"
        );
        self.owned.set(None);
    }
}

impl Drop for Recording {
    fn drop(&mut self) {
        let Some(instance) = self.owned.get() else {
            return;
        };
        // A failed assertion must not leave a stand-in running or signal a reused PID.
        if atlas_core::process::info(instance.pid)
            .ok()
            .flatten()
            .is_some_and(|info| {
                info.instance == instance
                    && info.ppid == std::process::id() as i32
                    && info.owned_by(nix::unistd::getuid().as_raw())
            })
            && kill(Pid::from_raw(instance.pid), Signal::SIGKILL).is_ok()
        {
            // Tokio may already have reaped it; Drop cannot report that race.
            let _ = waitpid(Pid::from_raw(instance.pid), None);
        }
    }
}

type Operation = Box<dyn FnMut(&str) -> std::io::Result<()> + Send>;
type Probe = Box<dyn FnMut(Option<u32>) -> Option<serde_json::Value> + Send>;

pub(super) struct StartupTest {
    pub command: Option<std::process::Command>,
    pub operation: Operation,
    pub probe: Option<Probe>,
    pub timeout: Duration,
}

struct Gate {
    point: &'static str,
    arrived: Option<tokio::sync::oneshot::Sender<()>>,
    release: std::sync::mpsc::Receiver<()>,
}

impl Gate {
    fn new(
        point: &'static str,
    ) -> (
        Self,
        tokio::sync::oneshot::Receiver<()>,
        std::sync::mpsc::Sender<()>,
    ) {
        let (arrived, arrival) = tokio::sync::oneshot::channel();
        let (release, released) = std::sync::mpsc::channel();
        (
            Self {
                point,
                arrived: Some(arrived),
                release: released,
            },
            arrival,
            release,
        )
    }

    fn visit(&mut self, point: &str) {
        if point == self.point
            && let Some(arrived) = self.arrived.take()
        {
            arrived.send(()).unwrap();
            self.release.recv_timeout(Duration::from_secs(10)).unwrap();
        }
    }
}

async fn arrived(arrival: tokio::sync::oneshot::Receiver<()>) {
    tokio::time::timeout(Duration::from_secs(10), arrival)
        .await
        .expect("the worker must reach the barrier within the watchdog")
        .unwrap();
}

fn startup(recording: &Recording, operation: Operation) -> Startup {
    let mut command = std::process::Command::new(&recording.program.executable);
    command
        .env("ATLAS_CLI", "fixture")
        .env("ATLAS_CONFIG", "fixture");
    Startup {
        config: recording.program.config.clone(),
        output: recording.directory.path().to_owned(),
        port: 12345,
        lock: None,
        child: None,
        child_identity_lost: false,
        test: Some(StartupTest {
            command: Some(command),
            operation,
            probe: Some(Box::new(|_| None)),
            timeout: Duration::from_secs(20),
        }),
    }
}

fn assert_start_is_excluded(output: &Path) {
    let lock = fs::OpenOptions::new()
        .write(true)
        .open(output.join("menubar.lock"))
        .unwrap();
    assert!(fs2::FileExt::try_lock_exclusive(&lock).is_err());
}

async fn wait_for_start_lock(output: &Path) -> atlas_core::files::LockFile {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let lock = fs::OpenOptions::new()
                .create(true)
                .truncate(false)
                .write(true)
                .open(output.join("menubar.lock"))
                .unwrap();
            if fs2::FileExt::try_lock_exclusive(&lock).is_ok() {
                return atlas_core::files::LockFile(lock);
            }
            // Observe actual lock release; elapsed time never implies cleanup.
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("startup must release exclusion within the watchdog")
}

fn hung_startup() -> Recording {
    Recording::new(
        "printf '%s' \"$ATLAS_JSON\" > \"$directory/json\"\n: > \"$directory/ready\"\nIFS= read -r held < \"$directory/hold\"",
    )
}

fn ready_record(pid: u32) -> serde_json::Value {
    serde_json::json!({
        "pid": pid,
        "url": "http://127.0.0.1:12345/private-viewer-canary",
        "extension": {"kept": true}
    })
}

#[tokio::test]
async fn cancelled_startup_reaps_its_child_before_releasing_the_lock() {
    for point in [
        "before spawn",
        "after spawn",
        "before readiness",
        "after readiness",
    ] {
        let recording = hung_startup();
        let (mut gate, arrival, release) = Gate::new(point);
        let (mut cleanup, cleaning, finish) = Gate::new("wait");
        let mut owner = startup(
            &recording,
            Box::new(move |point| {
                gate.visit(point);
                cleanup.visit(point);
                Ok(())
            }),
        );
        owner.test.as_mut().unwrap().probe = Some(Box::new(|pid| pid.map(ready_record)));
        let launching = tokio::spawn(launch_startup(owner));
        arrived(arrival).await;
        if point != "before spawn" {
            recording.started().await;
        }
        assert_start_is_excluded(recording.directory.path());
        launching.abort();
        assert!(launching.await.unwrap_err().is_cancelled());
        release.send(()).unwrap();
        if point != "before spawn" {
            arrived(cleaning).await;
            assert_start_is_excluded(recording.directory.path());
            finish.send(()).unwrap();
        }
        let _lock = wait_for_start_lock(recording.directory.path()).await;
        if point != "before spawn" {
            recording.assert_reaped();
        } else {
            assert!(!recording.directory.path().join("started").exists());
            assert!(!recording.directory.path().join("menubar.log").exists());
        }
    }
}

#[tokio::test]
async fn startup_is_owned_until_the_ready_record_is_accepted() {
    for accept in [false, true] {
        let recording = hung_startup();
        let mut owner = startup(&recording, Box::new(|_| Ok(())));
        owner.test.as_mut().unwrap().probe = Some(Box::new(|pid| pid.map(ready_record)));
        let (ready, result) = tokio::sync::oneshot::channel();
        let (acceptance, accepted) = std::sync::mpsc::channel();
        let worker = tokio::task::spawn_blocking(move || owner.run(ready, accepted));
        let running = result.await.unwrap().unwrap();
        let identity = recording.started().await;
        assert_eq!(running, (false, ready_record(identity.pid as u32)));
        assert_start_is_excluded(recording.directory.path());
        assert!(atlas_core::process::info(identity.pid).unwrap().is_some());
        if accept {
            acceptance.send(()).unwrap();
        }
        drop(acceptance);
        worker.await.unwrap();
        let _lock = wait_for_start_lock(recording.directory.path()).await;
        if accept {
            assert_eq!(
                atlas_core::process::info(identity.pid)
                    .unwrap()
                    .unwrap()
                    .instance,
                identity
            );
            // The accepted stand-in is ours; the service would keep running.
            kill(Pid::from_raw(identity.pid), Signal::SIGKILL).unwrap();
            recording.reaped().await;
        } else {
            recording.assert_reaped();
        }
    }
}

#[tokio::test]
async fn cleanup_failure_never_claims_the_child_ended() {
    for fail in ["kill", "wait", "both"] {
        let recording = hung_startup();
        let (mut gate, arrival, release) = Gate::new("after spawn");
        let (mut retry, retrying, recover) = Gate::new("kill");
        let mut failures = std::collections::VecDeque::from(if fail == "both" {
            vec!["kill", "poll"]
        } else {
            vec![fail]
        });
        let mut owner = startup(
            &recording,
            Box::new(move |point| {
                gate.visit(point);
                if failures.front().copied() == Some(point) {
                    failures.pop_front();
                    return Err(std::io::Error::other("private-cleanup-canary"));
                }
                if failures.is_empty() {
                    retry.visit(point);
                }
                Ok(())
            }),
        );
        owner.test.as_mut().unwrap().timeout = Duration::ZERO;
        let launching = tokio::spawn(launch_startup(owner));
        arrived(arrival).await;
        let identity = recording.started().await;
        release.send(()).unwrap();
        let error = launching.await.unwrap().unwrap_err();
        assert_eq!(error.code, "menubar");
        assert!(error.uncertain);
        assert_eq!(
            error.error,
            format!(
                "The menu bar did not start; termination of process {} is unknown",
                identity.pid
            )
        );
        let source = error
            .source
            .as_ref()
            .unwrap()
            .downcast_ref::<StartupFailure>()
            .unwrap();
        assert!(source.primary.error.contains("did not start in time"));
        assert_eq!(source.cleanup.pid, identity.pid as u32);
        assert_eq!(source.cleanup.kill.is_some(), fail != "wait");
        assert!(source.cleanup.wait.to_string().contains(if fail != "kill" {
            "private-cleanup-canary"
        } else {
            "still runs"
        }));
        for diagnostic in [
            serde_json::to_string(&error).unwrap(),
            format!("{error:?}"),
            format!("{source:?}"),
        ] {
            assert!(!diagnostic.contains("private-cleanup-canary"));
            assert!(!diagnostic.contains("private-viewer-canary"));
            assert!(!diagnostic.contains("reaped"));
        }
        arrived(retrying).await;
        assert_start_is_excluded(recording.directory.path());
        assert!(atlas_core::process::info(identity.pid).unwrap().is_some());
        recover.send(()).unwrap();
        let _lock = wait_for_start_lock(recording.directory.path()).await;
        recording.assert_reaped();
    }
}

#[tokio::test]
async fn a_competing_start_observes_the_existing_verified_service() {
    let recording = hung_startup();
    let (mut gate, arrival, release) = Gate::new("after readiness");
    let mut first = startup(
        &recording,
        Box::new(move |point| {
            gate.visit(point);
            Ok(())
        }),
    );
    first.test.as_mut().unwrap().probe = Some(Box::new(|pid| pid.map(ready_record)));
    let launching = tokio::spawn(launch_startup(first));
    arrived(arrival).await;
    let identity = recording.started().await;
    let expected = ready_record(identity.pid as u32);
    recording.wait_for("ready").await;
    let (mut contention, contending, continue_start) = Gate::new("lock contention");
    let mut second = startup(
        &recording,
        Box::new(move |point| {
            contention.visit(point);
            assert_ne!(point, "before spawn", "the competing start must not spawn");
            Ok(())
        }),
    );
    let existing = expected.clone();
    second.test.as_mut().unwrap().probe = Some(Box::new(move |pid| {
        assert!(pid.is_none());
        Some(existing.clone())
    }));
    let competing = tokio::spawn(launch_startup(second));
    arrived(contending).await;
    assert_start_is_excluded(recording.directory.path());
    release.send(()).unwrap();
    assert_eq!(launching.await.unwrap().unwrap(), (false, expected.clone()));
    continue_start.send(()).unwrap();
    assert_eq!(competing.await.unwrap().unwrap(), (true, expected));
    assert_eq!(
        fs::read(recording.directory.path().join("json")).unwrap(),
        b"1"
    );
    let args = fs::read(recording.directory.path().join("argv")).unwrap();
    assert_eq!(
        args,
        format!(
            "menubar\0--serve\0--port\012345\0--config\0{}\0",
            recording.program.config.display()
        )
        .as_bytes()
    );
    assert!(!String::from_utf8_lossy(&args).contains("private-viewer-canary"));
    kill(Pid::from_raw(identity.pid), Signal::SIGKILL).unwrap();
    recording.reaped().await;
}

#[tokio::test]
async fn a_hung_startup_keeps_the_timeout_words_and_reaps_its_child() {
    let recording = hung_startup();
    let (mut gate, arrival, release) = Gate::new("after spawn");
    let mut owner = startup(
        &recording,
        Box::new(move |point| {
            gate.visit(point);
            Ok(())
        }),
    );
    owner.test.as_mut().unwrap().timeout = Duration::ZERO;
    let launching = tokio::spawn(launch_startup(owner));
    arrived(arrival).await;
    recording.started().await;
    release.send(()).unwrap();
    let error = launching.await.unwrap().unwrap_err();
    assert_eq!(
        serde_json::to_value(&error).unwrap(),
        serde_json::json!({
            "ok": false, "code": "menubar", "uncertain": false,
            "error": format!("The menu bar did not start in time; see {}", recording.directory.path().join("menubar.log").display())
        })
    );
    let _lock = wait_for_start_lock(recording.directory.path()).await;
    recording.assert_reaped();
}

#[tokio::test]
async fn a_startup_worker_panic_cleans_up_before_reporting_failure() {
    let recording = hung_startup();
    let (mut gate, arrival, release) = Gate::new("after spawn");
    let owner = startup(
        &recording,
        Box::new(move |point| {
            gate.visit(point);
            assert_ne!(point, "before readiness", "private-panic-canary");
            Ok(())
        }),
    );
    let launching = tokio::spawn(launch_startup(owner));
    arrived(arrival).await;
    recording.started().await;
    release.send(()).unwrap();
    let error = launching.await.unwrap().unwrap_err();
    assert_eq!(error.code, "menubar");
    assert_eq!(error.error, "The menu bar start worker stopped");
    assert!(!error.uncertain);
    let _lock = wait_for_start_lock(recording.directory.path()).await;
    recording.assert_reaped();
}

#[tokio::test]
async fn cancellation_while_waiting_for_the_start_lock_never_spawns() {
    let recording = hung_startup();
    let held = wait_for_start_lock(recording.directory.path()).await;
    let mut owner = startup(
        &recording,
        Box::new(|point| {
            assert_ne!(point, "before spawn", "a cancelled waiter must not spawn");
            Ok(())
        }),
    );
    let (ready, result) = tokio::sync::oneshot::channel();
    // Closing before the blocking worker starts also covers a queued launch.
    drop(result);
    assert_eq!(
        owner.prepare(&ready).unwrap_err().error,
        "The menu bar start was cancelled"
    );
    drop(held);
    assert!(!recording.directory.path().join("started").exists());
}

#[tokio::test]
async fn port_contention_uses_the_other_services_record_and_preserves_startup_errors() {
    for reply in [
        r#"{"ok":true,"alreadyRunning":true}"#,
        r#"{"ok":false,"code":"port-in-use","error":"The port is in use"}"#,
        r#"{"ok":false,"code":"menubar","error":"A startup failure"}"#,
    ] {
        let recording = Recording::new(&format!("printf '%s\\n' '{reply}'"));
        let (mut gate, arrival, release) = Gate::new("after spawn");
        let mut owner = startup(
            &recording,
            Box::new(move |point| {
                gate.visit(point);
                Ok(())
            }),
        );
        let mut probes = 0;
        owner.test.as_mut().unwrap().probe = Some(Box::new(move |_| {
            probes += 1;
            (probes > 1).then(|| ready_record(42))
        }));
        let launching = tokio::spawn(launch_startup(owner));
        arrived(arrival).await;
        // Read the child's actual completion without reaping its owned handle.
        recording.wait_for("started").await;
        release.send(()).unwrap();
        let result = launching.await.unwrap();
        if reply.contains("A startup failure") {
            let error = result.unwrap_err();
            assert_eq!(error.error, "The menu bar did not start: A startup failure");
            assert_eq!(error.code, "menubar");
            assert!(!error.uncertain);
        } else {
            assert_eq!(result.unwrap(), (true, ready_record(42)));
        }
    }
}

#[test]
fn startup_requires_a_matching_record_and_socket_reply() {
    use std::io::Read;
    use std::os::fd::AsFd;

    let recording = hung_startup();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let pid = std::process::id();
    let record_path = recording
        .directory
        .path()
        .join(crate::viewer::MENUBAR_RECORD);
    let record = serde_json::json!({
        "pid": pid,
        "url": format!("http://127.0.0.1:{port}/private-socket-canary"),
        "extension": {"kept": true}
    });
    fs::write(&record_path, serde_json::to_vec(&record).unwrap()).unwrap();
    std::thread::scope(|scope| {
        let server = scope.spawn(move || {
            for answering in [pid + 1, pid] {
                let mut socket = [nix::poll::PollFd::new(
                    listener.as_fd(),
                    nix::poll::PollFlags::POLLIN,
                )];
                assert_eq!(nix::poll::poll(&mut socket, 10_000u16).unwrap(), 1);
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(10)))
                    .unwrap();
                stream
                    .set_write_timeout(Some(Duration::from_secs(10)))
                    .unwrap();
                let mut request = Vec::new();
                let mut byte = [0];
                while !request.ends_with(b"\r\n\r\n") {
                    stream.read_exact(&mut byte).unwrap();
                    request.extend_from_slice(&byte);
                    assert!(request.len() < 4096);
                }
                let body = serde_json::json!({"ok":true,"service":"atlas-viewer","pid":answering})
                    .to_string();
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
                .unwrap();
            }
        });
        let mut owner = startup(
            &recording,
            Box::new(|point| {
                assert_ne!(
                    point, "before spawn",
                    "an existing verified service must not spawn"
                );
                Ok(())
            }),
        );
        owner.port = port;
        owner.test.as_mut().unwrap().probe = None;
        assert!(owner.viewer_running(&record_path).is_none());
        let (ready, _result) = tokio::sync::oneshot::channel();
        assert_eq!(owner.prepare(&ready).unwrap(), (true, record));
        assert!(owner.child.is_none());
        assert!(!recording.directory.path().join("menubar.log").exists());
        server.join().unwrap();
    });
}

#[tokio::test]
async fn a_poll_failure_reaps_the_child_and_preserves_the_original_error() {
    let recording = hung_startup();
    let (mut gate, arrival, release) = Gate::new("after spawn");
    let mut failed = false;
    let owner = startup(
        &recording,
        Box::new(move |point| {
            gate.visit(point);
            if point == "poll" && !failed {
                failed = true;
                return Err(std::io::Error::from_raw_os_error(libc::EIO));
            }
            Ok(())
        }),
    );
    let launching = tokio::spawn(launch_startup(owner));
    arrived(arrival).await;
    recording.started().await;
    release.send(()).unwrap();
    let error = launching.await.unwrap().unwrap_err();
    assert_eq!(error.code, "io");
    assert!(!error.uncertain);
    assert_eq!(
        error.error,
        std::io::Error::from_raw_os_error(libc::EIO).to_string()
    );
    assert_eq!(
        error
            .source
            .as_ref()
            .unwrap()
            .downcast_ref::<std::io::Error>()
            .unwrap()
            .raw_os_error(),
        Some(libc::EIO)
    );
    let _lock = wait_for_start_lock(recording.directory.path()).await;
    recording.assert_reaped();
}

#[tokio::test]
async fn losing_the_child_identity_stops_cleanup_signals_and_retains_exclusion() {
    for fail in ["poll", "wait"] {
        let recording = hung_startup();
        let (mut gate, arrival, release) = Gate::new("after spawn");
        let mut failed = false;
        let mut owner = startup(
            &recording,
            Box::new(move |point| {
                assert!(
                    !failed,
                    "lost identity must prevent further child operations"
                );
                gate.visit(point);
                if point == fail {
                    failed = true;
                    return Err(std::io::Error::from_raw_os_error(libc::ECHILD));
                }
                Ok(())
            }),
        );
        owner.test.as_mut().unwrap().timeout = Duration::ZERO;
        let worker = tokio::task::spawn_blocking(move || {
            let (ready, _result) = tokio::sync::oneshot::channel();
            let error = owner.prepare(&ready).unwrap_err();
            if fail == "poll" {
                assert_eq!(error.code, "io");
                assert_eq!(
                    error.error,
                    std::io::Error::from_raw_os_error(libc::ECHILD).to_string()
                );
            } else {
                assert!(error.error.contains("did not start in time"));
            }
            let cleanup = owner.cleanup().unwrap_err();
            assert_eq!(cleanup.wait.raw_os_error(), Some(libc::ECHILD));
            owner
        });
        arrived(arrival).await;
        let identity = recording.started().await;
        release.send(()).unwrap();
        let mut owner = worker.await.unwrap();
        assert!(owner.child_identity_lost);
        assert_eq!(owner.child.as_ref().unwrap().id(), identity.pid as u32);
        for _ in 0..2 {
            let cleanup = owner.cleanup().unwrap_err();
            assert_eq!(cleanup.pid, identity.pid as u32);
            assert!(cleanup.kill.is_none());
            assert_eq!(cleanup.wait.raw_os_error(), Some(libc::ECHILD));
            assert_start_is_excluded(recording.directory.path());
        }
        // Only the fixture may clean up this simulated loss of identity.
        drop(owner);
        drop(recording);
    }
}

#[tokio::test]
async fn the_launcher_sends_its_private_url_only_through_stdin() {
    let test = "menubar::tests::the_launcher_sends_its_private_url_only_through_stdin";
    if std::env::var("ATLAS_LAUNCHER_TEST").as_deref() != Ok(test) {
        let mut command = Command::from(atlas_core::test_support::rerun(test));
        let output = tokio::time::timeout(
            Duration::from_secs(30),
            atlas_core::spawn::output_async(
                command
                    .env("ATLAS_LAUNCHER_TEST", test)
                    .env("XPC_SERVICE_NAME", LOGIN_JOB)
                    .env("ATLAS_CLI", "inherited-cli")
                    .env("ATLAS_CONFIG", "inherited-config")
                    .kill_on_drop(true),
            ),
        )
        .await
        .unwrap()
        .unwrap();
        assert!(output.status.success(), "the isolated launcher test failed");
        assert!(String::from_utf8_lossy(&output.stdout).contains("running 1 test"));
        return;
    }
    assert_eq!(std::env::var("XPC_SERVICE_NAME").unwrap(), LOGIN_JOB);
    assert_eq!(std::env::var("ATLAS_CLI").unwrap(), "inherited-cli");
    assert_eq!(std::env::var("ATLAS_CONFIG").unwrap(), "inherited-config");
    let recording = Recording::reader();
    let url = format!("http://127.0.0.1:12345/{:032x}", rand::random::<u128>());
    let args = ["--render", "a path with spaces.png", "--dark"];
    let (mut child, stdin) = spawn(&recording.program, &url, None, &args).await.unwrap();
    let instance = recording.started().await;
    assert_eq!(child.id(), Some(instance.pid as u32));
    recording.wait_for("ready").await;

    let read = |name| fs::read(recording.directory.path().join(name)).unwrap();
    assert!(
        read("argv") == format!("{}\0", args.join("\0")).as_bytes(),
        "the launcher must preserve its exact arguments"
    );
    assert!(
        read("url") == format!("{url}\n").as_bytes(),
        "the URL line must arrive while stdin stays open"
    );
    assert!(
        read("cli")
            == fs::canonicalize(std::env::current_exe().unwrap())
                .unwrap()
                .as_os_str()
                .as_encoded_bytes()
    );
    assert!(read("config") == recording.program.config.as_os_str().as_encoded_bytes());
    assert!(read("xpc").is_empty());
    assert!(!recording.directory.path().join("eof").exists());
    assert!(child.try_wait().unwrap().is_none());
    drop(stdin);
    assert!(
        tokio::time::timeout(Duration::from_secs(10), child.wait())
            .await
            .unwrap()
            .unwrap()
            .success()
    );
    recording.wait_for("eof").await;
    recording.reaped().await;
}

#[tokio::test]
async fn closing_the_launcher_pipe_lets_the_child_exit() {
    tokio::time::timeout(Duration::from_secs(30), async {
        let recording = Recording::reader();
        let (mut child, mut stdin) = spawn(&recording.program, "private-url", None, &[])
            .await
            .unwrap();
        recording.started().await;
        recording.wait_for("ready").await;
        assert!(
            fs::read(recording.directory.path().join("argv"))
                .unwrap()
                .is_empty()
        );
        stdin
            .write_all(Line::Viewer.text().as_bytes())
            .await
            .unwrap();
        drop(stdin);
        recording.wait_for("eof").await;
        assert!(child.wait().await.unwrap().success());
        assert_eq!(
            fs::read(recording.directory.path().join("rest")).unwrap(),
            b"viewer\n"
        );
        recording.reaped().await;
    })
    .await
    .expect("the launcher pipe test must finish within the watchdog");
}

#[tokio::test]
async fn a_failed_launch_leaves_no_owned_child() {
    tokio::time::timeout(Duration::from_secs(30), async {
        for ending in ["exit 23", "IFS= read -r held < \"$directory/hold\""] {
            let recording = Recording::new(&format!(
                "IFS= read -r release < \"$directory/release\"\nexec 0<&-\n{ending}"
            ));
            // Exceed pipe capacity so launch cannot finish before the child closes stdin.
            let url = "private-launch-canary".repeat(65536);
            let launching = spawn(&recording.program, &url, None, &[]);
            tokio::pin!(launching);
            tokio::select! {
                _ = &mut launching => panic!("launch finished before the child closed stdin"),
                _ = recording.started() => {}
            }
            recording.release().await;
            let result = tokio::time::timeout(Duration::from_secs(10), launching)
                .await
                .unwrap();
            assert!(result.is_err(), "a closed pipe must fail launch");
            let error = result.err().unwrap();
            assert_eq!(error.code, "io");
            assert!(!error.to_string().contains("private-launch-canary"));
            assert!(
                !serde_json::to_string(&error)
                    .unwrap()
                    .contains("private-launch-canary")
            );
            recording.reaped().await;
        }
    })
    .await
    .expect("the failed launch test must finish within the watchdog");
}

#[tokio::test]
async fn dropping_the_launcher_child_ends_only_its_own_process() {
    tokio::time::timeout(Duration::from_secs(30), async {
        let recording = Recording::reader();
        let other = Recording::reader();
        let (child, stdin) = spawn(&recording.program, "private-url", None, &[])
            .await
            .unwrap();
        recording.started().await;
        recording.wait_for("ready").await;
        let (mut unrelated, unrelated_stdin) =
            spawn(&other.program, "other-private-url", None, &[])
                .await
                .unwrap();
        let identity = other.started().await;
        other.wait_for("ready").await;

        drop(child);
        recording.reaped().await;
        assert_eq!(
            atlas_core::process::info(identity.pid)
                .unwrap()
                .unwrap()
                .instance,
            identity
        );
        assert!(unrelated.try_wait().unwrap().is_none());
        assert!(!recording.directory.path().join("eof").exists());
        drop(stdin);
        drop(unrelated_stdin);
        assert!(unrelated.wait().await.unwrap().success());
        other.reaped().await;
    })
    .await
    .expect("the launcher child drop test must finish within the watchdog");
}

#[test]
fn open_at_login_takes_only_its_three_states() {
    for (state, login) in [
        ("on", Login::On),
        ("off", Login::Off),
        ("status", Login::Status),
    ] {
        assert_eq!(Login::parse(state).unwrap(), login);
        assert_eq!(login.as_str(), state);
    }

    assert_eq!(Login::parse("yes").unwrap_err().code, "usage");
}

#[test]
fn a_suspended_menu_bar_that_does_not_answer_is_not_reported_absent() {
    use atlas_core::test_support::{TestChild, message, permitted, rerun};
    const NAME: &str =
        "menubar::tests::a_suspended_menu_bar_that_does_not_answer_is_not_reported_absent";
    if std::env::var_os("ATLAS_SUSPENDED_MENUBAR").is_some() {
        message("ready");
        permitted("continue");
        return;
    }

    // A live process of this very executable stands in for the menu bar;
    // its port takes connections and never answers.
    let mut child = TestChild::spawn(
        rerun(NAME)
            .arg("--nocapture")
            .env("ATLAS_SUSPENDED_MENUBAR", "1"),
    );
    child.expect("ready");
    let pid = Pid::from_raw(i32::try_from(child.process.id()).unwrap());
    kill(pid, Signal::SIGSTOP).unwrap();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let output = tempfile::tempdir().unwrap();
    let record = output.path().join(crate::viewer::MENUBAR_RECORD);
    fs::write(
        &record,
        serde_json::json!({"url": format!("http://127.0.0.1:{port}/private/"), "pid": pid.as_raw()})
            .to_string(),
    )
    .unwrap();

    let outcome = stop(output.path());
    kill(pid, Signal::SIGCONT).unwrap();
    child.permit("continue");
    assert!(child.wait().success());
    let error = outcome.expect_err("a menu bar that may still run is not absent");
    assert_eq!(error.code, Code::Menubar);
    assert!(record.exists());
}

#[test]
fn a_menu_bar_whose_records_are_unknown_runs_while_any_other_atlas_does() {
    use atlas_core::process::{Info, Instance};
    let process = |pid: i32, ppid: i32, name: &str, zombie: bool| Info {
        instance: Instance::new(pid, [1, 0]),
        ppid,
        pgid: pid,
        uid: 501,
        ruid: 501,
        svuid: 501,
        zombie,
        stopped: false,
        name: name.to_owned(),
    };
    let unlocated = || Error::new(Code::Config, "The settings do not load");
    let atlas = std::ffi::OsStr::new("atlas");
    let find = |listed: Vec<Info>| elsewhere(unlocated(), Ok(listed), 30, atlas);

    // This process, the Atlas that started it, exited ones and other programs.
    let quiet = vec![
        process(10, 1, "atlas", false),
        process(20, 10, "zsh", false),
        process(30, 20, "atlas", false),
        process(40, 1, "atlas", true),
        process(50, 1, "AtlasApp", false),
    ];
    assert_eq!(find(quiet.clone()).unwrap(), None);

    let mut busy = quiet;
    busy.push(process(60, 1, "atlas", false));
    let error = find(busy).unwrap_err();
    assert_eq!(error.code, Code::Menubar);
    assert_eq!(
        error.error,
        "Atlas can't find the menu bar's record while Atlas runs as process 60"
    );

    // The kernel keeps only the first 16 bytes of a longer name.
    let long = std::ffi::OsStr::new("atlas-0123456789abcdef");
    let listed = vec![process(60, 1, "atlas-0123456789", false)];
    assert!(elsewhere(unlocated(), Ok(listed), 30, long).is_err());

    let unlisted = elsewhere(
        unlocated(),
        Err(Error::new(Code::State, "no listing")),
        30,
        atlas,
    )
    .unwrap_err();
    assert_eq!(
        unlisted.error,
        "Atlas could not tell whether the menu bar runs"
    );
}

#[test]
fn only_a_menu_bar_known_not_to_run_is_reported_absent() {
    use atlas_core::process::{Info, Instance};
    use std::io::ErrorKind;

    const PID: i32 = 4242;
    let me = nix::unistd::getuid().as_raw();
    let live = |uid: u32, zombie: bool| Info {
        instance: Instance::new(PID, [1, 2]),
        ppid: 1,
        pgid: PID,
        uid,
        ruid: uid,
        svuid: uid,
        zombie,
        stopped: false,
        name: "atlas".into(),
    };
    let written = serde_json::json!({"url": "http://127.0.0.1:8123/private/", "pid": PID});
    let record = Path::new("/nowhere/menubar.json");

    #[derive(Clone)]
    struct World {
        read: std::result::Result<Vec<u8>, ErrorKind>,
        info: std::result::Result<Option<Info>, ()>,
        answers: Option<u32>,
        atlas: std::result::Result<bool, ()>,
    }
    let find = |world: World| {
        serving(
            record,
            |_| world.read.map_err(std::io::Error::from),
            |_| {
                world
                    .info
                    .map_err(|()| Error::new(Code::State, "unreadable"))
            },
            |url| {
                assert_eq!(url, "http://127.0.0.1:8123/private/");
                world.answers
            },
            |pid| {
                assert_eq!(pid, PID);
                world.atlas.map_err(|()| std::io::Error::other("ps failed"))
            },
        )
    };
    let running = World {
        read: Ok(written.to_string().into_bytes()),
        info: Ok(Some(live(me, false))),
        answers: Some(4242),
        atlas: Ok(true),
    };
    assert_eq!(
        find(running.clone()).unwrap(),
        Some((8123, Instance::new(PID, [1, 2])))
    );

    let absent = [
        (
            "no record",
            World {
                read: Err(ErrorKind::NotFound),
                ..running.clone()
            },
        ),
        (
            "no process",
            World {
                info: Ok(None),
                answers: None,
                ..running.clone()
            },
        ),
        (
            "an exited process",
            World {
                info: Ok(Some(live(me, true))),
                ..running.clone()
            },
        ),
        (
            "another account's",
            World {
                info: Ok(Some(live(me + 1, false))),
                ..running.clone()
            },
        ),
        (
            "another program",
            World {
                answers: None,
                atlas: Ok(false),
                ..running.clone()
            },
        ),
    ];
    for (case, world) in absent {
        assert_eq!(find(world).unwrap(), None, "{case}");
    }

    let unknown = [
        (
            "an unreadable record",
            World {
                read: Err(ErrorKind::PermissionDenied),
                ..running.clone()
            },
            "can't be read",
        ),
        (
            "a malformed record",
            World {
                read: Ok(b"{\"url\":".to_vec()),
                ..running.clone()
            },
            "can't be read",
        ),
        (
            "a record without its process",
            World {
                read: Ok(br#"{"url":"http://127.0.0.1:8123/private/"}"#.to_vec()),
                ..running.clone()
            },
            "can't be read",
        ),
        (
            "an unreadable process",
            World {
                info: Err(()),
                ..running.clone()
            },
            "could not tell",
        ),
        (
            "a suspended menu bar",
            World {
                answers: None,
                ..running.clone()
            },
            "does not answer",
        ),
        (
            "another answering process",
            World {
                answers: Some(7),
                ..running.clone()
            },
            "does not answer",
        ),
        (
            "a failed executable check",
            World {
                answers: None,
                atlas: Err(()),
                ..running.clone()
            },
            "could not tell",
        ),
        (
            "an answering process that is not Atlas",
            World {
                atlas: Ok(false),
                ..running.clone()
            },
            "not Atlas",
        ),
    ];
    for (case, world, said) in unknown {
        let error = find(world).unwrap_err();
        assert_eq!(error.code, Code::Menubar, "{case}");
        assert!(error.error.contains(said), "{case}: {}", error.error);
    }
}

#[test]
fn a_menu_bar_surviving_its_stop_deadline_is_not_reported_stopped() {
    use atlas_core::process::State;
    use nix::errno::Errno;

    /// Stop with each look answering the next of `looks` (the last again
    /// once they run out), `signal`'s outcome and three waits allowed.
    fn stop_seeing(looks: &[State], signal: nix::Result<()>) -> (Result<Option<u16>>, bool, usize) {
        let mut looks = looks.iter().copied();
        let mut last = State::Running;
        let mut signalled = false;
        let mut waits = 0;
        let outcome = stopped(
            8123,
            || {
                last = looks.next().unwrap_or(last);
                last
            },
            || {
                signalled = true;
                signal
            },
            || {
                waits += 1;
                waits < 3
            },
        );
        (outcome, signalled, waits)
    }

    for still in [State::Running, State::Stopped, State::Unknown] {
        let (outcome, signalled, waits) = stop_seeing(&[still], Ok(()));
        let error = outcome.unwrap_err();
        assert_eq!(error.code, Code::Menubar, "{still:?}");
        assert_eq!(error.error, "The menu bar did not stop within a minute");
        assert!(signalled);
        assert_eq!(waits, 3, "it waits out its deadline: {still:?}");
    }

    let (outcome, signalled, _) =
        stop_seeing(&[State::Running, State::Running, State::Gone], Ok(()));
    assert_eq!(outcome.unwrap(), Some(8123), "seen to exit");
    assert!(signalled);

    let (outcome, signalled, _) = stop_seeing(&[State::Running, State::Gone], Err(Errno::ESRCH));
    assert_eq!(
        outcome.unwrap(),
        Some(8123),
        "gone before the signal landed"
    );
    assert!(signalled);

    let (outcome, signalled, waits) = stop_seeing(&[State::Running], Err(Errno::EPERM));
    assert_eq!(
        outcome.unwrap_err().error,
        "Atlas could not stop the menu bar"
    );
    assert!(signalled);
    assert_eq!(waits, 0, "a refused signal is not exit evidence");

    let (outcome, signalled, _) = stop_seeing(&[State::Gone], Ok(()));
    assert_eq!(outcome.unwrap(), None, "another process now has its PID");
    assert!(!signalled, "and is never signalled");
}

#[test]
fn rendering_selects_the_appearance() {
    assert_eq!(render_args("/tmp/a.png", false), ["--render", "/tmp/a.png"]);
    assert_eq!(
        render_args("/tmp/a.png", true),
        ["--render", "/tmp/a.png", "--dark"]
    );
}

#[tokio::test]
async fn the_app_is_given_its_secret_on_stdin_after_the_url_and_nowhere_else() {
    tokio::time::timeout(Duration::from_secs(30), async {
        let recording = Recording::reader();
        let secret = "5e".repeat(32);
        let (mut child, stdin) = spawn(&recording.program, "private-url", Some(&secret), &[])
            .await
            .unwrap();
        recording.started().await;
        recording.wait_for("ready").await;
        drop(stdin);
        child.wait().await.unwrap();
        recording.wait_for("eof").await;

        let read = |name| fs::read_to_string(recording.directory.path().join(name)).unwrap();
        assert_eq!(read("url"), "private-url\n");
        assert_eq!(read("rest"), format!("secret {secret}\n"));
        assert!(!read("argv").contains(&secret));
        recording.reaped().await;
    })
    .await
    .unwrap();
}

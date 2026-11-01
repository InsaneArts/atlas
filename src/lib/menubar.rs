//! The macOS app: the Swift package in app/, whose AtlasApp executable
//! draws the menu bar, the viewer and the setup window. Atlas.app ships it
//! signed beside this executable; a development checkout builds the package
//! with SwiftPM. It renders the local viewer's status and frame and
//! runs `atlas` commands; it reaches nothing else. The capability URL
//! travels on its stdin, never in argv, and closing stdin makes it quit.
use crate::error::{Code, Error, Result};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};
use tokio::{
    io::AsyncWriteExt,
    process::{Child, ChildStdin, Command},
};

/// The app's executable, shipped beside `atlas` in Atlas.app.
pub const APP_EXECUTABLE: &str = "AtlasApp";

/// The login agent that runs `atlas menubar` (see xtask); launchd names
/// the job in XPC_SERVICE_NAME.
pub const LOGIN_JOB: &str = "com.insanearts.atlas.menubar";

/// Whether this process is the menu bar's login agent, which serves the
/// menu bar itself rather than starting it.
pub fn started_at_login() -> bool {
    std::env::var("XPC_SERVICE_NAME").is_ok_and(|job| job == LOGIN_JOB)
}

/// Start the menu bar unless it runs already: whether it did, and its
/// private record (pid and viewer URL). One start runs at a time; a start
/// that fails is stopped, never left half-running.
pub async fn launch(config: &Path, port: u16, output: &Path) -> Result<(bool, serde_json::Value)> {
    launch_startup(Startup {
        config: config.to_owned(),
        output: output.to_owned(),
        port,
        lock: None,
        child: None,
        child_identity_lost: false,
        #[cfg(test)]
        test: None,
    })
    .await
}

async fn launch_startup(startup: Startup) -> Result<(bool, serde_json::Value)> {
    let (ready, result) = tokio::sync::oneshot::channel();
    let (accept, accepted) = std::sync::mpsc::channel();
    let worker = tokio::task::spawn_blocking(move || startup.run(ready, accepted));
    match result.await {
        Ok(result) => {
            let running = result?;
            accept.send(()).map_err(|_| startup_worker_failed())?;
            // Acceptance and return have no cancellation point between them.
            // The worker only relinquishes the child after receiving acceptance.
            Ok(running)
        }
        Err(_) => {
            // Observe an unexpected worker failure; its panic text is private.
            match worker.await {
                Ok(()) => Err(startup_worker_failed()),
                Err(error) => Err(startup_worker_failed().with_source(error)),
            }
        }
    }
}

fn startup_worker_failed() -> Error {
    Error::new(Code::Menubar, "The menu bar start worker stopped")
}

type Ready = tokio::sync::oneshot::Sender<Result<(bool, serde_json::Value)>>;

/// One blocking worker owns files, the start lock, probes and the unreaped
/// child. Lock -> child is the only acquisition order; no runtime worker
/// waits on storage, process cleanup or the viewer's one-second socket probe.
struct Startup {
    config: PathBuf,
    output: PathBuf,
    port: u16,
    lock: Option<crate::files::LockFile>,
    child: Option<std::process::Child>,
    child_identity_lost: bool,
    #[cfg(test)]
    test: Option<tests::StartupTest>,
}

impl Startup {
    fn run(mut self, ready: Ready, accepted: std::sync::mpsc::Receiver<()>) {
        let prepared =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.prepare(&ready)))
                .unwrap_or_else(|_| Err(startup_worker_failed()));
        let (mut reply, primary) = match prepared {
            Ok(running) => {
                if ready.send(Ok(running)).is_ok() && accepted.recv().is_ok() {
                    // The accepted service intentionally outlives its launcher.
                    self.child.take();
                    return;
                }
                (None, startup_cancelled())
            }
            Err(error) => (Some(ready), error),
        };

        match self.cleanup() {
            Ok(()) => {
                if let Some(reply) = reply.take() {
                    // Cleanup is complete even when the caller has disappeared.
                    let _ = reply.send(Err(primary));
                }
            }
            Err(cleanup) => {
                if let Some(reply) = reply.take() {
                    let error = Error::new(
                        primary.code,
                        format!(
                            "The menu bar did not start; termination of process {} is unknown",
                            cleanup.pid
                        ),
                    )
                    .uncertain()
                    .with_source(StartupFailure { primary, cleanup });
                    // A missing receiver never releases the pending child or lock.
                    let _ = reply.send(Err(error));
                }
                // Keep the Child and exclusion until a checked reap. ECHILD loses
                // signal authority permanently; its numeric PID may be reused.
                // Even after reporting uncertainty, this worker owns recovery.
                loop {
                    std::thread::sleep(Duration::from_millis(100));
                    if self.cleanup().is_ok() {
                        break;
                    }
                }
            }
        }
    }

    fn prepare(&mut self, ready: &Ready) -> Result<(bool, serde_json::Value)> {
        self.lock = Some(self.starting(ready)?);
        let record = self.output.join(crate::viewer::MENUBAR_RECORD);
        if let Some(running) = self.viewer_running(&record) {
            return Ok((true, running));
        }
        self.check(ready, "before spawn")?;
        let log_path = self.output.join("menubar.log");
        let log = fs::File::create(&log_path)?;
        let mut command = std::process::Command::new(std::env::current_exe()?);
        #[cfg(test)]
        if let Some(test) = &mut self.test {
            command = test.command.take().unwrap();
        }
        self.child = Some(
            command
                .arg("menubar")
                .arg("--serve")
                .arg("--port")
                .arg(self.port.to_string())
                .arg("--config")
                .arg(fs::canonicalize(&self.config)?)
                .env("ATLAS_JSON", "1")
                .stdin(Stdio::null())
                .stdout(log.try_clone()?)
                .stderr(log)
                .spawn()?,
        );
        let pid = self.child.as_ref().map(std::process::Child::id);
        let timeout = Duration::from_secs(20);
        #[cfg(test)]
        let timeout = self.test.as_ref().map_or(timeout, |test| test.timeout);
        let deadline = std::time::Instant::now() + timeout;
        self.check(ready, "after spawn")?;

        loop {
            self.check(ready, "before readiness")?;
            if let Some(status) = self.poll()? {
                return self.exited(status, &log_path, &record, ready);
            }
            if let Some(running) = self
                .viewer_running(&record)
                .filter(|running| running["pid"].as_u64() == pid.map(u64::from))
            {
                self.check(ready, "after readiness")?;
                return Ok((false, running));
            }
            if std::time::Instant::now() >= deadline {
                return Err(Error::new(
                    Code::Menubar,
                    format!(
                        "The menu bar did not start in time; see {}",
                        log_path.display()
                    ),
                ));
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    fn exited(
        &mut self,
        status: std::process::ExitStatus,
        log: &Path,
        record: &Path,
        ready: &Ready,
    ) -> Result<(bool, serde_json::Value)> {
        let reply = last_reply(log).unwrap_or_default();
        let already = status.success() && reply["alreadyRunning"] == true;
        let code = reply["code"]
            .as_str()
            .and_then(crate::error::Code::parse)
            .unwrap_or(Code::Menubar);

        // A competing menu bar can hold the port before its record appears.
        if already || code == "port-in-use" {
            for _ in 0..25 {
                self.check(ready, "port contention")?;
                if let Some(running) = self.viewer_running(record) {
                    return Ok((true, running));
                }
                std::thread::sleep(Duration::from_millis(200));
            }
        }
        let reason = if already {
            format!(
                "another menu bar holds port {} but did not answer",
                self.port
            )
        } else {
            reply["error"]
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| format!("it stopped; see {}", log.display()))
        };
        Err(Error::new(
            code,
            format!("The menu bar did not start: {reason}"),
        ))
    }

    fn check(&mut self, ready: &Ready, _point: &str) -> Result<()> {
        #[cfg(test)]
        self.operation(_point)?;
        if ready.is_closed() {
            return Err(startup_cancelled());
        }
        Ok(())
    }

    fn viewer_running(&mut self, record: &Path) -> Option<serde_json::Value> {
        #[cfg(test)]
        if let Some(test) = &mut self.test
            && let Some(probe) = &mut test.probe
        {
            return probe(self.child.as_ref().map(std::process::Child::id));
        }
        // Direct, sequential, bounded socket probes; there are no nested workers
        // or socket joins left behind when startup is cancelled.
        crate::viewer::running(record, self.port)
    }

    fn poll(&mut self) -> std::io::Result<Option<std::process::ExitStatus>> {
        #[cfg(test)]
        let result = self.operation("poll");
        #[cfg(not(test))]
        let result = Ok(());
        let result = result.and_then(|()| {
            self.child
                .as_mut()
                .map_or(Ok(None), |child| child.try_wait())
        });
        if result
            .as_ref()
            .is_err_and(|error| error.raw_os_error() == Some(libc::ECHILD))
        {
            self.child_identity_lost = true;
        }
        result
    }

    fn cleanup(&mut self) -> std::result::Result<(), CleanupFailure> {
        let Some(pid) = self.child.as_ref().map(std::process::Child::id) else {
            return Ok(());
        };
        if self.child_identity_lost {
            return Err(CleanupFailure {
                pid,
                kill: None,
                wait: std::io::Error::from_raw_os_error(libc::ECHILD),
            });
        }
        let killed = self.kill();
        // Never perform a blocking wait after a failed kill. A failed signal
        // alone says nothing about whether the child ended.
        let waited = if killed.is_ok() {
            self.wait()
        } else {
            self.poll().and_then(|status| {
                status.ok_or_else(|| std::io::Error::other("The child still runs"))
            })
        };
        match waited {
            Ok(_) => {
                self.child.take();
                Ok(())
            }
            Err(wait) => Err(CleanupFailure {
                pid,
                kill: killed.err(),
                wait,
            }),
        }
    }

    fn kill(&mut self) -> std::io::Result<()> {
        #[cfg(test)]
        self.operation("kill")?;
        self.child.as_mut().map_or(Ok(()), |child| child.kill())
    }

    fn wait(&mut self) -> std::io::Result<std::process::ExitStatus> {
        #[cfg(test)]
        let result = self.operation("wait");
        #[cfg(not(test))]
        let result = Ok(());
        let result = result.and_then(|()| match &mut self.child {
            Some(child) => child.wait(),
            None => Err(std::io::Error::other("The child is not owned")),
        });
        if result
            .as_ref()
            .is_err_and(|error| error.raw_os_error() == Some(libc::ECHILD))
        {
            self.child_identity_lost = true;
        }
        result
    }

    /// Hold the menu bar's start lock, waiting up to 30 seconds for another start.
    fn starting(&mut self, ready: &Ready) -> Result<crate::files::LockFile> {
        use fs2::FileExt;
        use std::os::unix::fs::OpenOptionsExt;

        let lock = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .mode(0o600)
            .open(self.output.join("menubar.lock"))?;

        for _ in 0..300 {
            self.check(ready, "waiting for lock")?;
            if lock.try_lock_exclusive().is_ok() {
                return Ok(crate::files::LockFile(lock));
            }
            #[cfg(test)]
            self.operation("lock contention")?;
            std::thread::sleep(Duration::from_millis(100));
        }

        Err(Error::new(
            Code::Busy,
            "Another menu bar start is still running",
        ))
    }

    #[cfg(test)]
    fn operation(&mut self, point: &str) -> std::io::Result<()> {
        self.test
            .as_mut()
            .map_or(Ok(()), |test| (test.operation)(point))
    }
}

struct CleanupFailure {
    pid: u32,
    kill: Option<std::io::Error>,
    wait: std::io::Error,
}

struct StartupFailure {
    primary: Error,
    cleanup: CleanupFailure,
}

impl std::fmt::Debug for StartupFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StartupFailure")
            .field("pid", &self.cleanup.pid)
            .field("kill_failed", &self.cleanup.kill.is_some())
            .field("wait_kind", &self.cleanup.wait.kind())
            .finish_non_exhaustive()
    }
}

impl std::fmt::Display for StartupFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Termination of menu bar process {} is unknown",
            self.cleanup.pid
        )
    }
}

impl std::error::Error for StartupFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.primary)
    }
}

fn startup_cancelled() -> Error {
    Error::new(Code::Menubar, "The menu bar start was cancelled")
}

/// The last JSON reply the serving process wrote to its log.
fn last_reply(log: &Path) -> Option<serde_json::Value> {
    let said = fs::read_to_string(log).ok()?;
    let start = said
        .rfind("\n{")
        .map(|at| at + 1)
        .or_else(|| said.find('{'))?;

    serde_json::from_str(said[start..].trim()).ok()
}

/// Stop a running menu bar (as `uninstall` and `update` must): the port it
/// was serving, so it can be started again there. Only the process its
/// viewer answers as, which must be the one its record names and Atlas
/// itself, is signalled; a stale record naming anything else is not.
pub fn stop(output: &Path) -> Option<u16> {
    let record = output.join(crate::viewer::MENUBAR_RECORD);
    let port = fs::read(&record)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
        .and_then(|record| {
            let url = record["url"]
                .as_str()?
                .strip_prefix("http://127.0.0.1:")?
                .to_owned();
            url.split('/').next()?.parse::<u16>().ok()
        })?;

    let running = crate::viewer::running(&record, port)?;
    let pid = running["pid"]
        .as_i64()
        .and_then(|pid| i32::try_from(pid).ok())?;
    if !is_atlas(pid) {
        return None;
    }

    let pid = nix::unistd::Pid::from_raw(pid);
    nix::sys::signal::kill(pid, nix::sys::signal::Signal::SIGINT).ok()?;

    // It finishes what changes the helper first (a Recover can take most
    // of a minute), and keeps its port until then.
    let deadline = std::time::Instant::now() + STOP_WAIT;
    while nix::sys::signal::kill(pid, None).is_ok() && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(100));
    }

    Some(port)
}

/// How long [`stop`] waits for the menu bar to exit: 60 seconds, beyond a
/// Recover's worst case of about 50, which it lets finish.
const STOP_WAIT: Duration = Duration::from_secs(60);

/// Whether process `pid` runs an executable named like this one (`atlas`).
pub(crate) fn is_atlas(pid: i32) -> bool {
    let Some(name) = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.file_name().map(|name| name.to_owned()))
    else {
        return false;
    };

    std::process::Command::new("/bin/ps")
        .args(["-o", "comm=", "-p", &pid.to_string()])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()
        .is_some_and(|output| {
            let command = String::from_utf8_lossy(&output.stdout);
            Path::new(command.trim()).file_name() == Some(name.as_os_str())
        })
}

/// The app shipped beside `executable` in Atlas.app, if this is the
/// bundle's `atlas` and the app is there as a regular file.
fn bundled(executable: &Path) -> Option<PathBuf> {
    let directory = executable.parent()?;
    let app = directory.join(APP_EXECUTABLE);
    let regular = fs::symlink_metadata(&app).is_ok_and(|metadata| metadata.is_file());
    (directory.ends_with("Contents/MacOS") && regular).then_some(app)
}

/// The package in the checkout this binary was built from, built with SwiftPM.
fn development() -> Result<PathBuf> {
    let package = Path::new(env!("CARGO_MANIFEST_DIR")).join("app");
    if !package.join("Package.swift").is_file() {
        return Err(Error::new(
            Code::Menubar,
            format!(
                "Run the menu bar from Atlas.app; install it with: {}",
                crate::update::INSTALL_COMMAND
            ),
        ));
    }

    let swift = |args: &[&str]| {
        std::process::Command::new("/usr/bin/swift")
            .args(args)
            .arg("--package-path")
            .arg(&package)
            .stdin(Stdio::null())
            .output()
            .map_err(|_| {
                Error::new(
                    Code::Menubar,
                    "Building the menu bar needs the Xcode Command Line Tools; run xcode-select --install",
                )
            })
    };

    let built = swift(&["build", "-c", "release", "--product", APP_EXECUTABLE])?;
    if !built.status.success() {
        return Err(Error::new(
            Code::Menubar,
            format!(
                "The menu bar did not build: {}",
                crate::ui::text::first_line(&built.stderr, 300)
                    .unwrap_or_else(|| "no output".to_owned())
            ),
        ));
    }

    let bin = swift(&["build", "-c", "release", "--show-bin-path"])?;
    let directory = String::from_utf8_lossy(&bin.stdout).trim().to_owned();

    Ok(Path::new(&directory).join(APP_EXECUTABLE))
}

/// The app's executable: the signed one shipped in Atlas.app, or in a
/// development checkout the package built here.
pub fn executable() -> Result<PathBuf> {
    if let Some(app) = std::env::current_exe()
        .and_then(fs::canonicalize)
        .ok()
        .as_deref()
        .and_then(bundled)
    {
        // It receives the viewer's private URL: run only the app Atlas
        // signed, never a file merely placed there.
        if !crate::setup::app_signed(&app) {
            return Err(Error::new(
                Code::Menubar,
                "The app inside Atlas.app is not signed by Atlas",
            ));
        }

        return Ok(app);
    }

    development()
}

/// What to do with "Open at Login".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Login {
    On,
    Off,
    Status,
}

impl Login {
    /// The state as the command line and the app name it.
    pub(crate) fn parse(state: &str) -> Result<Self> {
        match state {
            "on" => Ok(Self::On),
            "off" => Ok(Self::Off),
            "status" => Ok(Self::Status),
            _ => Err(Error::new(
                Code::Usage,
                "Open at Login takes on, off or status",
            )),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::On => "on",
            Self::Off => "off",
            Self::Status => "status",
        }
    }
}

/// Turn "Open at Login" `on` or `off`, or report its `status`, as the
/// command line names them.
pub fn login(state: &str) -> Result<serde_json::Value> {
    set_login(Login::parse(state)?)
}

/// Apply `login`; only the app (as part of Atlas.app) can register its
/// login agent.
pub(crate) fn set_login(login: Login) -> Result<serde_json::Value> {
    let output = std::process::Command::new(executable()?)
        .args(["--login", login.as_str()])
        .stdin(Stdio::null())
        .output()?;
    let reply: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|_| Error::new(Code::Menubar, "The menu bar did not report its login item"))?;

    // Turning it on or off is the person's choice, and init keeps it.
    if login != Login::Status && reply["ok"] == true {
        decide_login();
    }

    Ok(reply)
}

/// Where the decision about opening at login is remembered.
fn login_record() -> Option<PathBuf> {
    crate::config::home()
        .ok()
        .map(|home| home.join("Library/Application Support/Atlas/menubar-login"))
}

/// Whether opening the menu bar at login was decided (by init or the person).
pub fn login_decided() -> bool {
    login_record().is_some_and(|path| path.exists())
}

/// Remember that opening at login was decided.
pub fn decide_login() {
    if let Some(path) = login_record() {
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = fs::write(path, b"decided\n");
    }
}

/// Forget that opening at login was decided, so setting Atlas up again
/// decides it afresh.
pub(crate) fn forget_login() {
    if let Some(path) = login_record() {
        let _ = fs::remove_file(path);
    }
}

/// The app to run, and the configuration its commands must use.
pub struct Program {
    pub(crate) executable: PathBuf,
    pub(crate) config: PathBuf,
}

impl Program {
    /// The app for the configuration at `config`.
    pub fn new(config: &Path) -> Result<Self> {
        Ok(Self {
            executable: executable()?,
            config: fs::canonicalize(config)?,
        })
    }
}

/// Start the app with `args` and hand it the URL on stdin. Its commands
/// run this very `atlas` with this configuration. Closing stdin asks the
/// app to quit by itself; dropping the returned child kills it outright
/// (`kill_on_drop`). The login job's name stays here: an `atlas` the app
/// runs is never the login agent ([`started_at_login`]).
async fn spawn(program: &Program, url: &str, args: &[&str]) -> Result<(Child, ChildStdin)> {
    let cli = std::env::current_exe().and_then(fs::canonicalize)?;
    let mut child = Command::new(&program.executable)
        .args(args)
        .env_remove("XPC_SERVICE_NAME")
        .env("ATLAS_CLI", cli)
        .env("ATLAS_CONFIG", &program.config)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .kill_on_drop(true)
        .spawn()?;

    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| Error::new(Code::Menubar, "The menu bar has no input"))?;
    stdin.write_all(format!("{url}\n").as_bytes()).await?;
    stdin.flush().await?;

    Ok((child, stdin))
}

/// A line the service sends the app on its stdin.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Line {
    /// Bring the viewer window forward.
    Viewer,
}

impl Line {
    fn text(self) -> &'static str {
        match self {
            Self::Viewer => "viewer\n",
        }
    }
}

/// The running app. Dropping it stops the app.
pub(crate) struct App {
    child: Child,
    writer: tokio::task::JoinHandle<()>,
}

impl App {
    /// Start the app; each of `lines` is passed on to it.
    pub(crate) async fn launch(
        program: &Program,
        url: &str,
        mut lines: tokio::sync::mpsc::UnboundedReceiver<Line>,
    ) -> Result<Self> {
        let (child, mut stdin) = spawn(program, url, &[]).await?;
        let writer = tokio::spawn(async move {
            while let Some(line) = lines.recv().await {
                if stdin.write_all(line.text().as_bytes()).await.is_err()
                    || stdin.flush().await.is_err()
                {
                    // The app is gone: later lines are refused, not kept.
                    return;
                }
            }

            // Its stdin stays open until the app is dropped.
            std::future::pending::<()>().await;
        });

        Ok(Self { child, writer })
    }

    /// Resolves when the person quits the app.
    pub(crate) async fn exited(&mut self) {
        let _ = self.child.wait().await;
    }
}

impl Drop for App {
    fn drop(&mut self) {
        // The aborted writer closes stdin once the runtime drops it, but
        // the app does not wait for that: its child handle drops right
        // after this, and `kill_on_drop` kills it then.
        self.writer.abort();
    }
}

/// The arguments that render the panel to `path` in one appearance.
fn render_args(path: &str, dark: bool) -> Vec<&str> {
    let mut args = vec!["--render", path];

    if dark {
        args.push("--dark");
    }

    args
}

/// Draw the app's panel, fed by the viewer at `url`, to a PNG.
pub(crate) async fn render(program: &Program, url: &str, path: &Path, dark: bool) -> Result<()> {
    let target = path
        .to_str()
        .ok_or_else(|| Error::new(Code::Usage, "The render path must be UTF-8"))?;
    let (mut child, _stdin) = spawn(program, url, &render_args(target, dark)).await?;
    let status = tokio::time::timeout(Duration::from_secs(30), child.wait())
        .await
        .map_err(|_| Error::new(Code::Menubar, "The menu bar did not finish rendering"))??;
    if !status.success() {
        return Err(Error::new(
            Code::Menubar,
            format!("The menu bar could not render ({status})"),
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
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
        owned: Cell<Option<crate::process::Instance>>,
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

        async fn started(&self) -> crate::process::Instance {
            self.wait_for("started").await;
            let pid = fs::read_to_string(self.directory.path().join("pid"))
                .unwrap()
                .parse()
                .unwrap();
            let info = crate::process::info(pid).unwrap().unwrap();
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
                                crate::process::info(instance.pid)
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
            if crate::process::info(instance.pid)
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

    async fn wait_for_start_lock(output: &Path) -> crate::files::LockFile {
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                let lock = fs::OpenOptions::new()
                    .create(true)
                    .truncate(false)
                    .write(true)
                    .open(output.join("menubar.lock"))
                    .unwrap();
                if fs2::FileExt::try_lock_exclusive(&lock).is_ok() {
                    return crate::files::LockFile(lock);
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
            assert!(crate::process::info(identity.pid).unwrap().is_some());
            if accept {
                acceptance.send(()).unwrap();
            }
            drop(acceptance);
            worker.await.unwrap();
            let _lock = wait_for_start_lock(recording.directory.path()).await;
            if accept {
                assert_eq!(
                    crate::process::info(identity.pid)
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
            assert!(crate::process::info(identity.pid).unwrap().is_some());
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
                    let body =
                        serde_json::json!({"ok":true,"service":"atlas-viewer","pid":answering})
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
            let mut command = Command::from(crate::test_support::rerun(test));
            let output = tokio::time::timeout(
                Duration::from_secs(30),
                command
                    .env("ATLAS_LAUNCHER_TEST", test)
                    .env("XPC_SERVICE_NAME", LOGIN_JOB)
                    .env("ATLAS_CLI", "inherited-cli")
                    .env("ATLAS_CONFIG", "inherited-config")
                    .kill_on_drop(true)
                    .output(),
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
        let (mut child, stdin) = spawn(&recording.program, &url, &args).await.unwrap();
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
            let (mut child, mut stdin) =
                spawn(&recording.program, "private-url", &[]).await.unwrap();
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
                let launching = spawn(&recording.program, &url, &[]);
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
            let (child, stdin) = spawn(&recording.program, "private-url", &[]).await.unwrap();
            recording.started().await;
            recording.wait_for("ready").await;
            let (mut unrelated, unrelated_stdin) = spawn(&other.program, "other-private-url", &[])
                .await
                .unwrap();
            let identity = other.started().await;
            other.wait_for("ready").await;

            drop(child);
            recording.reaped().await;
            assert_eq!(
                crate::process::info(identity.pid)
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
    fn the_bundle_ships_the_app_beside_atlas() {
        let dir = tempfile::tempdir().unwrap();
        let macos = dir.path().join("Atlas.app/Contents/MacOS");
        fs::create_dir_all(&macos).unwrap();
        let executable = macos.join("atlas");
        assert_eq!(bundled(&executable), None, "no app yet");
        fs::write(macos.join(APP_EXECUTABLE), b"").unwrap();
        assert_eq!(bundled(&executable), Some(macos.join(APP_EXECUTABLE)));

        let loose = dir.path().join("bin");
        fs::create_dir_all(&loose).unwrap();
        fs::write(loose.join(APP_EXECUTABLE), b"").unwrap();
        assert_eq!(bundled(&loose.join("atlas")), None, "only inside the app");

        fs::remove_file(macos.join(APP_EXECUTABLE)).unwrap();
        std::os::unix::fs::symlink(loose.join(APP_EXECUTABLE), macos.join(APP_EXECUTABLE)).unwrap();
        assert_eq!(bundled(&executable), None, "never through a link");
        assert!(
            !crate::setup::app_signed(&loose.join(APP_EXECUTABLE)),
            "an unsigned file is not the app"
        );

        let installed = Path::new("/Applications/Atlas.app/Contents/MacOS").join(APP_EXECUTABLE);
        if installed.exists() {
            assert!(crate::setup::app_signed(&installed));
        }
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
    fn rendering_selects_the_appearance() {
        assert_eq!(render_args("/tmp/a.png", false), ["--render", "/tmp/a.png"]);
        assert_eq!(
            render_args("/tmp/a.png", true),
            ["--render", "/tmp/a.png", "--dark"]
        );
    }
}

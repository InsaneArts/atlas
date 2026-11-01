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

/// Start the menu bar in its own session, so it keeps running after this
/// terminal closes, and report once it is up. It quits from its own menu.
pub async fn start(config: &Path, port: u16, output: &Path) -> Result<()> {
    let (already, record) = launch(config, port, output).await?;

    if crate::ui::human() {
        let what = if already {
            "is already running"
        } else {
            "is running"
        };

        println!(
            "{} The menu bar {what}. Click Atlas at the top of the screen; quit it from its menu.",
            crate::ui::style::ok()
        );
    } else {
        println!(
            "{}",
            serde_json::json!({"ok": true, "running": true, "alreadyRunning": already, "pid": record["pid"]})
        );
    }

    Ok(())
}

/// [`crate::viewer::running`] off the async workers: it waits on a socket.
async fn viewer_running(record: &Path, port: u16) -> Option<serde_json::Value> {
    let record = record.to_owned();
    tokio::task::spawn_blocking(move || crate::viewer::running(&record, port))
        .await
        .ok()
        .flatten()
}

/// Start the menu bar unless it runs already: whether it did, and its
/// private record (pid and viewer URL). One start runs at a time; a start
/// that fails is stopped, never left half-running.
pub async fn launch(config: &Path, port: u16, output: &Path) -> Result<(bool, serde_json::Value)> {
    let record = output.join(crate::viewer::MENUBAR_RECORD);
    let _starting = starting(&output.join("menubar.lock")).await?;

    if let Some(running) = viewer_running(&record, port).await {
        return Ok((true, running));
    }

    let log_path = output.join("menubar.log");
    let log = fs::File::create(&log_path)?;
    let mut child = std::process::Command::new(std::env::current_exe()?)
        .arg("menubar")
        .arg("--serve")
        .arg("--port")
        .arg(port.to_string())
        .arg("--config")
        .arg(fs::canonicalize(config)?)
        .env("ATLAS_JSON", "1")
        .stdin(Stdio::null())
        .stdout(log.try_clone()?)
        .stderr(log)
        .spawn()?;

    let pid = child.id();
    let deadline = std::time::Instant::now() + Duration::from_secs(20);

    loop {
        if let Some(status) = child.try_wait()? {
            let reply = last_reply(&log_path).unwrap_or_default();
            let already = status.success() && reply["alreadyRunning"] == true;
            let code = reply["code"]
                .as_str()
                .and_then(crate::error::Code::parse)
                .unwrap_or(Code::Menubar);

            // Another menu bar holds the port (the one that just stopped said
            // so, or the login item took it a moment before its record
            // appeared): give its record a few seconds.
            if already || code == "port-in-use" {
                for _ in 0..25 {
                    if let Some(running) = viewer_running(&record, port).await {
                        return Ok((true, running));
                    }
                    tokio::time::sleep(Duration::from_millis(200)).await;
                }
            }

            let reason = if already {
                format!("another menu bar holds port {port} but did not answer")
            } else {
                reply["error"]
                    .as_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| format!("it stopped; see {}", log_path.display()))
            };

            return Err(Error::new(
                code,
                format!("The menu bar did not start: {reason}"),
            ));
        }

        if let Some(running) = viewer_running(&record, port)
            .await
            .filter(|running| running["pid"] == pid)
        {
            return Ok((false, running));
        }

        if std::time::Instant::now() >= deadline {
            // Never leave a half-started menu bar behind.
            let _ = child.kill();
            let _ = child.wait();
            return Err(Error::new(
                Code::Menubar,
                format!(
                    "The menu bar did not start in time; see {}",
                    log_path.display()
                ),
            ));
        }

        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// Hold the menu bar's start lock, waiting up to 30 seconds for another start.
async fn starting(path: &Path) -> Result<crate::files::LockFile> {
    use fs2::FileExt;
    use std::os::unix::fs::OpenOptionsExt;

    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .mode(0o600)
        .open(path)?;

    for _ in 0..300 {
        if lock.try_lock_exclusive().is_ok() {
            return Ok(crate::files::LockFile(lock));
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    Err(Error::new(
        Code::Busy,
        "Another menu bar start is still running",
    ))
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
pub enum Login {
    On,
    Off,
    Status,
}

impl Login {
    /// The state as the command line and the app name it.
    pub fn parse(state: &str) -> Result<Self> {
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
pub fn set_login(login: Login) -> Result<serde_json::Value> {
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
pub fn forget_login() {
    if let Some(path) = login_record() {
        let _ = fs::remove_file(path);
    }
}

/// The app to run, and the configuration its commands must use.
pub struct Program {
    pub executable: PathBuf,
    pub config: PathBuf,
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
pub enum Line {
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
pub struct App {
    child: Child,
    writer: tokio::task::JoinHandle<()>,
}

impl App {
    /// Start the app; each of `lines` is passed on to it.
    pub async fn launch(
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
    pub async fn exited(&mut self) {
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
pub async fn render(program: &Program, url: &str, path: &Path, dark: bool) -> Result<()> {
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

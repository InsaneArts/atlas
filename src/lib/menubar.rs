//! The macOS app: the Swift package in app/, whose AtlasApp executable
//! draws the menu bar, the viewer and the setup window. Atlas.app ships it
//! signed beside this executable; a development checkout builds the package
//! with SwiftPM. It renders the local viewer's status and frame and
//! runs `atlas` commands; it reaches nothing else. The capability URL
//! travels on its stdin, never in argv, and closing stdin makes it quit.
use crate::error::{Error, Result};
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
            serde_json::to_string_pretty(
                &serde_json::json!({"ok": true, "running": true, "alreadyRunning": already, "pid": record["pid"]})
            )?
        );
    }
    Ok(())
}

/// Start the menu bar unless it runs already: whether it did, and its
/// private record (pid and viewer URL). One start runs at a time; a start
/// that fails is stopped, never left half-running.
pub async fn launch(config: &Path, port: u16, output: &Path) -> Result<(bool, serde_json::Value)> {
    let record = output.join(crate::viewer::MENUBAR_RECORD);
    let _starting = starting(&output.join("menubar.lock")).await?;
    if let Some(running) = crate::viewer::running(&record, port) {
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
                .and_then(crate::error::code)
                .unwrap_or("menubar");
            // Another menu bar holds the port (the one that just stopped said
            // so, or the login item took it a moment before its record
            // appeared): give its record a few seconds.
            if already || code == "port-in-use" {
                for _ in 0..25 {
                    if let Some(running) = crate::viewer::running(&record, port) {
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
        if let Some(running) =
            crate::viewer::running(&record, port).filter(|running| running["pid"] == pid)
        {
            return Ok((false, running));
        }
        if std::time::Instant::now() >= deadline {
            // Never leave a half-started menu bar behind.
            let _ = child.kill();
            let _ = child.wait();
            return Err(Error::new(
                "menubar",
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
async fn starting(path: &Path) -> Result<fs::File> {
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
            return Ok(lock);
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Err(Error::new(
        "busy",
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
/// was serving, so it can be started again there. Only a process that is
/// Atlas itself is signalled; a stale record naming anything else is not.
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
    for _ in 0..50 {
        if nix::sys::signal::kill(pid, None).is_err() {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    Some(port)
}

/// Whether process `pid` runs an executable named like this one (`atlas`).
fn is_atlas(pid: i32) -> bool {
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
            "menubar",
            "Run the menu bar from Atlas.app; install it with install.sh",
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
                    "menubar",
                    "Building the menu bar needs the Xcode Command Line Tools; run xcode-select --install",
                )
            })
    };
    let built = swift(&["build", "-c", "release", "--product", APP_EXECUTABLE])?;
    if !built.status.success() {
        return Err(Error::new(
            "menubar",
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
                "menubar",
                "The app inside Atlas.app is not signed by Atlas",
            ));
        }
        return Ok(app);
    }
    development()
}

/// Turn "Open at Login" `on` or `off`, or report its `status`; only the
/// app (as part of Atlas.app) can register its login agent.
pub fn login(state: &str) -> Result<serde_json::Value> {
    let output = std::process::Command::new(executable()?)
        .args(["--login", state])
        .stdin(Stdio::null())
        .output()?;
    let reply: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|_| Error::new("menubar", "The menu bar did not report its login item"))?;
    // Turning it on or off is the person's choice, and init keeps it.
    if state != "status" && reply["ok"] == true {
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
/// run this very `atlas` with this configuration.
async fn spawn(program: &Program, url: &str, args: &[&str]) -> Result<(Child, ChildStdin)> {
    let cli = std::env::current_exe().and_then(fs::canonicalize)?;
    let mut child = Command::new(&program.executable)
        .args(args)
        .env("ATLAS_CLI", cli)
        .env("ATLAS_CONFIG", &program.config)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .kill_on_drop(true)
        .spawn()?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| Error::new("menubar", "The menu bar has no input"))?;
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
        // Closes its stdin; the child is killed as it drops.
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
        .ok_or_else(|| Error::new("usage", "The render path must be UTF-8"))?;
    let (mut child, _stdin) = spawn(program, url, &render_args(target, dark)).await?;
    let status = tokio::time::timeout(Duration::from_secs(30), child.wait())
        .await
        .map_err(|_| Error::new("menubar", "The menu bar did not finish rendering"))??;
    if !status.success() {
        return Err(Error::new(
            "menubar",
            format!("The menu bar could not render ({status})"),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_url_arrives_on_stdin_never_in_argv() {
        let main = include_str!("../../app/Sources/AtlasApp/AtlasApp.swift");
        assert!(
            main.contains("guard let line = readLine(), let service = ViewerClient(line)"),
            "the URL arrives on stdin"
        );
        assert!(!main.contains("ViewerClient(arguments"), "never from argv");
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
    fn rendering_selects_the_appearance() {
        assert_eq!(render_args("/tmp/a.png", false), ["--render", "/tmp/a.png"]);
        assert_eq!(
            render_args("/tmp/a.png", true),
            ["--render", "/tmp/a.png", "--dark"]
        );
    }
}

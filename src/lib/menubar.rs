//! The macOS menu bar companion: the Swift package in menubar/. Understudy.app
//! ships it signed beside this executable; a development checkout builds the
//! package with SwiftPM. It renders the local viewer's status and frame and
//! runs `understudy` commands; it reaches nothing else. The capability URL
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

const NAME: &str = "UnderstudyBar";

/// The login agent that runs `understudy menubar` (see xtask); launchd names
/// the job in XPC_SERVICE_NAME.
const LOGIN_JOB: &str = "com.insanearts.understudy.menubar";

/// Whether this process is the menu bar's login agent, which serves the
/// menu bar itself rather than starting it.
pub fn started_at_login() -> bool {
    std::env::var("XPC_SERVICE_NAME").is_ok_and(|job| job == LOGIN_JOB)
}

/// Start the menu bar in its own session, so it keeps running after this
/// terminal closes, and report once it is up. It quits from its own menu.
pub async fn start(config: &Path, port: u16, record: &Path) -> Result<()> {
    let say = |already: bool, pid: serde_json::Value| -> Result<()> {
        if crate::ui::human() {
            let what = if already {
                "is already running"
            } else {
                "is running"
            };
            println!(
                "{} The menu bar {what}. Click the U at the top of the screen; quit it from its menu.",
                crate::ui::style::ok()
            );
        } else {
            println!(
                "{}",
                serde_json::to_string_pretty(
                    &serde_json::json!({"ok": true, "running": true, "alreadyRunning": already, "pid": pid})
                )?
            );
        }
        Ok(())
    };
    if let Some(running) = crate::viewer::running(record, port).filter(|r| r["menubar"] == true) {
        return say(true, running["pid"].clone());
    }
    let log_path = record.with_file_name("menubar.log");
    let log = fs::File::create(&log_path)?;
    let mut child = std::process::Command::new(std::env::current_exe()?)
        .arg("menubar")
        .arg("--serve")
        .arg("--port")
        .arg(port.to_string())
        .arg("--config")
        .arg(fs::canonicalize(config)?)
        .env("UNDERSTUDY_JSON", "1")
        .stdin(Stdio::null())
        .stdout(log.try_clone()?)
        .stderr(log)
        .spawn()?;
    let pid = child.id();
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    loop {
        if let Some(status) = child.try_wait()? {
            // It stopped: either another menu bar already holds the port
            // (a success), or it could not start and said why.
            let said = fs::read_to_string(&log_path).unwrap_or_default();
            let reply = said.rsplit("\n{").next().and_then(|tail| {
                let text = if tail.starts_with('{') {
                    tail.to_owned()
                } else {
                    format!("{{{tail}")
                };
                serde_json::from_str::<serde_json::Value>(&text).ok()
            });
            if status.success()
                && reply
                    .as_ref()
                    .is_some_and(|reply| reply["alreadyRunning"] == true)
            {
                return say(
                    true,
                    reply.map(|reply| reply["pid"].clone()).unwrap_or_default(),
                );
            }
            let reason = reply
                .as_ref()
                .and_then(|reply| reply["error"].as_str().map(str::to_owned))
                .unwrap_or_else(|| format!("it stopped; see {}", log_path.display()));
            let code = reply
                .as_ref()
                .and_then(|reply| reply["code"].as_str())
                .and_then(crate::error::code)
                .unwrap_or("menubar");
            return Err(Error::new(
                code,
                format!("The menu bar did not start: {reason}"),
            ));
        }
        let up = crate::viewer::running(record, port)
            .is_some_and(|running| running["pid"] == pid && running["menubar"] == true);
        if up {
            return say(false, pid.into());
        }
        if std::time::Instant::now() >= deadline {
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

/// The first non-empty line of compiler output, bounded for reports.
fn first_line(output: &[u8]) -> String {
    String::from_utf8_lossy(output)
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("no output")
        .chars()
        .take(300)
        .collect()
}

/// The companion shipped beside `executable` in Understudy.app, if this is
/// the app's executable and it is there as a regular file.
fn bundled(executable: &Path) -> Option<PathBuf> {
    let directory = executable.parent()?;
    let companion = directory.join(NAME);
    let regular = fs::symlink_metadata(&companion).is_ok_and(|metadata| metadata.is_file());
    (directory.ends_with("Contents/MacOS") && regular).then_some(companion)
}

/// The package in the checkout this binary was built from, built with SwiftPM.
fn development() -> Result<PathBuf> {
    let package = Path::new(env!("CARGO_MANIFEST_DIR")).join("menubar");
    if !package.join("Package.swift").is_file() {
        return Err(Error::new(
            "menubar",
            "Run the menu bar from Understudy.app; install it with install.sh",
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
    let built = swift(&["build", "-c", "release", "--product", NAME])?;
    if !built.status.success() {
        return Err(Error::new(
            "menubar",
            format!("The menu bar did not build: {}", first_line(&built.stderr)),
        ));
    }
    let bin = swift(&["build", "-c", "release", "--show-bin-path"])?;
    let directory = String::from_utf8_lossy(&bin.stdout).trim().to_owned();
    Ok(Path::new(&directory).join(NAME))
}

/// The companion executable: the signed one shipped in Understudy.app, or in
/// a development checkout the package built here.
pub fn executable() -> Result<PathBuf> {
    if let Some(companion) = std::env::current_exe()
        .and_then(fs::canonicalize)
        .ok()
        .as_deref()
        .and_then(bundled)
    {
        // It receives the viewer's private URL: run only the companion
        // Understudy signed, never a file merely placed there.
        if !crate::setup::companion_signed(&companion) {
            return Err(Error::new(
                "menubar",
                "The menu bar companion in Understudy.app is not signed by Understudy",
            ));
        }
        return Ok(companion);
    }
    development()
}

/// Turn "Open at Login" `on` or `off`, or report its `status`; only the
/// companion (as part of Understudy.app) can register its login agent.
pub fn login(state: &str) -> Result<serde_json::Value> {
    let output = std::process::Command::new(executable()?)
        .args(["--login", state])
        .stdin(Stdio::null())
        .output()?;
    serde_json::from_slice(&output.stdout)
        .map_err(|_| Error::new("menubar", "The menu bar did not report its login item"))
}

/// The companion to run, and the configuration its commands must use.
pub struct Program {
    pub executable: PathBuf,
    pub config: PathBuf,
}

impl Program {
    /// The companion for the configuration at `config`.
    pub fn new(config: &Path) -> Result<Self> {
        Ok(Self {
            executable: executable()?,
            config: fs::canonicalize(config)?,
        })
    }
}

/// Start the companion with `args` and hand it the URL on stdin. Its
/// commands run this very `understudy` with this configuration.
async fn spawn(program: &Program, url: &str, args: &[&str]) -> Result<(Child, ChildStdin)> {
    let cli = std::env::current_exe().and_then(fs::canonicalize)?;
    let mut child = Command::new(&program.executable)
        .args(args)
        .env("UNDERSTUDY_CLI", cli)
        .env("UNDERSTUDY_CONFIG", &program.config)
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

/// A running companion. Dropping it closes its stdin, which makes it quit,
/// and kills it if it has not.
pub struct Companion {
    child: Child,
    _stdin: ChildStdin,
}

impl Companion {
    pub async fn launch(program: &Program, url: &str) -> Result<Self> {
        let (child, stdin) = spawn(program, url, &[]).await?;
        Ok(Self {
            child,
            _stdin: stdin,
        })
    }
    /// Resolves when the person quits the companion.
    pub async fn exited(&mut self) {
        let _ = self.child.wait().await;
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

/// Draw the companion's panel, fed by the viewer at `url`, to a PNG.
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
        let main = include_str!("../../menubar/Sources/UnderstudyBar/main.swift");
        assert!(
            main.contains("guard let line = readLine(), let viewer = Viewer(line)"),
            "the URL arrives on stdin"
        );
        assert!(!main.contains("Viewer(CommandLine"), "never from argv");
    }

    #[test]
    fn the_app_ships_its_companion_beside_the_executable() {
        let dir = tempfile::tempdir().unwrap();
        let macos = dir.path().join("Understudy.app/Contents/MacOS");
        fs::create_dir_all(&macos).unwrap();
        let executable = macos.join("understudy");
        assert_eq!(bundled(&executable), None, "no companion yet");
        fs::write(macos.join(NAME), b"").unwrap();
        assert_eq!(bundled(&executable), Some(macos.join(NAME)));
        let loose = dir.path().join("bin");
        fs::create_dir_all(&loose).unwrap();
        fs::write(loose.join(NAME), b"").unwrap();
        assert_eq!(
            bundled(&loose.join("understudy")),
            None,
            "only inside the app"
        );
        fs::remove_file(macos.join(NAME)).unwrap();
        std::os::unix::fs::symlink(loose.join(NAME), macos.join(NAME)).unwrap();
        assert_eq!(bundled(&executable), None, "never through a link");
        assert!(
            !crate::setup::companion_signed(&loose.join(NAME)),
            "an unsigned file is not the companion"
        );
        let installed = Path::new("/Applications/Understudy.app/Contents/MacOS").join(NAME);
        if installed.exists() {
            assert!(crate::setup::companion_signed(&installed));
        }
    }

    #[test]
    fn rendering_selects_the_appearance() {
        assert_eq!(render_args("/tmp/a.png", false), ["--render", "/tmp/a.png"]);
        assert_eq!(
            render_args("/tmp/a.png", true),
            ["--render", "/tmp/a.png", "--dark"]
        );
        assert_eq!(first_line(b"\n\nerror: x\nmore"), "error: x");
    }
}

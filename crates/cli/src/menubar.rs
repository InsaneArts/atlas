//! The macOS app: the Swift package in app/, whose AtlasApp executable
//! draws the menu bar, the viewer and the setup window. Atlas.app ships it
//! signed beside this executable; a development checkout builds the package
//! with SwiftPM. It renders the local viewer's status and frame and
//! runs `atlas` commands; it reaches nothing else. The capability URL
//! travels on its stdin, never in argv, and closing stdin makes it quit.
use atlas_core::{
    app_bundle::{APP_EXECUTABLE, INSTALL_COMMAND, LOGIN_JOB, checkout, shipped_app},
    error::{Code, Error, Result},
    name::first_line,
};
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

/// The port the menu bar serves unless told another.
pub const PORT: u16 = 8765;

/// Whether this process is the menu bar's login agent, which serves the
/// menu bar itself rather than starting it.
pub fn started_at_login() -> bool {
    std::env::var("XPC_SERVICE_NAME").is_ok_and(|job| job == LOGIN_JOB)
}

/// Hold the place of the menu bar whose records `output` keeps, for as
/// long as it serves: one menu bar serves them, on whatever port. Its
/// record is only read, to name the one that holds it.
///
/// # Errors
///
/// Returns a menu bar error naming the one that serves already, or the
/// failure to open or lock its lock file.
pub(crate) fn alone(output: &Path) -> Result<atlas_core::files::LockFile> {
    let lock = atlas_core::files::open_lock(&output.join("menubar-serve.lock"))?;
    match fs2::FileExt::try_lock_exclusive(&lock.0) {
        Ok(()) => return Ok(lock),
        Err(error) if error.kind() != std::io::ErrorKind::WouldBlock => return Err(error.into()),
        Err(_) => {}
    }

    let record = crate::viewer::read_record(&output.join(crate::viewer::MENUBAR_RECORD));
    let named = record.as_ref().and_then(|record| {
        Some((
            crate::viewer::process(record)?,
            crate::viewer::recorded_port(record)?,
        ))
    });
    Err(Error::new(
        Code::Menubar,
        named.map_or_else(
            || "Another menu bar already runs".to_owned(),
            |(pid, port)| format!("The menu bar already runs as process {pid} on port {port}"),
        ),
    )
    .with(
        "next",
        serde_json::json!(atlas_core::error::advice::MENUBAR_RUNNING),
    ))
}

/// Start the menu bar unless it runs already: whether it did, and its
/// private record (pid and viewer URL). One start runs at a time; a start
/// that fails is stopped, never left half-running.
///
/// # Errors
///
/// Returns configuration, lock, spawn, readiness or cleanup failures. Cancellation keeps
/// the startup worker responsible for its child; unknown termination retains exclusion.
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
    lock: Option<atlas_core::files::LockFile>,
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
        self.child = Some(atlas_core::spawn::spawn(
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
                .stderr(log),
        )?);
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
                .filter(|running| crate::viewer::process(running) == pid)
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
        // Only these fields of the serving process's last reply decide a
        // failed start; one of another shape says nothing.
        let reply = last_reply(log).unwrap_or_default();
        let code = reply["code"]
            .as_str()
            .and_then(Code::parse)
            .unwrap_or(Code::Menubar);
        let already = status.success() && reply["alreadyRunning"] == true;

        // A competing menu bar can hold the port before its record appears.
        if already || code == Code::PortInUse {
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
            reply["error"].as_str().map_or_else(
                || format!("it stopped; see {}", log.display()),
                str::to_owned,
            )
        };
        Err(Error::new(
            code,
            format!("The menu bar did not start: {reason}"),
        ))
    }

    fn check(&mut self, ready: &Ready, point: &str) -> Result<()> {
        self.operation(point)?;
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
        let result = self.operation("poll").and_then(|()| {
            self.child
                .as_mut()
                .map_or(Ok(None), |child| child.try_wait())
        });
        self.noting_lost_child(result)
    }

    /// `result`, remembering a child that can no longer be told from
    /// whatever reuses its process ID.
    fn noting_lost_child<T>(&mut self, result: std::io::Result<T>) -> std::io::Result<T> {
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
        self.operation("kill")?;
        self.child.as_mut().map_or(Ok(()), |child| child.kill())
    }

    fn wait(&mut self) -> std::io::Result<std::process::ExitStatus> {
        let result = self.operation("wait").and_then(|()| match &mut self.child {
            Some(child) => child.wait(),
            None => Err(std::io::Error::other("The child is not owned")),
        });
        self.noting_lost_child(result)
    }

    /// Hold the menu bar's start lock, waiting up to 30 seconds for another start.
    fn starting(&mut self, ready: &Ready) -> Result<atlas_core::files::LockFile> {
        use fs2::FileExt;
        use std::os::unix::fs::OpenOptionsExt;

        let lock = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .mode(0o600)
            .open(self.output.join(START_LOCK))?;

        let refused = start_refused();
        let mut last = None;
        for _ in 0..300 {
            self.check(ready, "waiting for lock")?;
            match lock.try_lock_exclusive() {
                Ok(()) => return Ok(atlas_core::files::LockFile(lock)),
                Err(error) => last = Some(error),
            }
            self.operation("lock contention")?;
            std::thread::sleep(Duration::from_millis(100));
        }

        Err(match last {
            Some(cause) => refused.with_source(cause),
            None => refused,
        })
    }

    /// A step tests can fail or hold; nothing in a shipping build.
    fn operation(&mut self, _point: &str) -> std::io::Result<()> {
        #[cfg(test)]
        if let Some(test) = &mut self.test {
            return (test.operation)(_point);
        }
        Ok(())
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

/// The lock one start of the menu bar holds at a time, and how long
/// another waits for it.
const START_LOCK: &str = "menubar.lock";
const START_WAIT: Duration = Duration::from_secs(30);

fn start_refused() -> Error {
    Error::new(Code::Busy, "Another menu bar start is still running")
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

/// Stop every running menu bar (as `uninstall` and `update` must): the
/// port it was serving, so it can be started again there. The one its
/// record names is signalled only when its viewer answers as it and it is
/// Atlas; a stale record naming anything else is not. Any other process of
/// this account that runs `atlas menubar` to serve it is a menu bar too,
/// one whose record was lost or replaced. None only when no menu bar runs.
///
/// # Errors
///
/// Returns a menu bar error when its record, this account's processes or a
/// command line can't be read, the recorded one runs but does not answer,
/// one can't be signalled, or one has not been seen to exit by the
/// deadline. It may still run then, and its record stays.
pub fn stop(output: &Path) -> Result<Option<u16>> {
    // A start under way finishes first, so the menu bar it starts is found,
    // and no other starts while they are found and stopped.
    let starting = atlas_core::files::open_lock(&output.join(START_LOCK))?;
    atlas_core::files::lock_within(&starting, atlas_core::files::Lock::Exclusive, START_WAIT)
        .map_err(|held| start_refused().with_source(held.0))?;

    let record = output.join(crate::viewer::MENUBAR_RECORD);
    let recorded = serving(
        &record,
        |record| fs::read(record),
        atlas_core::process::info,
        crate::viewer::answering,
        atlas_executable,
    )?;
    let exe = std::env::current_exe()?;
    let me = i32::try_from(std::process::id()).unwrap_or(-1);
    let unrecorded = menu_bars(
        listed(),
        me,
        exe.file_name().unwrap_or_default(),
        |process| match atlas_core::process::arguments(process.instance.pid) {
            Ok(arguments) => Ok(Some(arguments)),
            Err(_) if process.instance.ended() => Ok(None),
            Err(error) => Err(error),
        },
    )?;

    // Each finishes what changes the helper first (a Recover can take most
    // of a minute), and keeps its port until then. The recorded one, also
    // listed, is not signalled twice: once gone, it is not signalled again.
    let deadline = std::time::Instant::now() + STOP_WAIT;
    let mut port = None;
    for (serving, instance) in recorded.into_iter().chain(unrecorded) {
        let stopped = stopped(
            serving,
            || instance.state(),
            || {
                nix::sys::signal::kill(
                    nix::unistd::Pid::from_raw(instance.pid),
                    nix::sys::signal::Signal::SIGINT,
                )
            },
            || {
                std::thread::sleep(Duration::from_millis(100));
                std::time::Instant::now() < deadline
            },
        )?;
        port = port.or(stopped);
    }
    Ok(port)
}

/// [`stop`] the menu bar whose records `output` holds. When where they are
/// can't be told (`output` says why), a menu bar is known not to run only
/// when no other process of this account runs Atlas.
///
/// # Errors
///
/// Returns what [`stop`] returns, or a menu bar error when `output` failed
/// and this account's processes can't be listed or another runs Atlas.
pub fn stop_located(output: Result<PathBuf>) -> Result<Option<u16>> {
    match output {
        Ok(output) => stop(&output),
        Err(unlocated) => {
            let exe = std::env::current_exe()?;
            let name = exe.file_name().unwrap_or_default();
            let me = i32::try_from(std::process::id()).unwrap_or(-1);
            elsewhere(unlocated, listed(), me, name)
        }
    }
}

#[cfg(test)]
thread_local! {
    /// The processes [`listed`] gives on this thread, when a test says.
    pub(crate) static LISTED: std::cell::RefCell<Option<Vec<atlas_core::process::Info>>> =
        const { std::cell::RefCell::new(None) };
}

/// This account's processes.
fn listed() -> Result<Vec<atlas_core::process::Info>> {
    #[cfg(test)]
    if let Some(listed) = LISTED.with(|listed| listed.borrow().clone()) {
        return Ok(listed);
    }
    atlas_core::process::of_user(nix::unistd::getuid().as_raw())
}

/// With the menu bar's records not found (`unlocated` says why), whether
/// one may run: any of `processes` named `name`, as the kernel keeps it,
/// other than `me` and its ancestors and processes that have exited.
fn elsewhere(
    unlocated: Error,
    processes: Result<Vec<atlas_core::process::Info>>,
    me: i32,
    name: &std::ffi::OsStr,
) -> Result<Option<u16>> {
    let processes = processes.map_err(|error| undecided().with_source(error))?;

    let mut mine = vec![me];
    let mut at = me;
    while let Some(parent) = processes
        .iter()
        .find(|process| process.instance.pid == at)
        .map(|process| process.ppid)
        .filter(|parent| *parent > 1 && !mine.contains(parent))
    {
        mine.push(parent);
        at = parent;
    }

    match processes.iter().find(|process| {
        !process.zombie && !mine.contains(&process.instance.pid) && named(process, name)
    }) {
        None => Ok(None),
        Some(process) => Err(Error::new(
            Code::Menubar,
            format!(
                "Atlas can't find the menu bar's record while Atlas runs as process {}",
                process.instance.pid
            ),
        )
        .with_source(unlocated)),
    }
}

/// Whether `process` runs a command named `name`, as the kernel keeps it:
/// at most its first 16 bytes.
fn named(process: &atlas_core::process::Info, name: &std::ffi::OsStr) -> bool {
    use std::os::unix::ffi::OsStrExt;
    let name = name.as_bytes();
    process.name.as_bytes() == name.get(..16).unwrap_or(name)
}

/// The menu bars among `processes`, and the port each serves: live ones
/// other than `me`, named `name`, whose command line as `arguments` reads
/// it (None once gone) runs `atlas menubar` to serve it. An ancestor of
/// `me` counts: the menu bar's app starts updates of its own.
fn menu_bars(
    processes: Result<Vec<atlas_core::process::Info>>,
    me: i32,
    name: &std::ffi::OsStr,
    mut arguments: impl FnMut(&atlas_core::process::Info) -> Result<Option<Vec<String>>>,
) -> Result<Vec<(u16, atlas_core::process::Instance)>> {
    let processes = processes.map_err(|error| undecided().with_source(error))?;
    let mut found = Vec::new();
    for process in processes
        .iter()
        .filter(|process| !process.zombie && process.instance.pid != me && named(process, name))
    {
        let arguments = arguments(process).map_err(|error| undecided().with_source(error))?;
        if let Some(port) = arguments.and_then(|arguments| menu_bar_port(&arguments, process.ppid))
        {
            found.push((port, process.instance));
        }
    }
    Ok(found)
}

/// The port served by the `atlas menubar` whose arguments are `arguments`
/// and whose parent is `parent`: one asked to `--serve`, or started at
/// login, as launchd's child. None for any other command, and for one that
/// renders or changes its login item.
fn menu_bar_port(arguments: &[String], parent: i32) -> Option<u16> {
    let mut rest = arguments.iter().skip(1).map(String::as_str);
    let (mut menubar, mut serve, mut port) = (false, false, PORT);
    while let Some(argument) = rest.next() {
        let (flag, value) = argument
            .split_once('=')
            .map_or((argument, None), |(flag, value)| (flag, Some(value)));
        match flag {
            "menubar" if !menubar && value.is_none() => menubar = true,
            "--serve" => serve = true,
            "--port" => port = value.or_else(|| rest.next())?.parse().ok()?,
            "--config" | "--wait" if value.is_none() => {
                rest.next();
            }
            "--config" | "--wait" | "--json" | "--allow-disrupt" => {}
            // Nothing may follow it: the menu bar takes no arguments.
            "--" if value.is_none() && rest.next().is_none() => {}
            _ => return None,
        }
    }
    (menubar && (serve || parent == 1)).then_some(port)
}

fn undecided() -> Error {
    Error::new(
        Code::Menubar,
        "Atlas could not tell whether the menu bar runs",
    )
}

/// The port and process of the menu bar `record` names, when it runs and
/// its viewer answers as it; None when it is known not to run. It reads the
/// record with `read`, the process with `info`, asks the viewer at the
/// record's address which process it is with `answering`, and whether a
/// process runs Atlas with `atlas`.
fn serving(
    record: &Path,
    read: impl FnOnce(&Path) -> std::io::Result<Vec<u8>>,
    info: impl FnOnce(i32) -> Result<Option<atlas_core::process::Info>>,
    answering: impl FnOnce(&str) -> Option<u32>,
    atlas: impl FnOnce(i32) -> std::io::Result<bool>,
) -> Result<Option<(u16, atlas_core::process::Instance)>> {
    let unreadable = || {
        Error::new(
            Code::Menubar,
            format!(
                "The menu bar's record, {}, can't be read",
                atlas_core::config::tilde(record)
            ),
        )
    };

    let bytes = match read(record) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(unreadable().with_source(error)),
    };
    let written: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|error| unreadable().with_source(error))?;
    let (Some(port), Some(address), Some(pid)) = (
        crate::viewer::recorded_port(&written),
        crate::viewer::address(&written),
        crate::viewer::process(&written).and_then(|pid| i32::try_from(pid).ok()),
    ) else {
        return Err(unreadable());
    };

    // The menu bar is this account's own, and never the process asking.
    let me = nix::unistd::getuid().as_raw();
    let Some(process) = info(pid).map_err(|error| undecided().with_source(error))? else {
        return Ok(None);
    };
    if process.zombie || !process.owned_by(me) || process.instance.is_this_one() {
        return Ok(None);
    }

    let answers = answering(address).is_some_and(|answered| u32::try_from(pid) == Ok(answered));
    match (answers, atlas(pid)) {
        (true, Ok(true)) => Ok(Some((port, process.instance))),
        (true, Ok(false)) => Err(Error::new(
            Code::Menubar,
            "The process serving the menu bar is not Atlas",
        )),
        // Its PID is another program's now.
        (false, Ok(false)) => Ok(None),
        (false, Ok(true)) => Err(Error::new(
            Code::Menubar,
            "The menu bar runs but does not answer",
        )),
        (_, Err(error)) => Err(undecided().with_source(error)),
    }
}

/// Ask the menu bar serving `port` to stop with `signal`, then `look` at
/// its process until it is gone, while `wait` allows: only a process seen
/// gone has stopped. One already gone is not signalled, and was not
/// running to be stopped.
fn stopped(
    port: u16,
    mut look: impl FnMut() -> atlas_core::process::State,
    signal: impl FnOnce() -> nix::Result<()>,
    mut wait: impl FnMut() -> bool,
) -> Result<Option<u16>> {
    use atlas_core::process::State;
    if look() == State::Gone {
        return Ok(None);
    }
    match signal() {
        Ok(()) | Err(nix::errno::Errno::ESRCH) => {}
        Err(errno) => {
            return Err(
                Error::new(Code::Menubar, "Atlas could not stop the menu bar").with_source(errno),
            );
        }
    }
    loop {
        if look() == State::Gone {
            return Ok(Some(port));
        }
        if !wait() {
            return Err(Error::new(
                Code::Menubar,
                "The menu bar did not stop within a minute",
            ));
        }
    }
}

/// How long [`stop`] waits for the menu bar to exit: 60 seconds, beyond a
/// Recover's worst case of about 50, which it lets finish.
const STOP_WAIT: Duration = Duration::from_secs(60);

/// Whether process `pid` runs an executable named like this one (`atlas`).
/// One that can't be read is not.
pub(crate) fn is_atlas(pid: i32) -> bool {
    atlas_executable(pid).unwrap_or(false)
}

/// Whether process `pid` runs an executable named like this one (`atlas`),
/// or the failure to tell, a process that has gone among them.
fn atlas_executable(pid: i32) -> std::io::Result<bool> {
    let exe = std::env::current_exe()?;
    let name = exe
        .file_name()
        .ok_or_else(|| std::io::Error::other("this executable has no name"))?;

    let output = atlas_core::spawn::spawn(
        std::process::Command::new("/bin/ps")
            .args(["-o", "comm=", "-p", &pid.to_string()])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null()),
    )
    .and_then(std::process::Child::wait_with_output)?;
    if !output.status.success() {
        return Err(std::io::Error::other("ps could not describe the process"));
    }
    let command = String::from_utf8_lossy(&output.stdout);
    Ok(Path::new(command.trim()).file_name() == Some(name))
}

/// What to do with "Open at Login".
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Login {
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
///
/// # Errors
///
/// Rejects an unsupported login state or returns failures inspecting or changing Atlas's
/// login item.
pub fn login(state: &str) -> Result<serde_json::Value> {
    set_login(Login::parse(state)?)
}

/// Apply `login`; only the app (as part of Atlas.app) can register its
/// login agent.
///
/// # Errors
///
/// Returns failures inspecting or changing Atlas's login item.
pub fn set_login(login: Login) -> Result<serde_json::Value> {
    let output = atlas_core::spawn::spawn(
        std::process::Command::new(app_program()?)
            .args(["--login", login.as_str()])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped()),
    )
    .and_then(std::process::Child::wait_with_output)?;
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
    atlas_core::config::home()
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
    ///
    /// # Errors
    ///
    /// Returns configuration, installed-app or executable-location failures while selecting a
    /// viewer program.
    pub fn new(config: &Path) -> Result<Self> {
        Ok(Self {
            executable: app_program()?,
            config: fs::canonicalize(config)?,
        })
    }
}

/// The app's executable: the signed one shipped in Atlas.app, or in a
/// development checkout the package built here.
///
/// # Errors
///
/// Returns an error when the supported Atlas app executable cannot be located or validated.
pub fn app_program() -> Result<PathBuf> {
    shipped_app()?.map_or_else(development, Ok)
}

/// The package in the checkout this binary was built from, built with SwiftPM.
fn development() -> Result<PathBuf> {
    let package = checkout().join("app");
    if !package.join("Package.swift").is_file() {
        return Err(Error::new(
            Code::Menubar,
            format!("Run the menu bar from Atlas.app; install it with: {INSTALL_COMMAND}"),
        ));
    }

    let swift = |args: &[&str]| {
        atlas_core::spawn::output(
            std::process::Command::new("/usr/bin/swift")
                .args(args)
                .arg("--package-path")
                .arg(&package),
        )
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
                first_line(&built.stderr, 300).unwrap_or_else(|| "no output".to_owned())
            ),
        ));
    }

    let bin = swift(&["build", "-c", "release", "--show-bin-path"])?;
    let directory = String::from_utf8_lossy(&bin.stdout).trim().to_owned();

    Ok(Path::new(&directory).join(APP_EXECUTABLE))
}

/// Start the app with `args` and hand it the URL on stdin. Its commands
/// run this very `atlas` with this configuration. Closing stdin asks the
/// app to quit by itself; dropping the returned child kills it outright
/// (`kill_on_drop`). The login job's name stays here: an `atlas` the app
/// runs is never the login agent ([`started_at_login`]).
async fn spawn(program: &Program, url: &str, args: &[&str]) -> Result<(Child, ChildStdin)> {
    let cli = std::env::current_exe().and_then(fs::canonicalize)?;
    let mut child = atlas_core::spawn::spawn_async(
        Command::new(&program.executable)
            .args(args)
            .env_remove("XPC_SERVICE_NAME")
            .env("ATLAS_CLI", cli)
            .env("ATLAS_CONFIG", &program.config)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .kill_on_drop(true),
    )?;

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
mod tests;

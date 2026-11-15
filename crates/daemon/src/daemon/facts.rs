//! The `hello` reply: who the daemon runs as, in which session, on which
//! display, with which permissions.
use super::recorder;
use atlas_core::{protocol::Census, session::Console};
use atlas_native::native::{accessibility_trusted as accessibility, capture, cf, resources};
use objc2_core_graphics::{
    CGDisplayBounds, CGDisplayCopyDisplayMode, CGDisplayMode, CGMainDisplayID,
};
use serde_json::{Value, json};

/// This process's permissions now (`atlas permissions-info`).
pub fn permissions_now() -> Value {
    json!({
        "ok": true,
        "screenCapture": capture::permitted(),
        "accessibility": accessibility(),
    })
}

/// This process's account: name, home and UID.
fn user() -> Value {
    let uid = nix::unistd::getuid().as_raw();
    match atlas_core::account::account_with_uid(uid) {
        Ok(Some(account)) => json!({"uid": uid, "name": account.name, "home": account.home}),
        _ => json!({"uid": uid, "name": null, "home": null}),
    }
}

/// Whether this graphical session is on the console now, if known, from
/// the window server's own session record.
pub(super) fn console_state() -> Option<bool> {
    let record = cf::session()?;
    cf::boolean(&record, "kCGSSessionOnConsoleKey")
        .or_else(|| cf::boolean(&record, "kCGSessionOnConsoleKey"))
}

/// Who owns this graphical session and whether their login has finished,
/// from the window server's own session record.
pub(super) fn session_owner() -> Option<(i64, bool)> {
    let record = cf::session()?;
    let uid = cf::integer(&record, "kCGSSessionUserIDKey")?;
    let done = cf::boolean(&record, "kCGSessionLoginDoneKey")?;
    Some((uid, done))
}

/// Whether macOS gives this process a mode for the main display, which
/// ScreenCaptureKit needs.
pub(super) fn display_has_mode() -> bool {
    CGDisplayCopyDisplayMode(CGMainDisplayID()).is_some()
}

/// Whether the session is on the console. Unknown counts as on the console:
/// nothing acts unless the session is known to be hidden.
pub(super) fn on_console() -> bool {
    console_state() != Some(false)
}

/// Whether the session is on the console, read as `console`'s kind must
/// read it: unknown counts as away from where it keeps the screen.
pub(super) fn observed(console: Console) -> fn() -> bool {
    match console {
        Console::Off => on_console,
        Console::Own => shown,
    }
}

/// Whether the session is known to be on the console. Unknown counts as
/// off it: a machine's work never runs on a screen not known to be its own.
fn shown() -> bool {
    console_state() == Some(true)
}

/// The main display's bounds in points: x, y, width, height.
pub(super) fn display_bounds() -> (f64, f64, f64, f64) {
    let bounds = CGDisplayBounds(CGMainDisplayID());
    (
        bounds.origin.x,
        bounds.origin.y,
        bounds.size.width,
        bounds.size.height,
    )
}

/// The main display in points, and its backing pixels per point.
fn display() -> Value {
    let display = CGMainDisplayID();
    let bounds = CGDisplayBounds(display);
    let mode = CGDisplayCopyDisplayMode(display);
    let (pixels, points) = (
        CGDisplayMode::pixel_width(mode.as_deref()),
        CGDisplayMode::width(mode.as_deref()),
    );
    let scale = if points > 0 {
        pixels as f64 / points as f64
    } else {
        1.0
    };

    json!({"width": bounds.size.width, "height": bounds.size.height, "scale": scale})
}

pub(super) use atlas_native::native::manager_name;

/// The daemon's hello: who and where the helper is, what it may do, and,
/// when it keeps background jobs, how many may still run, are launching,
/// or have records it cannot read, and whether it could not read them all
/// (`jobs`). Its resources include the screen recorder, when `recorder`
/// has something to say of it.
pub(super) fn hello(
    manager: &str,
    jobs: Option<Census>,
    recorder: Option<recorder::Report>,
) -> Value {
    use atlas_core::protocol::{
        APP_PARTS, JOBS, READ, RGB_STREAM, RGB_STREAM_HIGH_RES, RGB_STREAM_STOP, SCK_EXPERIMENTAL,
        SESSION_PIN, SESSION_PROOF, SOUND,
    };

    let mut capabilities = vec![
        SESSION_PIN,
        APP_PARTS,
        READ,
        RGB_STREAM,
        RGB_STREAM_HIGH_RES,
        SESSION_PROOF,
        SCK_EXPERIMENTAL,
        RGB_STREAM_STOP,
        SOUND,
        atlas_core::outcomes::OUTCOMES,
    ];
    if jobs.is_some() {
        capabilities.push(JOBS);
    }
    let mut usage = json!(resources::own());
    if let (Some(fields), Some(recorder)) = (usage.as_object_mut(), recorder) {
        fields.insert("screenRecorder".to_owned(), json!(recorder));
    }
    let mut hello = json!({
        "ok": true,
        "op": "hello",
        "user": user(),
        "permissions": {
            "accessibility": accessibility(),
            "postEvents": objc2_core_graphics::CGPreflightPostEventAccess(),
            "screenCapture": capture::permitted(),
        },
        "session": {"managerName": manager, "onConsole": console_state()},
        "display": display(),
        "daemon": {"protocol": 1, "pid": std::process::id(), "version": env!("CARGO_PKG_VERSION"),
            "capabilities": capabilities, "resources": usage},
    });
    if let Some(jobs) = jobs {
        hello["daemon"]["jobs"] = json!(jobs);
    }
    // Reading an existing source's counters never starts capture. They stay
    // available after the viewer closes, for performance diagnosis.
    if let Some(stats) = capture::stream_diagnostics() {
        hello["daemon"]["captureStats"] = stats;
    }
    hello["daemon"]["pictureStats"] = capture::picture_diagnostics();
    hello
}

/// A machine desktop's hello: who and where it is, and what it declares
/// (`declared`), never the hidden login's capabilities; whether agents are
/// admitted now, apart from the Stop its disk keeps; and what work may still
/// run there. Read from this process and its own records alone, so a busy
/// guest answers it at once.
pub(super) fn machine_hello(
    manager: &str,
    declared: &atlas_session::compatibility::Declared,
    posture: Posture,
    input: Input,
    stop: &Stop,
    work: Work,
) -> Value {
    let held = posture.held();
    json!({
        "ok": true,
        "op": "hello",
        "user": user(),
        "session": {"managerName": manager, "onConsole": console_state()},
        "daemon": {"protocol": declared.protocol, "pid": std::process::id(),
            "version": declared.version, "capabilities": declared.capabilities},
        "remoteLogin": match posture.remote_login {
            Some(true) => "on",
            Some(false) => "off",
            None => "unknown",
        },
        "account": {"admin": posture.admin},
        "agents": {"admitted": held.is_none(), "held": held, "stopped": stop.halted},
        "input": {
            "standing": input.standing,
            "failure": input.failure.as_ref().map(|failure| json!({"code": failure.code, "error": failure.error})),
            "held": input.held(),
        },
        "stop": match &stop.kept {
            Some(crate::machine::stop::Kept::Clear) => json!({"state": "clear"}),
            Some(crate::machine::stop::Kept::Stopped { why }) => {
                json!({"state": "stopped", "why": why})
            }
            None => json!({"state": "unknown"}),
        },
        "work": {
            "state": work.state(),
            "jobs": told(work.jobs),
            "changes": told(work.changes),
            "supervisors": told(work.supervisors),
        },
    })
}

/// Whether agents are stopped on a machine desktop: as its disk keeps it,
/// none when that cannot be told, and whether its changes are refused now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Stop {
    pub(super) kept: Option<crate::machine::stop::Kept>,
    pub(super) halted: bool,
}

/// What work may still run on a machine desktop, each part none when it
/// cannot be told.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Work {
    /// Background jobs running, starting or being ended.
    pub(super) jobs: Option<usize>,
    /// Supervised commands its keepers hold, running or being ended, a
    /// job's included.
    pub(super) changes: Option<usize>,
    /// Supervisors kept on its disk that no keeper holds and that are not
    /// known to be done with.
    pub(super) supervisors: Option<usize>,
}

impl Work {
    /// `running` when any part runs, `none` only when every part is known
    /// to be idle, else `unknown`, which holds as running does.
    fn state(self) -> &'static str {
        let parts = [self.jobs, self.changes, self.supervisors];
        if parts.iter().any(|part| part.is_some_and(|count| count > 0)) {
            "running"
        } else if parts.iter().all(Option::is_some) {
            "none"
        } else {
            "unknown"
        }
    }
}

/// A count as the hello says it: the number, or `unknown`.
fn told(count: Option<usize>) -> Value {
    count.map_or_else(|| json!("unknown"), |count| json!(count))
}

/// How the guest's input service stands, as its daemon sees it, and why
/// the last one ended before it was ready, as that service said itself.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Input {
    /// `starting`, `ready`, `stopping`, `gone` or `cleanupUnknown`; none when the
    /// guest has no input service.
    pub(super) standing: Option<&'static str>,
    pub(super) failure: Option<crate::machine::input::Failure>,
}

impl Input {
    /// Why agents' changes wait on input, in the person's words; none once
    /// it is ready.
    fn held(&self) -> Option<String> {
        let failed = self.failure.as_ref();
        Some(match (self.standing, failed) {
            (Some("ready"), _) => return None,
            (None, _) => "This guest has no input service.".to_owned(),
            (_, Some(failure)) if failure.code == "permission" => {
                "Allow Atlas in Privacy & Security, Accessibility, in the VM's window: agents wait for it."
                    .to_owned()
            }
            (_, Some(failure)) => failure.error.clone(),
            (Some("starting"), None) => "The guest's input service is starting.".to_owned(),
            (Some("cleanupUnknown"), None) => {
                "Input cleanup has not been confirmed; agents wait for it.".to_owned()
            }
            (Some(_), None) => "The guest's input service is not running.".to_owned(),
        })
    }
}

/// How closed a machine desktop's guest is to anyone but Atlas: whether
/// Remote Login is on and whether its account is an administrator, each
/// none when it cannot be told.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Posture {
    pub(super) remote_login: Option<bool>,
    pub(super) admin: Option<bool>,
}

impl Posture {
    /// As this guest is now.
    pub(super) fn now() -> Self {
        Self {
            remote_login: remote_login(),
            admin: account_admin(),
        }
    }

    /// Closed: Remote Login off and the account no administrator.
    #[cfg(test)]
    pub(super) const CLOSED: Self = Self {
        remote_login: Some(false),
        admin: Some(false),
    };

    /// Why agents are held while the guest is as it says; none once Remote
    /// Login is off and its account no administrator.
    pub(super) fn held(self) -> Option<&'static str> {
        match (self.remote_login, self.admin) {
            (Some(false), Some(false)) => None,
            (Some(true), _) => Some("Remote Login is on in the guest."),
            (None, _) => Some("Whether Remote Login is on in the guest cannot be told."),
            (_, Some(true)) => Some("The guest's account is an administrator."),
            (_, None) => Some("Whether the guest's account is an administrator cannot be told."),
        }
    }
}

/// Whether Remote Login is on in this Mac; none when that cannot be told.
fn remote_login() -> Option<bool> {
    let asked = |arguments: &[&str]| {
        atlas_core::bounded::output(
            std::process::Command::new("/bin/launchctl").args(arguments),
            atlas_core::bounded::LOOKUP,
        )
        .ok()
    };
    let disabled = asked(&["print-disabled", "system"])
        .and_then(|printed| sshd_disabled(&String::from_utf8_lossy(&printed.stdout)));
    let loaded =
        asked(&["print", "system/com.openssh.sshd"]).map(|printed| printed.status.success());
    let address = std::net::SocketAddr::from(([127, 0, 0, 1], 22));
    let listening =
        match std::net::TcpStream::connect_timeout(&address, std::time::Duration::from_millis(500))
        {
            Ok(_) => Some(true),
            Err(error) if error.kind() == std::io::ErrorKind::ConnectionRefused => Some(false),
            Err(_) => None,
        };
    remote_login_from(disabled, loaded, listening)
}

/// Remote Login, as launchd's `disabled` setting for it, whether its job is
/// `loaded`, and whether anything is `listening` on its port say: off only
/// when all three say so, on when any says on, none otherwise.
fn remote_login_from(
    disabled: Option<bool>,
    loaded: Option<bool>,
    listening: Option<bool>,
) -> Option<bool> {
    if disabled == Some(false) || loaded == Some(true) || listening == Some(true) {
        return Some(true);
    }
    (disabled == Some(true) && loaded == Some(false) && listening == Some(false)).then_some(false)
}

/// Whether `launchctl print-disabled system` says Remote Login's job is
/// disabled; none when it does not name it.
fn sshd_disabled(printed: &str) -> Option<bool> {
    printed.lines().find_map(|line| {
        let setting = line.trim().strip_prefix("\"com.openssh.sshd\" => ")?;
        match setting.trim() {
            "disabled" | "true" => Some(true),
            "enabled" | "false" => Some(false),
            _ => None,
        }
    })
}

/// Whether this process's account is an administrator; none when that
/// cannot be told.
fn account_admin() -> Option<bool> {
    let account = nix::unistd::User::from_uid(nix::unistd::getuid())
        .ok()
        .flatten()?;
    atlas_core::account::administrator(&account.name)
}

#[cfg(test)]
#[path = "facts_tests.rs"]
mod tests;

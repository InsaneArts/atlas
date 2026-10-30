//! What tests across the crate share: temporary clients, a stand-in for
//! the helper's daemon, unprivileged stand-ins for interrupted mutation
//! transactions, and running one test again in a process of its own.
mod executor;
mod helper;

pub(crate) use executor::{interrupted_executor, pause_executor};
pub(crate) use helper::{Helper, completed, exited};

use crate::{client::Client, config::Config, control::Controller};
use std::{
    path::{Path, PathBuf},
    process::Command,
};

/// The helper account test clients are configured for.
pub(crate) const UID: u32 = 7001;

/// Process observations supplied to controller-resolution tests.
pub(crate) trait ProcessLookup: Send + Sync {
    fn instances(&self, uid: u32) -> crate::error::Result<Vec<crate::process::Instance>>;
    fn start(&self, pid: i32) -> crate::error::Result<crate::process::Start>;
}

/// This test binary, told to run only the test named `test`, exactly.
pub(crate) fn rerun(test: &str) -> Command {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command.args(["--exact", test]);
    command
}

/// Run lock-lifetime assertions in a process without parallel test threads.
/// On macOS a concurrent spawn can briefly inherit another thread's flock
/// until exec, keeping it held after the owning test drops its descriptor.
/// Unique directories cannot isolate those inherited open file descriptions.
pub(crate) fn isolated(test: &str) -> bool {
    if std::env::var("ATLAS_ISOLATED_TEST").as_deref() == Ok(test) {
        return true;
    }
    let output = rerun(test)
        .arg("--nocapture")
        .env("ATLAS_ISOLATED_TEST", test)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{test} failed: {}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("running 1 test"),
        "the exact test must run: {test}",
    );
    false
}

/// Exercise the real local calendar without changing TZ underneath other
/// test threads. The child inherits its time zone before libc initializes.
pub(crate) fn in_timezone(test: &str, timezone: &str) -> bool {
    if std::env::var("ATLAS_TIME_ZONE_TEST").as_deref() == Ok(test) {
        assert_eq!(std::env::var("TZ").unwrap(), timezone);
        return true;
    }
    let output = rerun(test)
        .arg("--nocapture")
        .env("ATLAS_TIME_ZONE_TEST", test)
        .env("TZ", timezone)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{test} failed: {}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("running 1 test"));
    false
}

/// The explicit temporary directory makes subprocess entry tests inert normally.
pub(crate) fn directory() -> Option<PathBuf> {
    std::env::var_os("ATLAS_MUTATION_TEST_DIRECTORY").map(PathBuf::from)
}

/// A client whose endpoints and controller state are entirely temporary.
pub(crate) fn client(directory: &Path) -> Client {
    let mut client = Client::new(Config {
        helper_uid: UID,
        socket_path: directory.join("absent.sock"),
        identity_probe: directory.join("absent-probe"),
        input_socket: directory.join("absent-input.sock"),
        output_dir: directory.join("output"),
    })
    .unwrap();
    client.controller = Controller::at(UID, &directory.join("state"));
    client
}

/// The helper account [`SETTINGS`] names.
pub(crate) const CONFIGURED_UID: u32 = 60001;

/// A configuration file as a person writes one, its sockets, probe and
/// output named relative to it.
pub(crate) const SETTINGS: &str = "helper_uid = 60001\nsocket_path = \"c.sock\"\noutput_dir = \"out\"\n\
                                   identity_probe = \"probe\"\ninput_socket = \"i.sock\"\n";

/// A client read from [`SETTINGS`] kept in `directory`, with its controller
/// state kept in `directory` too.
pub(crate) fn client_in(directory: &Path) -> Client {
    let config = Config::parse(SETTINGS.as_bytes(), &directory.join("u.toml")).unwrap();
    let mut client = Client::new(config).unwrap();
    client.controller = Controller::at(CONFIGURED_UID, &directory.join("state"));
    client
}

//! What tests across the crate share: temporary clients, a stand-in for
//! the helper's daemon, unprivileged stand-ins for interrupted mutation
//! transactions, and running one test again in a process of its own.
mod child;
mod executor;
mod helper;

pub(crate) use child::{TestChild, message, permitted};
pub(crate) use executor::{Fixture, executor_ready, interrupted_executor, pause_executor};
pub(crate) use helper::{Helper, completed, exited};

use crate::{client::Client, config::Config, control::Controller};
use std::{
    path::{Path, PathBuf},
    process::Command,
};

/// The helper account test clients are configured for.
pub(crate) const UID: u32 = 7001;

/// Unsupported roots for the exported doctor report projections.
pub(crate) fn unsupported_report_roots() -> Vec<serde_json::Value> {
    use serde_json::json;
    vec![
        json!(false),
        json!(true),
        json!(0),
        json!(-1),
        json!(u64::MAX),
        json!(1.5),
        json!(""),
        json!("private-report-input-canary"),
        json!([]),
        json!([null, false, "private-report-input-canary"]),
    ]
}

/// Present final-reply flags that cannot establish whether an outcome is known.
pub(crate) fn invalid_uncertainty_flags() -> Vec<serde_json::Value> {
    use serde_json::json;
    vec![
        json!(null),
        json!(0),
        json!(1),
        json!(1.5),
        json!("private-uncertainty-input-canary"),
        json!([]),
        json!([true]),
        json!({"value": false}),
    ]
}

/// This test binary, told to run only the test named `test`, exactly.
pub(crate) fn rerun(test: &str) -> Command {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command.args(["--exact", test]);
    command
}

/// Run a test in a process without parallel test threads.
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
        socket_path: directory.join("helper/absent.sock"),
        identity_probe: directory.join("absent-probe"),
        input_socket: directory.join("absent-input.sock"),
        output_dir: directory.join("output"),
    })
    .unwrap();
    client.controller = Controller::at(UID, &directory.join("state"));
    client
}

/// A live waiting note beside a temporary client's existing holder. No
/// thread waits for control, and no account's runtime paths are resolved.
pub(crate) fn waiting_note(state: &Path, host: Option<&crate::host_session::HostSession>) {
    use crate::{
        host_session::{Host, Instance},
        time::now_ms,
    };
    use std::fs;

    let holder = fs::read_dir(state)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| path.to_string_lossy().ends_with(".holder.json"))
        .unwrap();
    let waiting = holder.with_file_name(
        holder
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .replace(".holder.json", ".waiting"),
    );
    fs::create_dir_all(&waiting).unwrap();
    let note = crate::control::Note {
        session: host.map(|host| host.key().clone()),
        host: host.map(|host| host.host()),
        folder: host.and_then(|host| host.folder().map(str::to_owned)),
        named: host
            .filter(|host| host.host() == Host::Named)
            .map(|host| host.id().clone()),
        process: Instance::current().unwrap(),
        since: now_ms(),
    };
    fs::write(
        waiting.join("0000000000000001.json"),
        serde_json::to_vec(&note).unwrap(),
    )
    .unwrap();
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

/// Exact compact JSON, including the line delimiter when the transport sends one.
pub(crate) fn reply_cost(value: &serde_json::Value, baseline: &serde_json::Value) {
    let mut serialized = value.to_string();
    if baseline["newline"] == true {
        serialized.push('\n');
    }
    assert_eq!(
        serialized.len(),
        baseline["bytes"].as_u64().unwrap() as usize,
        "{}",
        baseline["id"]
    );
    assert_eq!(
        serialized,
        baseline["serialized"].as_str().unwrap(),
        "{}",
        baseline["id"]
    );
    let rows = baseline["rowsPointer"].as_str().map_or(0, |pointer| {
        value.pointer(pointer).unwrap().as_array().unwrap().len()
    });
    assert_eq!(
        rows,
        baseline["rows"].as_u64().unwrap() as usize,
        "{}",
        baseline["id"]
    );
}

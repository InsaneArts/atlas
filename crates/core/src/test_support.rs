//! What tests across the workspace share: owned child processes, canaries,
//! and running one test again in a process of its own.
#![allow(
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    reason = "test fixtures: failing is how they report a broken test"
)]
mod child;
pub mod host;

pub use child::{TestChild, message, permitted};

use crate::error::Result;
use std::{
    path::{Path, PathBuf},
    process::Command,
};

/// Atlas.app's executable, its reachability read from `reachable`.
pub fn app_executable_using(reachable: impl FnOnce(&Path) -> bool) -> Result<PathBuf> {
    crate::app_bundle::app_executable_using(reachable)
}

/// `executable` as the installed app, its signature read from `developer_id`.
pub fn signed_executable_using(
    executable: PathBuf,
    developer_id: impl FnOnce(&Path) -> bool,
) -> Result<PathBuf> {
    crate::app_bundle::signed_executable_using(executable, developer_id)
}

/// The helper account test clients are configured for.
pub const UID: u32 = 7001;

/// Unsupported roots for the exported doctor report projections.
pub fn unsupported_report_roots() -> Vec<serde_json::Value> {
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
pub fn invalid_uncertainty_flags() -> Vec<serde_json::Value> {
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
pub fn rerun(test: &str) -> Command {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command.args(["--exact", test]);
    command
}

/// Run a test in a process without parallel test threads.
pub fn isolated(test: &str) -> bool {
    if std::env::var("ATLAS_ISOLATED_TEST").as_deref() == Ok(test) {
        return true;
    }
    let output = crate::spawn::output(
        rerun(test)
            .arg("--nocapture")
            .env("ATLAS_ISOLATED_TEST", test),
    )
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
pub fn in_timezone(test: &str, timezone: &str) -> bool {
    if std::env::var("ATLAS_TIME_ZONE_TEST").as_deref() == Ok(test) {
        assert_eq!(std::env::var("TZ").unwrap(), timezone);
        return true;
    }
    let output = crate::spawn::output(
        rerun(test)
            .arg("--nocapture")
            .env("ATLAS_TIME_ZONE_TEST", test)
            .env("TZ", timezone),
    )
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
pub fn directory() -> Option<PathBuf> {
    std::env::var_os("ATLAS_MUTATION_TEST_DIRECTORY").map(PathBuf::from)
}

/// The helper account [`SETTINGS`] names.
pub const CONFIGURED_UID: u32 = 60001;

/// A configuration file as a person writes one, its sockets, probe and
/// output named relative to it.
pub const SETTINGS: &str = "helper_uid = 60001\nsocket_path = \"c.sock\"\noutput_dir = \"out\"\n\
                                   identity_probe = \"probe\"\ninput_socket = \"i.sock\"\n";

/// The reply-cost fixtures in `source`, each one written out or given as a
/// count of rows that `input` builds.
pub fn reply_cost_fixtures(
    source: &str,
    input: impl Fn(usize) -> serde_json::Value,
) -> Vec<serde_json::Value> {
    let mut fixtures: Vec<serde_json::Value> = serde_json::from_str(source).unwrap();
    for fixture in &mut fixtures {
        if let Some(rows) = fixture["inputRows"].as_u64() {
            fixture["input"] = input(usize::try_from(rows).unwrap());
        }
    }
    fixtures
}

/// The text a reply-cost row carries, with escapes and multi-byte
/// characters: once in an even row, six times over in an odd one.
pub fn reply_cost_text(row: usize) -> String {
    const TEXT: &str = "Éditeur 東京 🦀 \"draft\" \\ folder\nnext\tline";
    if row % 2 == 1 {
        TEXT.repeat(6)
    } else {
        TEXT.to_owned()
    }
}

/// Exact compact JSON, including the line delimiter when the transport sends one.
pub fn reply_cost(value: &serde_json::Value, baseline: &serde_json::Value) {
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

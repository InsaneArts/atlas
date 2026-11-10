//! Whether selected tools and apps are usable in the helper account.
//!
//! Presence comes from environment discovery. A present tool is then invoked
//! with `--version` in the helper's selected environment (or, when none is
//! installed, its own PATH), so a binary that exists but cannot run is
//! reported as failing rather than ready. A tool that runs but does not
//! understand `--version` (macOS's own cat and sleep among them) has shown
//! it can run, which is what readiness asks. An app reports its bundle and
//! whether the helper has its own state for it. Launching apps and sign-ins
//! are not verified: credentials are never copied, so only the app can tell.
use crate::{
    client::{Client, Run},
    environment::{
        discovery::{Kind, PathObservation, inspect},
        run as environment_run,
    },
};
use atlas_core::{
    error::{Code, Error, Result},
    transport::run_output,
};
use serde::Serialize;
use serde_json::{Value, json};

/// What the aggregate `ready` does not cover.
const NOT_VERIFIED: [&str; 2] = ["application launch", "application sign-in"];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
enum ItemState {
    Ready,
    Installed,
    Missing,
    Failing,
    Unknown,
}

impl ItemState {
    fn ready(self) -> bool {
        match self {
            Self::Ready | Self::Installed => true,
            Self::Missing | Self::Failing | Self::Unknown => false,
        }
    }
}

/// The PATH tools resolve from inside the helper when no environment is
/// selected: the same places, in the same order, as the runner.
fn helper_path(home: &str) -> String {
    std::env::join_paths(atlas_core::account::helper_search_path(
        std::path::Path::new(home),
    ))
    .map(|path| path.to_string_lossy().into_owned())
    .unwrap_or_default()
}

fn first_line(output: &[u8]) -> String {
    atlas_core::name::first_line(output, 200).unwrap_or_default()
}

/// Classify a present tool from its `--version` invocation. Completion is
/// judged before the exit status; a probe the tool does not understand
/// still shows the tool runs, which is what matters.
fn tool_status(reply: &Value, output: &[u8]) -> (ItemState, String) {
    let text = String::from_utf8_lossy(output).to_lowercase();

    if reply["timedOut"] == true || !reply["signal"].is_null() {
        return (
            ItemState::Unknown,
            "The --version probe did not complete".into(),
        );
    }

    if reply["exitCode"] == 0 {
        return (ItemState::Ready, first_line(output));
    }

    if text.contains("library not loaded") || text.contains("image not found") {
        (
            ItemState::Failing,
            "A required library is missing in the helper account".into(),
        )
    } else if text.contains("bad cpu type") {
        (
            ItemState::Failing,
            "The binary is built for another architecture".into(),
        )
    } else if reply["exitCode"] == 127 || reply["exitCode"] == 126 {
        (
            ItemState::Failing,
            format!("It could not run: {}", first_line(output)),
        )
    } else if ["unknown option", "unrecognized", "invalid option", "usage:"]
        .iter()
        .any(|hint| text.contains(hint))
    {
        (
            ItemState::Ready,
            "Runs, though it does not answer --version".into(),
        )
    } else {
        (ItemState::Failing, first_line(output))
    }
}

async fn probe_using(
    client: &Client,
    home: &str,
    name: &str,
    environment: impl Future<Output = Result<Value>>,
) -> Result<(Value, &'static str)> {
    match environment.await {
        Ok(reply) => Ok((reply, "environment")),
        Err(error) if error.code == crate::environment::NO_PROFILE => {
            let argv = vec![
                "/usr/bin/env".to_owned(),
                format!("PATH={}", helper_path(home)),
                name.to_owned(),
                "--version".to_owned(),
            ];

            Ok((client.run(argv, Run::held()).await?, "path"))
        }
        Err(error) => Err(error),
    }
}

async fn tool(client: &Client, home: &str, name: &str) -> Result<(ItemState, Value)> {
    let argv = vec![name.to_owned(), "--version".to_owned()];
    let (reply, context) = probe_using(
        client,
        home,
        name,
        environment_run(client, argv, crate::client::RUN_TIMEOUT),
    )
    .await?;
    let (status, detail) = match run_output(&reply, 16384) {
        Ok(output) => tool_status(&reply, &output),
        Err(_) => (
            ItemState::Unknown,
            "The --version output was malformed or too long".into(),
        ),
    };

    Ok((
        status,
        json!({
            "kind": "tool",
            "name": name,
            "status": status,
            "detail": detail,
            "context": context,
            "exitCode": reply["exitCode"],
        }),
    ))
}

/// A plausible bundle identifier (reverse-DNS characters only).
fn bundle_identifier(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= 255
        && text
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b".-_".contains(&c))
}

/// Parse the app probe's `key=value` lines; anything unexpected is None.
fn app_facts(output: &str) -> Option<(String, Vec<String>)> {
    let mut identifier = None;
    let mut state = Vec::new();

    for line in output.lines() {
        match line.split_once('=')? {
            ("id", id) if identifier.is_none() && bundle_identifier(id) => {
                identifier = Some(id.to_owned());
            }
            ("state", kind @ ("preferences" | "container")) => state.push(kind.to_owned()),
            _ => return None,
        }
    }

    Some((identifier?, state))
}

/// Helper-side facts about an app bundle: its identifier and whether the
/// helper account has preferences or a container for it.
async fn app(client: &Client, name: &str, bundle: &str) -> (ItemState, Value) {
    let script = concat!(
        "id=$(/usr/bin/plutil -extract CFBundleIdentifier raw -o - \"$1/Contents/Info.plist\") ",
        "|| exit 3; ",
        "case \"$id\" in *[!A-Za-z0-9._-]*|'') exit 4;; esac; ",
        "printf 'id=%s\\n' \"$id\"; ",
        "[ -e \"$HOME/Library/Preferences/$id.plist\" ] && printf 'state=preferences\\n'; ",
        "[ -d \"$HOME/Library/Containers/$id\" ] && printf 'state=container\\n'; true",
    );

    // One app the helper could not look at leaves the others' report whole.
    let reply = client
        .run(
            vec![
                "/bin/sh".into(),
                "-c".into(),
                script.into(),
                "sh".into(),
                bundle.into(),
            ],
            Run::inspect(),
        )
        .await
        .unwrap_or(Value::Null);

    let facts = (reply["exitCode"] == 0)
        .then(|| run_output(&reply, 4096).ok())
        .flatten()
        .and_then(|output| app_facts(&String::from_utf8_lossy(&output)));
    let Some((identifier, state)) = facts else {
        return (
            ItemState::Unknown,
            json!({
                "kind": "application",
                "name": name,
                "status": ItemState::Unknown,
                "bundle": bundle,
                "detail": "The bundle could not be inspected in the helper",
            }),
        );
    };

    (
        ItemState::Installed,
        json!({
            "kind": "application",
            "name": name,
            "status": ItemState::Installed,
            "bundle": bundle,
            "bundleIdentifier": identifier,
            "helperState": if state.is_empty() { "none" } else { "present" },
            "helperStateKinds": state,
            "signIn": "unknown",
            "detail": "Account data and sign-ins belong to the helper; check the app itself.",
        }),
    )
}

/// The first helper candidate discovery found usable: a readable,
/// traversable bundle directory.
fn usable_bundle(candidates: &[PathObservation]) -> Option<&str> {
    candidates
        .iter()
        .find(|candidate| {
            let access = &candidate.access;
            access.directory && access.readable && access.executable
        })?
        .path
        .to_str()
}

/// Report readiness for the selected tools and apps. `ready` means every
/// tool ran and every app is installed; for apps it does not cover
/// `NOT_VERIFIED`.
///
/// # Errors
///
/// Rejects invalid tool or app names and returns helper identity or discovery failures.
/// Availability does not prove authentication or that a program can complete a task.
pub async fn readiness(client: &Client, tools: &[String], apps: &[String]) -> Result<Value> {
    if tools.is_empty() && apps.is_empty() {
        return Err(Error::new(
            Code::Usage,
            "Select tools with --tool and apps with --app",
        ));
    }

    let found = inspect(client, tools, apps, &[], &[]).await?;
    let home = client.status().await["user"]["home"]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| Error::new(Code::Identity, "The helper home is unknown"))?;
    let mut reports = Vec::new();
    let mut ready = true;

    for item in &found.selections {
        let name = &item.name;
        let kind = match item.kind {
            Kind::Tool => "tool",
            Kind::Application => "application",
        };
        let (state, report) = match (item.kind, item.available()) {
            (Kind::Tool, true) => tool(client, &home, name).await?,
            (Kind::Application, true) => match usable_bundle(&item.helper_candidates) {
                Some(bundle) => app(client, name, bundle).await,
                None => (
                    ItemState::Unknown,
                    json!({ "kind": kind, "name": name, "status": ItemState::Unknown }),
                ),
            },
            (_, false) => (
                ItemState::Missing,
                json!({
                    "kind": kind,
                    "name": name,
                    "status": ItemState::Missing,
                    "detail": "Not found where the helper looks; copy it in with atlas environment, or install it for every account",
                }),
            ),
        };

        ready &= state.ready();
        reports.push(report);
    }

    Ok(json!({
        "ok": true,
        "ready": ready,
        "notVerified": if apps.is_empty() { &[][..] } else { &NOT_VERIFIED[..] },
        "items": reports,
    }))
}

#[cfg(test)]
#[path = "readiness_tests.rs"]
mod tests;

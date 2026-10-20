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
    client::Client,
    environment::{discovery::discover, run as environment_run},
    error::{Error, Result},
    transport::run_output,
};
use serde_json::{Value, json};

/// What the aggregate `ready` does not cover.
const NOT_VERIFIED: [&str; 2] = ["application launch", "application sign-in"];

/// The PATH tools resolve from inside the helper when no environment is
/// selected: the same places, in the same order, as the runner.
fn helper_path(home: &str) -> String {
    std::env::join_paths(crate::environment::helper_search_path(
        std::path::Path::new(home),
    ))
    .map(|path| path.to_string_lossy().into_owned())
    .unwrap_or_default()
}

/// The first non-empty line of command output, bounded for reports.
fn first_line(output: &[u8]) -> String {
    crate::ui::text::first_line(output, 200).unwrap_or_default()
}

/// Classify a present tool from its `--version` invocation. Completion is
/// judged before the exit status; a probe the tool does not understand
/// still shows the tool runs, which is what matters.
pub(crate) fn tool_status(reply: &Value, output: &[u8]) -> (&'static str, String) {
    let text = String::from_utf8_lossy(output).to_lowercase();

    if reply["timedOut"] == true || !reply["signal"].is_null() {
        return ("unknown", "The --version probe did not complete".into());
    }

    if reply["exitCode"] == 0 {
        return ("ready", first_line(output));
    }

    if text.contains("library not loaded") || text.contains("image not found") {
        (
            "failing",
            "A required library is missing in the helper account".into(),
        )
    } else if text.contains("bad cpu type") {
        (
            "failing",
            "The binary is built for another architecture".into(),
        )
    } else if reply["exitCode"] == 127 || reply["exitCode"] == 126 {
        (
            "failing",
            format!("It could not run: {}", first_line(output)),
        )
    } else if ["unknown option", "unrecognized", "invalid option", "usage:"]
        .iter()
        .any(|hint| text.contains(hint))
    {
        ("ready", "Runs, though it does not answer --version".into())
    } else {
        ("failing", first_line(output))
    }
}

/// Run `name --version` in the helper's selected environment, or with the
/// helper PATH when no environment profile is installed.
async fn probe(client: &Client, home: &str, name: &str) -> Result<(Value, &'static str)> {
    let argv = vec![name.to_owned(), "--version".to_owned()];
    match environment_run(client, argv).await {
        Ok(reply) => Ok((reply, "environment")),
        // An older helper build refused with the generic code; its wording
        // is matched only for that.
        Err(error)
            if error.code == crate::environment::NO_PROFILE
                || (error.code == "environment"
                    && error.error.contains("No helper environment profile")) =>
        {
            let argv = vec![
                "/usr/bin/env".to_owned(),
                format!("PATH={}", helper_path(home)),
                name.to_owned(),
                "--version".to_owned(),
            ];

            Ok((client.run(argv).await?, "path"))
        }
        Err(error) => Err(error),
    }
}

async fn tool(client: &Client, home: &str, name: &str) -> Result<Value> {
    let (reply, context) = probe(client, home, name).await?;
    let (status, detail) = match run_output(&reply, 16384) {
        Ok(output) => tool_status(&reply, &output),
        Err(_) => (
            "unknown",
            "The --version output was malformed or too long".into(),
        ),
    };

    Ok(json!({
        "kind": "tool",
        "name": name,
        "status": status,
        "detail": detail,
        "context": context,
        "exitCode": reply["exitCode"],
    }))
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
pub(crate) fn app_facts(output: &str) -> Option<(String, Vec<String>)> {
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
async fn app(client: &Client, name: &str, bundle: &str) -> Result<Value> {
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
        .inspect(vec![
            "/bin/sh".into(),
            "-c".into(),
            script.into(),
            "sh".into(),
            bundle.into(),
        ])
        .await
        .unwrap_or(Value::Null);

    let facts = (reply["exitCode"] == 0)
        .then(|| run_output(&reply, 4096).ok())
        .flatten()
        .and_then(|output| app_facts(&String::from_utf8_lossy(&output)));
    let Some((identifier, state)) = facts else {
        return Ok(json!({
            "kind": "application",
            "name": name,
            "status": "unknown",
            "bundle": bundle,
            "detail": "The bundle could not be inspected in the helper",
        }));
    };

    Ok(json!({
        "kind": "application",
        "name": name,
        "status": "installed",
        "bundle": bundle,
        "bundleIdentifier": identifier,
        "helperState": if state.is_empty() { "none" } else { "present" },
        "helperStateKinds": state,
        "signIn": "unknown",
        "detail": "Account data and sign-ins belong to the helper; check the app itself.",
    }))
}

/// The first helper candidate discovery found usable: a readable,
/// traversable bundle directory.
fn usable_bundle(item: &Value) -> Option<&str> {
    item["helperCandidates"]
        .as_array()?
        .iter()
        .find(|candidate| {
            let access = &candidate["access"];
            access["directory"] == true
                && access["readable"] == true
                && access["executable"] == true
        })?
        .get("path")?
        .as_str()
}

/// Report readiness for the selected tools and apps. `ready` means every
/// tool ran and every app is installed; for apps it does not cover
/// `NOT_VERIFIED`.
pub async fn readiness(client: &Client, tools: &[String], apps: &[String]) -> Result<Value> {
    if tools.is_empty() && apps.is_empty() {
        return Err(Error::new(
            "usage",
            "Select tools with --tool and apps with --app",
        ));
    }

    let found = discover(client, tools, apps, &[], &[]).await?;
    let home = client.status().await["user"]["home"]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| Error::new("identity", "The helper home is unknown"))?;
    let mut reports = Vec::new();

    for item in found["selections"].as_array().into_iter().flatten() {
        let name = item["name"].as_str().unwrap_or_default();
        let kind = item["kind"].as_str();
        let report = match (kind, item["available"] == true) {
            (Some("tool"), true) => tool(client, &home, name).await?,
            (Some("application"), true) => match usable_bundle(item) {
                Some(bundle) => app(client, name, bundle).await?,
                None => json!({ "kind": kind, "name": name, "status": "unknown" }),
            },
            (_, false) => json!({
                "kind": kind,
                "name": name,
                "status": "missing",
                "detail": "Not found where the helper looks; copy it in with atlas environment, or install it for every account",
            }),
            _ => json!({ "kind": kind, "name": name, "status": "unknown" }),
        };

        reports.push(report);
    }

    let ready = reports
        .iter()
        .all(|report| matches!(report["status"].as_str(), Some("ready" | "installed")));

    Ok(json!({
        "ok": true,
        "ready": ready,
        "notVerified": if apps.is_empty() { &[][..] } else { &NOT_VERIFIED[..] },
        "items": reports,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reply(exit: i64) -> Value {
        json!({"exitCode": exit, "timedOut": false, "signal": null})
    }

    #[test]
    fn tools_are_classified_by_a_completed_invocation() {
        assert_eq!(
            tool_status(&reply(0), b"\nmarkdown2pdf 1.6.1\nmore"),
            ("ready", "markdown2pdf 1.6.1".into())
        );

        let timed_out = json!({"exitCode": 0, "timedOut": true, "signal": "SIGKILL"});
        assert_eq!(
            tool_status(&timed_out, b"x").0,
            "unknown",
            "never ready on a timeout"
        );
        assert_eq!(
            tool_status(&reply(134), b"dyld: Library not loaded: @rpath/libx.dylib").1,
            "A required library is missing in the helper account"
        );
        assert_eq!(
            tool_status(&reply(126), b"Bad CPU type in executable").0,
            "failing"
        );
        assert_eq!(
            tool_status(&reply(127), b"env: python3: No such file").0,
            "failing",
            "present but broken, not missing"
        );
        assert_eq!(tool_status(&reply(2), b"usage: tool [-x]").0, "ready");
        assert_eq!(tool_status(&reply(1), b"error: no config").0, "failing");
        assert_eq!(first_line(&[b'x'; 500]).len(), 200);
    }

    #[test]
    fn app_facts_accept_only_the_probe_protocol() {
        assert_eq!(
            app_facts("id=com.apple.Preview\nstate=preferences\nstate=container\n"),
            Some((
                "com.apple.Preview".into(),
                vec!["preferences".into(), "container".into()]
            ))
        );
        assert_eq!(
            app_facts("id=com.apple.Chess\n"),
            Some(("com.apple.Chess".into(), vec![]))
        );

        for output in [
            "",
            "state=preferences\n",
            "id=com.apple.x\nnoise\n",
            "id=../evil\n",
            "id=a\nid=b\n",
            "id=com.apple.x\nstate=other\n",
        ] {
            assert_eq!(app_facts(output), None, "{output:?}");
        }
    }

    #[test]
    fn only_usable_bundle_candidates_are_inspected() {
        let item = json!({"helperCandidates": [
            {"path": "/Applications/A.app",
             "access": {"directory": true, "readable": false, "executable": true}},
            {"path": "/Users/h/Applications/A.app",
             "access": {"directory": true, "readable": true, "executable": true}},
        ]});
        assert_eq!(usable_bundle(&item), Some("/Users/h/Applications/A.app"));
        assert_eq!(usable_bundle(&json!({"helperCandidates": []})), None);
    }

    #[test]
    fn helper_path_prefers_helper_local_tools() {
        let path = helper_path("/Users/h");
        assert!(path.starts_with("/Users/h/.local/bin:/Users/h/.cargo/bin:"));
        assert!(path.ends_with("/usr/sbin:/sbin"));
    }
}

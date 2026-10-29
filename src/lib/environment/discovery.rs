//! Read-only inspection of explicit selections; never executes a selected tool
//! or sources a shell startup file. Presence is not application compatibility.
use super::{SHARED_BIN, helper_search_path, variable_name as variable};
use crate::{
    client::{Client, RUN_TIMEOUT},
    error::{Error, Result},
    session::require_hidden,
    transport::{OutputError, run_output},
};
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    fs,
    ops::Range,
    path::{Component, Path, PathBuf},
};

/// Every path is a separate positional argument. No selected name, path or
/// environment value is interpreted as shell syntax or emitted by this probe.
const PATH_PROBE: &str = r#"
for item do
    flags=''
    if [ -f "$item" ]; then flags="${flags}f"; fi
    if [ -d "$item" ]; then flags="${flags}d"; fi
    if [ -r "$item" ]; then flags="${flags}r"; fi
    if [ -w "$item" ]; then flags="${flags}w"; fi
    if [ -x "$item" ]; then flags="${flags}x"; fi
    printf '%s\n' "-$flags"
done
"#;
const ENV_PROBE: &str = r#"
for item do
    if /usr/bin/printenv "$item" >/dev/null 2>&1; then
        printf 'defined\n'
    else
        printf 'undefined\n'
    fi
done
"#;

/// The two fixed scripts discovery runs in the helper. Both only test paths
/// or look up variable names, so they take no control and leave nothing
/// pending when their reply is lost.
#[derive(Clone, Copy)]
enum Probe {
    /// Whether each path is a file or folder the helper can read, write or run.
    Paths,
    /// Whether each variable is defined in the helper's inherited environment.
    Variables,
}

impl Probe {
    fn script(self) -> &'static str {
        match self {
            Probe::Paths => PATH_PROBE,
            Probe::Variables => ENV_PROBE,
        }
    }

    /// Run this probe over `args` in the helper: one output line per
    /// argument.
    async fn run(self, client: &Client, args: &[String]) -> Result<Vec<String>> {
        if args.is_empty() {
            return Ok(Vec::new());
        }

        let mut argv = vec![
            "/bin/sh".into(),
            "-c".into(),
            self.script().into(),
            "sh".into(),
        ];
        argv.extend_from_slice(args);

        let reply = client.inspect_within(argv, RUN_TIMEOUT).await?;
        if reply["exitCode"] != 0 || reply["timedOut"] == true || !reply["signal"].is_null() {
            return Err(Error::new(
                "environment",
                "Helper discovery did not complete",
            ));
        }

        let output = run_output(&reply, 16384).map_err(|error| match error {
            OutputError::Malformed => invalid(),
            OutputError::Oversized => {
                Error::new("environment", "Discovery output exceeded its limit")
            }
        })?;

        let lines = String::from_utf8(output)
            .map_err(|_| invalid())?
            .lines()
            .map(str::to_owned)
            .collect::<Vec<_>>();
        if lines.len() != args.len() {
            return Err(Error::new(
                "environment",
                "Incomplete discovery probe response",
            ));
        }

        Ok(lines)
    }
}

/// The most of each kind one inspection may select.
const SELECTION_LIMIT: usize = 16;
/// Where macOS keeps its own apps that are not in /System/Applications.
const CORE_SERVICES: &str = "/System/Library/CoreServices";

fn invalid() -> Error {
    Error::new(
        "environment",
        "The helper's discovery answer could not be read; run atlas doctor",
    )
}

/// Refuse a selection with a sentence that names the item at fault, so an
/// agent corrects its command rather than guessing.
fn selection(
    tools: &[String],
    apps: &[String],
    environment: &[String],
    references: &[impl AsRef<Path>],
) -> Result<()> {
    let refuse = |message: String| Err(Error::new("environment", message));

    if tools.len() + apps.len() + environment.len() + references.len() == 0 {
        return refuse("Select at least one tool, app, variable or reference path".into());
    }

    if [tools.len(), apps.len(), environment.len(), references.len()]
        .into_iter()
        .any(|len| len > SELECTION_LIMIT)
    {
        return refuse(format!(
            "Select at most {SELECTION_LIMIT} tools, {SELECTION_LIMIT} apps, \
             {SELECTION_LIMIT} variables and {SELECTION_LIMIT} reference paths at a time"
        ));
    }

    if let Some((name, problem)) = tools
        .iter()
        .find_map(|name| Some((name, name_problem(name)?)))
    {
        return refuse(format!(
            "A tool is named by its bare command name such as cat; this one {problem}: {name:?}"
        ));
    }

    if let Some((name, problem)) = apps
        .iter()
        .find_map(|name| Some((name, name_problem(name)?)))
    {
        return refuse(format!(
            "An app is named by its name such as Safari; this one {problem}: {name:?}"
        ));
    }

    if let Some(name) = environment.iter().find(|name| !variable(name)) {
        return refuse(format!(
            "A variable name is letters, digits and underscores, not starting with a digit: {name}"
        ));
    }

    if let Some(path) = references.iter().map(AsRef::as_ref).find(|path: &&Path| {
        !path.is_absolute()
            || path
                .to_str()
                .is_none_or(|p| p.len() > 4096 || p.contains('\0'))
            || path.components().any(|c| matches!(c, Component::ParentDir))
    }) {
        return refuse(format!(
            "A reference path is absolute, without ..: {}",
            path.display()
        ));
    }

    Ok(())
}

/// What is wrong with a tool or app name, if anything.
fn name_problem(name: &str) -> Option<&'static str> {
    if name.is_empty() {
        Some("is empty")
    } else if name.len() > 128 {
        Some("is longer than 128 bytes")
    } else if name.chars().any(char::is_control) {
        Some("contains a control character")
    } else if !component(name) {
        Some("is a path")
    } else {
        None
    }
}

fn component(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && !name.chars().any(char::is_control)
        && Path::new(name).components().count() == 1
        && matches!(
            Path::new(name).components().next(),
            Some(Component::Normal(_))
        )
}

#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct Access {
    file: bool,
    directory: bool,
    readable: bool,
    writable: bool,
    executable: bool,
}

impl Access {
    fn from_flags(flags: &str) -> Result<Self> {
        if !flags.starts_with('-') || flags.bytes().skip(1).any(|c| !b"fdrwx".contains(&c)) {
            return Err(Error::new(
                "environment",
                "Invalid discovery probe response",
            ));
        }

        Ok(Self {
            file: flags.contains('f'),
            directory: flags.contains('d'),
            readable: flags.contains('r'),
            writable: flags.contains('w'),
            executable: flags.contains('x'),
        })
    }
}

fn access(path: &Path) -> Access {
    use nix::unistd::AccessFlags;

    let metadata = fs::metadata(path).ok();
    let permits = |mode| nix::unistd::access(path, mode).is_ok();

    Access {
        file: metadata.as_ref().is_some_and(|m| m.is_file()),
        directory: metadata.as_ref().is_some_and(|m| m.is_dir()),
        readable: permits(AccessFlags::R_OK),
        writable: permits(AccessFlags::W_OK),
        executable: permits(AccessFlags::X_OK),
    }
}

/// Inspect selected names and paths without installing, launching or signing in.
pub async fn discover(
    client: &Client,
    tools: &[String],
    apps: &[String],
    environment: &[String],
    references: &[PathBuf],
) -> Result<Value> {
    selection(tools, apps, environment, references)?;

    let status = client.status().await;
    let session = require_hidden(&status, client.config.helper_uid)?;
    let home_unknown = || Error::new("identity", "The helper home is unknown; run atlas doctor");
    let helper_home = PathBuf::from(status["user"]["home"].as_str().ok_or_else(home_unknown)?);
    if !helper_home.is_absolute() {
        return Err(home_unknown());
    }

    let home = crate::config::home()?;
    let mut host_bins = std::env::var_os("PATH")
        .map(|value| {
            std::env::split_paths(&value)
                .filter(|p| p.is_absolute())
                .take(32)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    host_bins.extend([home.join(".local/bin"), home.join(".cargo/bin")]);
    host_bins.extend(SHARED_BIN.iter().map(PathBuf::from));
    let helper_bins = helper_search_path(&helper_home);

    // Every helper-side path is probed in one run; each report remembers
    // which of them are its candidates.
    let mut paths = Vec::<PathBuf>::new();
    let mut reports: Vec<(Value, Range<usize>)> = Vec::new();

    for name in tools {
        let host = host_bins.iter().map(|dir| dir.join(name)).find(|path| {
            let metadata = access(path);
            metadata.file && metadata.executable
        });
        let start = paths.len();

        paths.extend(helper_bins.iter().map(|dir| dir.join(name)));
        reports.push((
            json!({"kind": "tool", "name": name, "host": host, "executed": false}),
            start..paths.len(),
        ));
    }

    for name in apps {
        let bundle = if name.ends_with(".app") {
            name.clone()
        } else {
            format!("{name}.app")
        };

        // Where apps live: the usual folders, and macOS's own in
        // CoreServices (Finder among them).
        let host = [
            PathBuf::from("/Applications"),
            PathBuf::from("/System/Applications"),
            PathBuf::from(CORE_SERVICES),
            home.join("Applications"),
        ]
        .iter()
        .map(|dir| dir.join(&bundle))
        .find(|path| path.is_dir());
        let start = paths.len();

        paths.extend([
            PathBuf::from("/Applications").join(&bundle),
            PathBuf::from("/System/Applications").join(&bundle),
            PathBuf::from(CORE_SERVICES).join(&bundle),
            helper_home.join("Applications").join(&bundle),
        ]);
        reports.push((
            json!({
                "kind": "application",
                "name": name,
                "host": host,
                "launched": false,
                "accountData": "separate",
                "signIn": "not checked",
            }),
            start..paths.len(),
        ));
    }

    let reference_start = paths.len();
    paths.extend_from_slice(references);

    let args = paths
        .iter()
        .map(|p| p.to_str().map(str::to_owned).ok_or_else(invalid))
        .collect::<Result<Vec<_>>>()?;

    let observed = Probe::Paths
        .run(client, &args)
        .await?
        .iter()
        .map(|s| Access::from_flags(s))
        .collect::<Result<Vec<_>>>()?;

    let selections: Vec<Value> = reports
        .into_iter()
        .map(|(mut report, range)| {
            let usable = |access: &Access| {
                if report["kind"] == "tool" {
                    access.file && access.executable
                } else {
                    access.directory && access.readable && access.executable
                }
            };
            report["available"] = json!(range.clone().any(|i| usable(&observed[i])));
            report["helperCandidates"] = json!(
                range
                    .map(|i| json!({"path": paths[i], "access": observed[i]}))
                    .collect::<Vec<_>>()
            );

            report
        })
        .collect();

    let defined = Probe::Variables.run(client, environment).await?;
    if defined.iter().any(|s| s != "defined" && s != "undefined") {
        return Err(invalid());
    }

    if client.identity().await?.epoch != session.epoch {
        return Err(Error::new(
            "identity",
            "Helper changed during discovery; repeat the inspection",
        ));
    }

    let variables: Vec<Value> = environment
        .iter()
        .zip(defined)
        .map(|(name, value)| {
            json!({
                "name": name,
                "hostDefined": std::env::var_os(name).is_some(),
                "helperInheritedDefined": value == "defined",
            })
        })
        .collect();

    let references: Vec<Value> = references
        .iter()
        .enumerate()
        .map(|(i, path)| {
            json!({"path": path, "host": access(path), "helper": observed[reference_start + i]})
        })
        .collect();

    Ok(json!({
        "ok": true,
        "readOnly": true,
        "helperUID": session.epoch.uid,
        "selections": selections,
        "environment": variables,
        "references": references,
        "scope": "Selected executable/app presence and access only. Shell startup files were \
                  not loaded; inherited variables may differ from Terminal. Dependencies, app \
                  data and sign-ins require separate live verification. No selected program \
                  was executed and no file contents or variable values were read into the \
                  report.",
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{Helper, UID, completed};
    use std::os::unix::fs::PermissionsExt;

    /// The arguments a probe was given, after its fixed script.
    fn arguments(request: &Value) -> &[Value] {
        &request["argv"].as_array().unwrap()[4..]
    }

    /// Answer a probe as a helper where every path is a runnable file and
    /// every variable is defined.
    fn found(request: &Value) -> Option<Value> {
        let line = if request["argv"][2] == PATH_PROBE {
            "-frx\n"
        } else {
            "defined\n"
        };

        Some(completed(&line.repeat(arguments(request).len())))
    }

    /// Discover one tool, one variable and one reference path.
    async fn discovered(client: &Client) -> Result<Value> {
        discover(
            client,
            &["cat".into()],
            &[],
            &["HOME".into()],
            &[PathBuf::from("/Users/Shared")],
        )
        .await
    }

    #[tokio::test]
    async fn discovery_works_while_another_controller_holds_the_lock() {
        let directory = tempfile::tempdir().unwrap();
        let (client, helper) = Helper::client(directory.path(), UID);
        let _held = client.controller.acquire(false).unwrap();

        let report = helper.serve(discovered(&client), found).await.unwrap();
        assert_eq!(report["ok"], true);
        assert_eq!(report["selections"][0]["available"], true);
        assert_eq!(report["environment"][0]["helperInheritedDefined"], true);
        assert!(client.controller.pending().unwrap().is_empty());
    }

    #[tokio::test]
    async fn discovery_works_while_an_earlier_outcome_is_unknown() {
        let directory = tempfile::tempdir().unwrap();
        let (client, helper) = Helper::client(directory.path(), UID);
        let unknown = client.controller.quarantine().unwrap();
        let generation = unknown.generation().to_owned();
        drop(unknown);

        let report = helper.serve(discovered(&client), found).await.unwrap();
        assert_eq!(report["ok"], true);
        assert_eq!(client.controller.pending().unwrap(), [generation]);
    }

    #[tokio::test]
    async fn a_lost_discovery_reply_leaves_no_marker() {
        let directory = tempfile::tempdir().unwrap();
        let (client, helper) = Helper::client(directory.path(), UID);

        let error = helper
            .serve(discovered(&client), |_| None)
            .await
            .unwrap_err();
        assert!(!error.uncertain, "{error:?}");
        assert!(client.controller.pending().unwrap().is_empty());
        assert!(client.controller.acquire(false).is_ok());
    }

    #[tokio::test]
    async fn a_discovery_probe_that_times_out_leaves_no_marker() {
        let directory = tempfile::tempdir().unwrap();
        let (client, helper) = Helper::client(directory.path(), UID);
        let timed_out =
            json!({"ok": true, "exitCode": null, "timedOut": true, "signal": "SIGKILL"});

        let error = helper
            .serve(discovered(&client), |_| Some(timed_out.clone()))
            .await
            .unwrap_err();
        assert_eq!(error.code, "environment");
        assert!(!error.uncertain);
        assert!(client.controller.pending().unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_helper_that_changes_during_discovery_still_fails_it() {
        let directory = tempfile::tempdir().unwrap();
        let (client, helper) = Helper::client(directory.path(), UID);
        let helper = helper.moving_after(1);

        let error = helper.serve(discovered(&client), found).await.unwrap_err();
        assert_eq!(error.code, "identity");
        assert_eq!(
            error.error,
            "Helper changed during discovery; repeat the inspection"
        );
        assert!(client.controller.pending().unwrap().is_empty());
    }

    #[tokio::test]
    async fn bad_discovery_output_still_fails_it() {
        let directory = tempfile::tempdir().unwrap();
        let (client, helper) = Helper::client(directory.path(), UID);

        let unknown_flag =
            |request: &Value| Some(completed(&"-q\n".repeat(arguments(request).len())));
        let error = helper
            .serve(discovered(&client), unknown_flag)
            .await
            .unwrap_err();
        assert_eq!(error.code, "environment");
        assert_eq!(error.error, "Invalid discovery probe response");

        let short =
            |request: &Value| Some(completed(&"-frx\n".repeat(arguments(request).len() - 1)));
        let error = helper.serve(discovered(&client), short).await.unwrap_err();
        assert_eq!(error.code, "environment");
        assert_eq!(error.error, "Incomplete discovery probe response");
    }

    #[tokio::test]
    async fn a_helper_on_screen_refuses_discovery_with_its_own_code() {
        let directory = tempfile::tempdir().unwrap();
        let (client, helper) = Helper::client(directory.path(), UID);
        let on_screen = json!({"ok": false, "code": "on-console",
            "error": "The helper is on screen", "uncertain": false});

        let error = helper
            .serve(discovered(&client), |_| Some(on_screen.clone()))
            .await
            .unwrap_err();
        assert_eq!(error.code, "on-console");
        assert!(!error.uncertain);
    }

    #[tokio::test]
    async fn discovery_writes_no_activity_row() {
        let directory = tempfile::tempdir().unwrap();
        let (client, helper) = Helper::client(directory.path(), UID);

        helper.serve(discovered(&client), found).await.unwrap();
        assert!(
            !client
                .config
                .output_dir
                .join(crate::activity_log::FILE)
                .exists()
        );
    }

    #[tokio::test]
    async fn discovery_runs_only_its_two_fixed_scripts() {
        let directory = tempfile::tempdir().unwrap();
        let (client, helper) = Helper::client(directory.path(), UID);
        let mut heard = Vec::new();

        helper
            .serve(discovered(&client), |request| {
                heard.push(request.clone());
                found(request)
            })
            .await
            .unwrap();

        let [paths, variables] = heard.as_slice() else {
            panic!("discovery runs one path probe and one variable probe: {heard:?}");
        };
        for (request, script) in [(paths, PATH_PROBE), (variables, ENV_PROBE)] {
            let fixed = &request["argv"].as_array().unwrap()[..4];
            assert_eq!(
                fixed,
                [json!("/bin/sh"), json!("-c"), json!(script), json!("sh")]
            );
        }

        let paths = arguments(paths);
        assert!(
            paths[..paths.len() - 1]
                .iter()
                .all(|path| { path.as_str().is_some_and(|path| path.ends_with("/cat")) })
        );
        assert_eq!(paths.last().unwrap(), "/Users/Shared");
        assert_eq!(arguments(variables), [json!("HOME")]);
    }

    #[test]
    fn inspection_does_not_execute_a_selected_program() {
        let directory = tempfile::tempdir().unwrap();
        let tool = directory.path().join("tool with spaces");
        fs::write(&tool, b"#!/bin/sh\nexit 127\n").unwrap();
        fs::set_permissions(&tool, fs::Permissions::from_mode(0o700)).unwrap();
        let result = access(&tool);
        assert!(result.file && result.executable);
        fs::set_permissions(&tool, fs::Permissions::from_mode(0o600)).unwrap();
        assert!(!access(&tool).executable);
    }

    #[test]
    fn selections_and_probe_results_are_bounded() {
        assert!(component("Google Chrome"));
        assert!(!component("../other"));
        assert!(!component("tool\nname"));
        assert!(!variable("KEY=value"));
        assert!(!variable("-PATH"));
        assert!(variable("OLLAMA_MODELS"));
        assert!(Access::from_flags("-frx").unwrap().executable);
        assert!(Access::from_flags("unexpected content").is_err());
    }

    /// A refused selection names the item at fault and what was expected.
    #[test]
    fn a_refused_selection_names_the_item_at_fault() {
        let names = |items: &[&str]| items.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let paths = |items: &[&str]| items.iter().map(PathBuf::from).collect::<Vec<_>>();
        let said = |tools: &[&str], apps: &[&str], variables: &[&str], references: &[&str]| {
            selection(
                &names(tools),
                &names(apps),
                &names(variables),
                &paths(references),
            )
            .unwrap_err()
            .error
        };

        assert_eq!(
            said(&[], &[], &[], &[]),
            "Select at least one tool, app, variable or reference path"
        );
        assert_eq!(
            said(&["/bin/sh"], &[], &[], &[]),
            "A tool is named by its bare command name such as cat; this one is a path: \"/bin/sh\""
        );
        assert_eq!(
            said(&[""], &[], &[], &[]),
            "A tool is named by its bare command name such as cat; this one is empty: \"\""
        );
        assert_eq!(
            said(&["sh"], &["Sa\nfari"], &[], &[]),
            "An app is named by its name such as Safari; this one contains a control character: \"Sa\\nfari\""
        );
        assert_eq!(
            said(&["sh"], &["/Applications/Safari.app"], &[], &[]),
            "An app is named by its name such as Safari; this one is a path: \"/Applications/Safari.app\""
        );
        assert_eq!(
            said(&[], &[], &["1PATH"], &[]),
            "A variable name is letters, digits and underscores, not starting with a digit: 1PATH"
        );
        assert_eq!(
            said(&[], &[], &[], &["/tmp/../etc"]),
            "A reference path is absolute, without ..: /tmp/../etc"
        );

        let many: Vec<String> = (0..17).map(|i| format!("tool{i}")).collect();
        assert!(
            selection(&many, &[], &[], &paths(&[]))
                .unwrap_err()
                .error
                .starts_with("Select at most 16 tools")
        );
        assert!(selection(&names(&["cat"]), &[], &[], &paths(&["/usr/bin"])).is_ok());
    }
}

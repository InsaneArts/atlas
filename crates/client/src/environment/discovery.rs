//! Read-only inspection of explicit selections; never executes a selected tool
//! or sources a shell startup file. Presence is not application compatibility.
use super::{problem, variable_name as variable};
use crate::client::{Client, RUN_TIMEOUT, Run};
use atlas_core::{
    account::{SHARED_BIN, helper_search_path},
    error::{Code, Error, Result},
    session::Observation,
    transport::{OutputError, run_output},
};
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    fs,
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

        let reply = client.run(argv, Run::inspect().within(RUN_TIMEOUT)).await?;
        if reply["exitCode"] != 0 || reply["timedOut"] == true || !reply["signal"].is_null() {
            return Err(problem("Helper discovery did not complete"));
        }

        let output = run_output(&reply, 16384).map_err(|error| match error {
            OutputError::Malformed => invalid(),
            OutputError::Oversized => problem("Discovery output exceeded its limit"),
        })?;

        let lines = String::from_utf8(output)
            .map_err(|_| invalid())?
            .lines()
            .map(str::to_owned)
            .collect::<Vec<_>>();
        if lines.len() != args.len() {
            return Err(incomplete());
        }

        Ok(lines)
    }
}

/// The most of each kind one inspection may select.
const SELECTION_LIMIT: usize = 16;
/// Where macOS keeps its own apps that are not in /System/Applications.
const CORE_SERVICES: &str = "/System/Library/CoreServices";

fn invalid() -> Error {
    problem("The helper's discovery answer could not be read; run atlas doctor")
}

/// Refuse a selection with a sentence that names the item at fault, so an
/// agent corrects its command rather than guessing.
fn selection(
    tools: &[String],
    apps: &[String],
    environment: &[String],
    references: &[impl AsRef<Path>],
) -> Result<()> {
    let refuse = |message: String| Err(problem(message));

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

    for (names, expected) in [
        (
            tools,
            "A tool is named by its bare command name such as cat",
        ),
        (apps, "An app is named by its name such as Safari"),
    ] {
        if let Some((name, problem)) = names
            .iter()
            .find_map(|name| Some((name, name_problem(name)?)))
        {
            return refuse(format!("{expected}; this one {problem}: {name:?}"));
        }
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
    } else if Path::new(name).components().count() != 1
        || !matches!(
            Path::new(name).components().next(),
            Some(Component::Normal(_))
        )
    {
        Some("is a path")
    } else {
        None
    }
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Access {
    file: bool,
    pub(crate) directory: bool,
    pub(crate) readable: bool,
    writable: bool,
    pub(crate) executable: bool,
}

impl Access {
    pub(super) fn permits_reference(&self, writable: bool) -> bool {
        (self.file || self.directory)
            && self.readable
            && (!self.directory || self.executable)
            && (!writable || self.writable)
    }

    fn from_flags(flags: &str) -> Result<Self> {
        if !flags.starts_with('-') || flags.bytes().skip(1).any(|c| !b"fdrwx".contains(&c)) {
            return Err(problem("Invalid discovery probe response"));
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

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Tool,
    Application,
}

#[derive(Debug, Serialize)]
pub(crate) struct PathObservation {
    pub(crate) path: PathBuf,
    pub(crate) access: Access,
}

pub(crate) struct Candidate {
    pub(crate) kind: Kind,
    pub(crate) name: String,
    host: Option<PathBuf>,
    pub(crate) helper_candidates: Vec<PathObservation>,
}

impl Candidate {
    fn new(kind: Kind, name: &str, host: Option<PathBuf>, paths: Vec<PathBuf>) -> Self {
        // Preserve host serialization failures before any helper probes.
        let _ = json!(host);
        Self {
            kind,
            name: name.to_owned(),
            host,
            helper_candidates: paths
                .into_iter()
                .map(|path| PathObservation {
                    path,
                    access: Access::default(),
                })
                .collect(),
        }
    }

    pub(crate) fn available(&self) -> bool {
        self.helper_candidates.iter().any(|candidate| {
            let access = &candidate.access;
            match self.kind {
                Kind::Tool => access.file && access.executable,
                Kind::Application => access.directory && access.readable && access.executable,
            }
        })
    }

    fn report(&self) -> Value {
        let mut report = match self.kind {
            Kind::Tool => json!({
                "kind": "tool", "name": self.name, "host": self.host, "executed": false,
            }),
            Kind::Application => json!({
                "kind": "application", "name": self.name, "host": self.host,
                "launched": false, "accountData": "separate", "signIn": "not checked",
            }),
        };
        report["available"] = json!(self.available());
        report["helperCandidates"] = json!(self.helper_candidates);
        report
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Variable {
    name: String,
    host_defined: bool,
    helper_inherited_defined: bool,
}

#[derive(Serialize)]
pub(super) struct Reference {
    path: PathBuf,
    host: Access,
    pub(super) helper: Access,
}

pub(crate) struct Discovery {
    pub(crate) selections: Vec<Candidate>,
    helper_uid: u32,
    environment: Vec<Variable>,
    pub(super) references: Vec<Reference>,
}

impl Discovery {
    pub(super) fn report(&self) -> Value {
        let selections: Vec<_> = self.selections.iter().map(Candidate::report).collect();
        json!({
            "ok": true,
            "readOnly": true,
            "helperUID": self.helper_uid,
            "selections": selections,
            "environment": self.environment,
            "references": self.references,
            "scope": "Selected executable/app presence and access only. Shell startup files were \
                      not loaded; inherited variables may differ from Terminal. Dependencies, app \
                      data and sign-ins require separate live verification. No selected program \
                      was executed and no file contents or variable values were read into the \
                      report.",
        })
    }
}

fn candidates(
    tools: &[String],
    apps: &[String],
    home: &Path,
    helper_home: &Path,
) -> Vec<Candidate> {
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
    let helper_bins = helper_search_path(helper_home);

    tools
        .iter()
        .map(|name| {
            let host = host_bins.iter().map(|dir| dir.join(name)).find(|path| {
                let metadata = access(path);
                metadata.file && metadata.executable
            });
            let paths = helper_bins.iter().map(|dir| dir.join(name)).collect();
            Candidate::new(Kind::Tool, name, host, paths)
        })
        .chain(apps.iter().map(|name| application(name, home, helper_home)))
        .collect()
}

fn application(name: &str, home: &Path, helper_home: &Path) -> Candidate {
    let bundle = if name.ends_with(".app") {
        name.to_owned()
    } else {
        format!("{name}.app")
    };

    // Where apps live: the usual folders, and macOS's own in
    // CoreServices (Finder among them).
    let shared = ["/Applications", "/System/Applications", CORE_SERVICES]
        .map(|directory| Path::new(directory).join(&bundle));
    let host = shared
        .iter()
        .cloned()
        .chain([home.join("Applications").join(&bundle)])
        .find(|path| path.is_dir());
    Candidate::new(
        Kind::Application,
        name,
        host,
        shared
            .into_iter()
            .chain([helper_home.join("Applications").join(&bundle)])
            .collect(),
    )
}

fn incomplete() -> Error {
    problem("Incomplete discovery probe response")
}

/// Associate the already validated batch in its original candidate/reference order.
fn associate(
    candidates: &mut [Candidate],
    references: &[PathBuf],
    observed: Vec<Access>,
) -> Result<Vec<PathObservation>> {
    let expected = candidates
        .iter()
        .map(|candidate| candidate.helper_candidates.len())
        .sum::<usize>()
        + references.len();
    if observed.len() != expected {
        return Err(incomplete());
    }
    let mut observed = observed.into_iter();
    for (path, access) in candidates
        .iter_mut()
        .flat_map(|candidate| &mut candidate.helper_candidates)
        .zip(observed.by_ref())
    {
        path.access = access;
    }
    Ok(references
        .iter()
        .zip(observed)
        .map(|(path, access)| PathObservation {
            path: path.clone(),
            access,
        })
        .collect())
}

pub(crate) async fn inspect(
    client: &Client,
    tools: &[String],
    apps: &[String],
    environment: &[String],
    references: &[PathBuf],
) -> Result<Discovery> {
    selection(tools, apps, environment, references)?;

    let status = Observation::decode(&client.status().await);
    let session = status.require_hidden(client.config.helper_uid)?;
    let helper_home = status.helper_home()?.to_owned();
    let home = atlas_core::config::home()?;
    let mut selections = candidates(tools, apps, &home, &helper_home);
    let args = selections
        .iter()
        .flat_map(|candidate| candidate.helper_candidates.iter().map(|path| &path.path))
        .chain(references)
        .map(|p| p.to_str().map(str::to_owned).ok_or_else(invalid))
        .collect::<Result<Vec<_>>>()?;
    let observed = Probe::Paths
        .run(client, &args)
        .await?
        .iter()
        .map(|s| Access::from_flags(s))
        .collect::<Result<Vec<_>>>()?;
    let references = associate(&mut selections, references, observed)?;

    let defined = Probe::Variables.run(client, environment).await?;
    if defined.iter().any(|s| s != "defined" && s != "undefined") {
        return Err(invalid());
    }

    if client.identity().await?.epoch != session.epoch {
        return Err(Error::new(
            Code::Identity,
            "Helper changed during discovery; repeat the inspection",
        ));
    }

    // Probe::run has checked exact cardinality before any association.
    let environment = environment
        .iter()
        .zip(defined)
        .map(|(name, value)| Variable {
            name: name.clone(),
            host_defined: std::env::var_os(name).is_some(),
            helper_inherited_defined: value == "defined",
        })
        .collect();
    let references = references
        .into_iter()
        .map(|observed| Reference {
            host: access(&observed.path),
            path: observed.path,
            helper: observed.access,
        })
        .collect();
    Ok(Discovery {
        selections,
        helper_uid: session.epoch.uid,
        environment,
        references,
    })
}

/// Inspect selected names and paths without installing, launching or signing in.
///
/// # Errors
///
/// Rejects invalid tool or app names, unavailable helper identity and incomplete discovery
/// replies. Discovery reports access facts without executing selected programs.
pub async fn discover(
    client: &Client,
    tools: &[String],
    apps: &[String],
    environment: &[String],
    references: &[PathBuf],
) -> Result<Value> {
    Ok(inspect(client, tools, apps, environment, references)
        .await?
        .report())
}

#[cfg(test)]
mod tests;

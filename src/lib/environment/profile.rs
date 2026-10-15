//! Explicit environment selections. Values are written only to a private file;
//! reports contain names and readiness observations, never variable values.
use super::{Selection, discovery, freeze, problem, selectable_variable};
use crate::{client::Client, error::Result};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    os::unix::fs::DirBuilderExt,
    path::{Path, PathBuf},
};

const ENVIRONMENT_TARGET: &str = ".config/atlas/environment.json";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Profile {
    version: u32,
    #[serde(default)]
    files: Vec<Selection>,
    #[serde(default)]
    tools: Vec<String>,
    #[serde(default)]
    applications: Vec<String>,
    #[serde(default)]
    environment: Vec<String>,
    #[serde(default)]
    references: Vec<Reference>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Reference {
    path: PathBuf,
    #[serde(default)]
    writable: bool,
}

/// Read, parse and validate a TOML profile.
pub(super) fn load(path: &Path) -> Result<Profile> {
    let profile: Profile =
        crate::config::parse_toml(&super::files::read_limited(path, 16384)?, path)?;
    validate(&profile)?;
    Ok(profile)
}

fn validate(profile: &Profile) -> Result<()> {
    if profile.version != 2 || profile.files.len() > 31 || profile.environment.len() > 16 {
        return Err(problem(
            "Expected profile version 2, at most 31 files and 16 environment variables",
        ));
    }
    let mut names = BTreeSet::new();
    if profile
        .environment
        .iter()
        .any(|name| !selectable_variable(name) || !names.insert(name))
    {
        return Err(problem(
            "Select unique environment variable names; helper identity, home, temporary directory and PATH are managed separately",
        ));
    }
    super::files::unique_targets(
        profile
            .files
            .iter()
            .map(|file| file.target.as_path())
            .chain([Path::new(ENVIRONMENT_TARGET)]),
    )
}

/// Freeze only the exported variables explicitly named by the profile. Commands
/// launched with `environment run` consume them; existing GUI apps do not.
pub async fn sync(client: &Client, path: &Path, apply: bool) -> Result<Value> {
    let path = fs::canonicalize(path)?;
    let profile = load(&path)?;
    let mut variables = BTreeMap::new();
    for name in &profile.environment {
        let value = std::env::var(name).map_err(|_| {
            problem(format!(
                "Selected variable {name} is missing or is not UTF-8 in this controller process"
            ))
        })?;
        if value.contains('\0') || value.len() > 65536 {
            return Err(problem(
                "Selected environment contains an unsupported value",
            ));
        }
        variables.insert(name.clone(), value);
    }
    let bytes = serde_json::to_vec(&json!({"version":1,"variables":variables}))?;
    if bytes.len() > 65536 {
        return Err(problem("Selected environment exceeds 64 KiB"));
    }
    let references = profile
        .references
        .iter()
        .map(|reference| reference.path.clone())
        .collect::<Vec<_>>();
    let readiness = if profile.tools.is_empty()
        && profile.applications.is_empty()
        && profile.environment.is_empty()
        && references.is_empty()
    {
        json!({"ok":true,"readOnly":true,"selections":[],"environment":[],"references":[]})
    } else {
        discovery::discover(
            client,
            &profile.tools,
            &profile.applications,
            &profile.environment,
            &references,
        )
        .await?
    };
    for (reference, observed) in profile
        .references
        .iter()
        .zip(readiness["references"].as_array().into_iter().flatten())
    {
        let access = &observed["helper"];
        if (access["file"] != true && access["directory"] != true)
            || access["readable"] != true
            || (access["directory"] == true && access["executable"] != true)
            || (reference.writable && access["writable"] != true)
        {
            return Err(problem(format!(
                "Selected reference is not accessible to the helper with the requested permissions: {}",
                reference.path.display()
            )));
        }
    }
    let temporary = fs::canonicalize(client.output_dir()?)?
        .join(format!("environment-profile-{}", crate::control::token()));
    fs::DirBuilder::new().mode(0o700).create(&temporary)?;
    let generated = temporary.join("environment.json");
    let result: Result<Value> = async {
        super::files::remove_inherited_acl(&temporary)?;
        super::files::atomic_write(&generated, &bytes, 0o600)?;
        let mut selections = profile.files;
        selections.push(Selection {
            source: generated,
            target: ENVIRONMENT_TARGET.into(),
            executable: false,
        });
        let mut result = freeze(client, &path, selections, apply).await?;
        result["profileVersion"] = json!(2);
        result["environmentNames"] = json!(profile.environment);
        result["readinessBeforeApply"] = readiness;
        result["environmentScope"] = json!(
            "Selected exported values are frozen privately and used by environment run. \
             Existing GUI apps and interactive shell startup files are not modified. \
             Application logins remain separate."
        );
        Ok(result)
    }
    .await;
    // freeze has copied the generated bytes into its own retained plan before it
    // can dispatch an apply. Removing this source cannot invalidate recovery.
    let cleanup = fs::remove_dir_all(&temporary);
    match result {
        Ok(mut value) => {
            if cleanup.is_err() {
                value["cleanupWarning"] = json!(format!(
                    "The operation result is known, but a private generated source remains at {}",
                    temporary.display()
                ));
            }
            Ok(value)
        }
        Err(mut error) => {
            if cleanup.is_err() {
                error.error.push_str(&format!(
                    " Private generated source cleanup is incomplete at {}.",
                    temporary.display()
                ));
            }
            Err(error)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile(value: Value) -> Profile {
        serde_json::from_value(value).unwrap()
    }

    use crate::environment::RESERVED_VARIABLES;

    #[test]
    fn profile_cannot_shadow_helper_identity_or_generated_state() {
        for name in RESERVED_VARIABLES {
            assert!(validate(&profile(json!({"version":2,"environment":[name]}))).is_err());
        }
        assert!(
            validate(&profile(
                json!({"version":2,"files":[{"source":"x","target":ENVIRONMENT_TARGET}]})
            ))
            .is_err()
        );
        assert!(validate(&profile(json!({"version":2,"environment":["KEY","KEY"]}))).is_err());
        assert!(validate(&profile(json!({"version":2,"environment":["KEY=value"]}))).is_err());
        assert!(
            validate(&profile(
                json!({"version":2,"environment":["OLLAMA_MODELS"]})
            ))
            .is_ok()
        );
    }
}

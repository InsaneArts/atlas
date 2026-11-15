//! Explicit environment selections. Values are written only to a private file;
//! reports contain names and readiness observations, never variable values.
use super::{
    Kind, Selection, problem,
    runner::{ENVIRONMENT_TARGET, Environment},
    selectable_variable,
};
use crate::client::Client;
use atlas_core::error::Result;
use serde::Deserialize;
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Profile {
    version: u32,
    #[serde(default)]
    pub(crate) files: Vec<Selection>,
    #[serde(default)]
    pub(crate) tools: Vec<String>,
    #[serde(default)]
    pub(crate) applications: Vec<String>,
    #[serde(default)]
    pub(crate) environment: Vec<String>,
    #[serde(default)]
    pub(crate) references: Vec<Reference>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Reference {
    pub(crate) path: PathBuf,
    #[serde(default)]
    pub(crate) writable: bool,
}

/// Read, parse and validate a TOML profile, through the one guarded reader.
pub(crate) fn load(path: &Path) -> Result<Profile> {
    let reading = atlas_core::checked::read(path)?;
    let resolved = reading.path().to_path_buf();
    let bytes = reading.read_up_to(super::MAX_MANIFEST + 1)?;
    if bytes.len() as u64 > super::MAX_MANIFEST {
        return Err(problem("The profile is too large"));
    }
    let profile: Profile = atlas_core::config::parse_toml(&bytes, &resolved)?;
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

/// The environment file for the variables `profile` selects, with their
/// values in this process.
pub(crate) fn selected_environment(profile: &Profile) -> Result<Vec<u8>> {
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

    let bytes = serde_json::to_vec(&Environment {
        variables,
        version: 1,
    })?;
    if bytes.len() > 65536 {
        return Err(problem("Selected environment exceeds 64 KiB"));
    }

    Ok(bytes)
}

/// Refuses `profile`, read from `path`, when a file it copies or a folder it lets the helper read is one of the
/// person's stores of sign-ins or secrets, before the helper is asked anything.
fn refuse_credentials(path: &Path, profile: &Profile) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| problem("Missing profile parent"))?;
    // An absolute source replaces the profile's folder as it is joined.
    let files = profile.files.iter().map(|file| parent.join(&file.source));
    // A relative reference is refused as such by discovery, as before.
    let references = profile
        .references
        .iter()
        .filter(|reference| reference.path.is_absolute())
        .map(|reference| reference.path.clone());
    let selected: Vec<PathBuf> = files.chain(references).collect();
    crate::credential_files::refuse(selected.iter().map(PathBuf::as_path))
}

/// Freeze only the exported variables explicitly named by the profile. Commands
/// launched with `environment run` consume them; existing GUI apps do not.
///
/// # Errors
///
/// Returns profile parsing, discovery, access-validation or staging failures. Applied
/// selections can retain incomplete transactions and require recovery.
pub async fn sync(client: &Client, path: &Path, apply: bool) -> Result<Value> {
    // Atlas's own folder is refused on the named path before anything reads
    // it; `load` then reads through the guarded reader, and a profile's own
    // sources and references are refused as the person's sign-ins and secrets.
    atlas_core::own_folder::refuse(path)?;
    let path = fs::canonicalize(path)?;
    let profile = load(&path)?;
    refuse_credentials(&path, &profile)?;
    let bytes = selected_environment(&profile)?;
    match client.desktop().kind() {
        Kind::User(login) => {
            crate::user::environment::sync_in(client, login, path, profile, bytes, apply).await
        }
        Kind::Vm(_) => Err(client.unreachable()),
    }
}

#[cfg(test)]
#[path = "profile_tests.rs"]
mod tests;

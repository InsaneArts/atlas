//! A helper-local environment, loaded without shell evaluation or secret argv.
use super::*;
use std::{os::unix::process::CommandExt, process::Command};

/// The code of the refusal when no environment profile is installed:
/// callers fall back to the helper's PATH on it.
pub(crate) const NO_PROFILE: Code = Code::NoEnvironment;

/// Where a helper's environment profile is, in its home.
pub(super) const ENVIRONMENT_TARGET: &str = ".config/atlas/environment.json";

/// The profile a command runs with. Its fields stay in the order the file
/// has always been written in.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Environment {
    pub(super) variables: BTreeMap<String, String>,
    pub(super) version: u32,
}

pub(super) fn validate_argv(argv: &[String]) -> Result<()> {
    if argv.is_empty()
        || argv[0].is_empty()
        || argv.len() > 128
        || argv.iter().any(|arg| arg.contains('\0'))
        || argv.iter().map(String::len).sum::<usize>() > 65536
    {
        return Err(problem(
            "Select a command with at most 128 bounded, NUL-free arguments",
        ));
    }

    Ok(())
}

fn load(home: &Path) -> Result<BTreeMap<String, String>> {
    let file = open_regular(&home.join(ENVIRONMENT_TARGET), false)?.ok_or_else(|| {
        Error::new(
            NO_PROFILE,
            "No helper environment profile is installed. Use atlas run instead, or run \
             atlas environment profile to set one up.",
        )
    })?;
    validate_private(&file, false)?;

    let environment: Environment = decode(&read_file(file, 65536)?)?;
    if environment.version != 1 || environment.variables.len() > 64 {
        return Err(problem("Unsupported helper environment profile"));
    }

    for (name, value) in &environment.variables {
        if !selectable_variable(name) || value.contains('\0') {
            return Err(problem(
                "Helper environment contains an invalid or reserved variable",
            ));
        }
    }

    Ok(environment.variables)
}

pub(super) fn execute(
    home: &Path,
    argv: &[String],
    guard: &impl Fn() -> Result<()>,
) -> Result<Value> {
    validate_argv(argv)?;
    let variables = load(home)?;
    let path = std::env::join_paths(helper_search_path(home))
        .map_err(|_| problem("Invalid helper command path"))?;

    let mut command = Command::new(&argv[0]);
    command
        .args(&argv[1..])
        .envs(variables)
        .env("HOME", home)
        .env("PATH", path)
        .current_dir(home);

    guard()?;
    // exec replaces this worker: there is no detached child to outlive the
    // existing helper transport's timeout, signals, stdout or stderr handling.
    let error = command.exec();

    Err(problem(format!(
        "Could not execute the helper command: {}",
        error.kind()
    ))
    .with_source(error))
}

#[cfg(test)]
mod tests;

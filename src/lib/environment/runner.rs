//! A helper-local environment, loaded without shell evaluation or secret argv.
use super::*;
use std::{os::unix::process::CommandExt, process::Command};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Environment {
    version: u32,
    variables: BTreeMap<String, String>,
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
    let path = home.join(".config/atlas/environment.json");
    let file = open_regular(&path, false)?.ok_or_else(|| {
        problem(
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
    let path = std::env::join_paths([
        home.join(".local/bin"),
        home.join(".cargo/bin"),
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/usr/local/bin"),
        PathBuf::from("/usr/bin"),
        PathBuf::from("/bin"),
        PathBuf::from("/usr/sbin"),
        PathBuf::from("/sbin"),
    ])
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
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn profile_file_is_private_bounded_and_never_evaluated_as_shell() {
        let root = tempfile::tempdir().unwrap();
        let home = fs::canonicalize(root.path()).unwrap();
        let path = home.join(".config/atlas/environment.json");
        let literal = "$(touch should-not-run); private value";
        atomic_write(
            &path,
            &serde_json::to_vec(&json!({"version": 1, "variables": {"SELECTED": literal}}))
                .unwrap(),
            0o600,
        )
        .unwrap();
        assert_eq!(load(&home).unwrap()["SELECTED"], literal);
        assert!(!home.join("should-not-run").exists());
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(load(&home).is_err());
        for name in [
            "HOME", "USER", "LOGNAME", "SHELL", "PATH", "UID", "EUID", "TMPDIR", "1BAD", "BAD-NAME",
        ] {
            atomic_write(
                &path,
                &serde_json::to_vec(&json!({"version": 1, "variables": {name: "secret"}})).unwrap(),
                0o600,
            )
            .unwrap();
            let error = load(&home).unwrap_err();
            assert!(!error.error.contains("secret"));
        }
        assert!(validate_argv(&[]).is_err());
        assert!(validate_argv(&["tool\0name".into()]).is_err());
    }

    #[test]
    fn identity_is_rechecked_after_loading_profile_before_exec() {
        let root = tempfile::tempdir().unwrap();
        let home = fs::canonicalize(root.path()).unwrap();
        atomic_write(
            &home.join(".config/atlas/environment.json"),
            b"{\"version\":1,\"variables\":{}}",
            0o600,
        )
        .unwrap();
        let error = execute(&home, &["/usr/bin/true".into()], &|| {
            Err(problem("session changed"))
        })
        .unwrap_err();
        assert_eq!(error.error, "session changed");
    }
}

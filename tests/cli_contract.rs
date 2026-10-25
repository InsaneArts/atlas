use serde_json::{Value, json};
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
};

struct Fixture {
    directory: tempfile::TempDir,
}

impl Fixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("broken.toml"), "helper_uid = [\n").unwrap();
        Self { directory }
    }

    fn config(&self, name: &str) -> PathBuf {
        self.directory.path().join(name)
    }

    fn files(&self) -> Vec<(OsString, Vec<u8>)> {
        let mut files = std::fs::read_dir(self.directory.path())
            .unwrap()
            .map(|entry| {
                let entry = entry.unwrap();
                (entry.file_name(), std::fs::read(entry.path()).unwrap())
            })
            .collect::<Vec<_>>();
        files.sort();
        files
    }

    fn run(&self, config: &Path, args: &[&str], atlas_json: Option<&str>) -> Output {
        let before = self.files();
        let mut command = Command::new(env!("CARGO_BIN_EXE_atlas"));
        command
            .current_dir(self.directory.path())
            .env_clear()
            .envs([
                ("PATH", "/usr/bin:/bin:/usr/sbin:/sbin"),
                ("LANG", "C"),
                ("LC_ALL", "C"),
                ("TERM", "dumb"),
                ("NO_COLOR", "1"),
            ])
            .arg("--config")
            .arg(config)
            .args(args)
            .stdin(Stdio::null());
        if let Some(value) = atlas_json {
            command.env("ATLAS_JSON", value);
        }

        let output = command.output().unwrap();
        assert_eq!(
            self.files(),
            before,
            "a nonmutating command changed its fixtures"
        );
        output
    }

    fn missing(&self, args: &[&str]) -> Output {
        self.run(&self.config("missing.toml"), args, None)
    }
}

#[test]
fn help_is_plain_grouped_text_without_loading_settings() {
    let fixture = Fixture::new();
    let output = fixture.missing(&["--help"]);
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    let help = std::str::from_utf8(&output.stdout).unwrap();
    for heading in [
        "Usage:",
        "Get started:",
        "Look at the helper:",
        "Act in the helper:",
        "Prepare the helper:",
        "Take over or recover:",
        "Look after Atlas:",
        "Global options:",
    ] {
        assert!(help.contains(heading), "missing {heading}");
    }
    assert!(
        !help.contains('\u{1b}'),
        "piped help must not contain ANSI escapes"
    );
    let broken = fixture.run(&fixture.config("broken.toml"), &["--help"], None);
    assert_eq!(broken.status.code(), Some(0));
    assert_eq!(broken.stdout, output.stdout);
    assert!(broken.stderr.is_empty());
}

#[test]
fn version_is_plain_text_without_loading_settings() {
    let fixture = Fixture::new();
    let output = fixture.missing(&["--version"]);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        output.stdout,
        concat!("atlas ", env!("CARGO_PKG_VERSION"), "\n").as_bytes()
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn invalid_arguments_report_one_json_error_before_execution() {
    let fixture = Fixture::new();
    let output = fixture.missing(&["run", "--timeout", "0", "--", "/usr/bin/true"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    let error: Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(error["ok"], false);
    assert_eq!(error["code"], "usage");
    assert!(
        error["error"]
            .as_str()
            .unwrap()
            .contains("Give 1 to 600 seconds.")
    );
}

#[test]
fn the_agent_reference_is_raw_markdown_without_an_extra_result() {
    let fixture = Fixture::new();
    let output = fixture.missing(&["agent-guide", "--commands"]);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        output.stdout,
        include_bytes!("../agents/atlas/skills/atlas/references/commands.md")
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn a_broken_configuration_reports_a_json_failure_on_stderr() {
    let fixture = Fixture::new();
    let output = fixture.run(&fixture.config("broken.toml"), &["config", "--json"], None);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let error: Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(error["ok"], false);
    assert_eq!(error["code"], "config-invalid");
    assert_eq!(error["uncertain"], false);
    let message = error["error"].as_str().unwrap();
    assert!(message.starts_with("Atlas could not read its settings in "));
    assert!(message.contains("broken.toml"));
}

fn assert_missing_update_finish(output: Output) {
    if nix::unistd::geteuid().is_root() {
        assert_eq!(output.status.code(), Some(1));
        assert!(output.stdout.is_empty());
        assert_eq!(
            serde_json::from_slice::<Value>(&output.stderr).unwrap(),
            json!({"ok": false, "code": "usage", "error": "Run atlas update as yourself, not with sudo.", "uncertain": false})
        );
    } else {
        assert_eq!(output.status.code(), Some(0));
        assert_eq!(
            serde_json::from_slice::<Value>(&output.stdout).unwrap(),
            json!({"ok": true, "next": "atlas init"})
        );
        assert!(output.stderr.is_empty());
    }
}

#[test]
fn finishing_an_update_without_settings_is_a_nonmutating_noop() {
    let fixture = Fixture::new();
    assert_missing_update_finish(fixture.missing(&["update-finish"]));
}

#[test]
fn update_finish_keeps_machine_json_even_when_human_output_is_requested() {
    let fixture = Fixture::new();
    assert_missing_update_finish(fixture.run(
        &fixture.config("missing.toml"),
        &["update-finish"],
        Some("0"),
    ));
}

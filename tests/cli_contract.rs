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
        self.run_line(Some(config), args, atlas_json)
    }

    fn run_line(&self, config: Option<&Path>, args: &[&str], atlas_json: Option<&str>) -> Output {
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
            .env("ATLAS_CONFIG", self.config("broken.toml"))
            .stdin(Stdio::null());
        if let Some(config) = config {
            command.arg("--config").arg(config);
        }
        command.args(args);
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
fn usage_names_the_parsed_command_without_reading_command_shaped_settings() {
    let fixture = Fixture::new();
    std::fs::write(fixture.config("update"), "helper_uid = [\n").unwrap();
    for (args, name) in [
        (vec!["--config", "update", "status", "--unknown"], "status"),
        (vec!["status", "--config", "update", "--unknown"], "status"),
        (vec!["--config=update", "status", "--unknown"], "status"),
        (
            vec!["environment", "--config", "update", "rollback", "--unknown"],
            "environment",
        ),
        (
            vec!["--config", "update", "environment", "rollback", "--unknown"],
            "environment rollback",
        ),
        (
            vec!["--config", "update", "capture", "--unknown"],
            "screenshot",
        ),
        (vec!["--config", "update", "helper-serve", "--unknown"], ""),
        (vec!["status", "--", "update"], "status"),
    ] {
        let output = fixture.run_line(None, &args, None);
        assert_eq!(output.status.code(), Some(2), "{args:?}");
        assert!(output.stdout.is_empty(), "{args:?}");
        let error: Value = serde_json::from_slice(&output.stderr).unwrap();
        assert_eq!(error["ok"], false);
        assert_eq!(error["code"], "usage");
        assert!(error["next"].is_string(), "{args:?}");
        let command = if name.is_empty() {
            "atlas".to_owned()
        } else {
            format!("atlas {name}")
        };
        let expected = if args.contains(&"--") {
            format!("{command} does not take update.")
        } else {
            format!("{command} has no option --unknown.")
        };
        assert_eq!(error["error"], expected, "{args:?}");

        let human = fixture.run_line(None, &args, Some("0"));
        assert_eq!(human.status.code(), Some(2));
        assert!(human.stdout.is_empty());
        let message = std::str::from_utf8(&human.stderr).unwrap();
        assert!(message.contains(&expected), "{args:?}: {message}");
        assert!(message.contains(&format!("{command} --help")));
    }
}

#[test]
fn only_atlas_json_selects_json_usage_errors() {
    let fixture = Fixture::new();
    for command in [vec!["run"], vec!["environment", "run"]] {
        for (json_before, json_after) in [(false, false), (true, false), (false, true)] {
            let mut args = command.clone();
            if json_before {
                args.push("--json");
            }
            args.extend(["--timeout", "0"]);
            if json_after {
                args.push("--json");
            }
            args.extend(["--", "/usr/bin/true", "--json"]);
            let output = fixture.run_line(None, &args, Some("0"));
            assert_eq!(output.status.code(), Some(2), "{args:?}");
            assert!(output.stdout.is_empty());
            if json_before || json_after {
                let error: Value = serde_json::from_slice(&output.stderr).unwrap();
                assert_eq!(error["code"], "usage");
                assert_eq!(error["ok"], false);
                assert!(error["next"].is_string());
                assert!(
                    error["error"]
                        .as_str()
                        .unwrap()
                        .contains("Give 1 to 600 seconds.")
                );
            } else {
                let error = std::str::from_utf8(&output.stderr).unwrap();
                assert!(serde_json::from_str::<Value>(error).is_err());
                assert!(error.contains("Give 1 to 600 seconds."));
                assert!(error.contains(&format!("atlas {} --help", command.join(" "))));
            }
        }
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
    assert!(error["next"].is_string());
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
    assert!(error["next"].is_string());
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
            json!({"ok": false, "code": "usage", "error": "Run atlas update as yourself, not with sudo.", "uncertain": false, "next": "Add --help to the command to see what it accepts."})
        );
    } else {
        assert_eq!(output.status.code(), Some(0));
        assert_eq!(output.stdout, b"{\"next\":\"atlas init\",\"ok\":true}\n");
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

#[test]
fn pipelined_cursor_initialization_guards_an_imported_plain_mcp_connection() {
    use std::io::Write;

    let fixture = Fixture::new();
    let config = fixture.config("mcp.toml");
    std::fs::write(
        &config,
        "helper_uid = 60001\nsocket_path = \"absent.sock\"\nidentity_probe = \"absent\"\ninput_socket = \"input.sock\"\noutput_dir = \"out\"\n",
    )
    .unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_atlas"))
        .args(["--config", config.to_str().unwrap(), "mcp"])
        .env("HOME", fixture.directory.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    let messages = [
        json!({"id":1,"method":"initialize","params":{"clientInfo":{"name":"cursor-vscode","version":"1.0.0"},"capabilities":{"extensions":{"io.modelcontextprotocol/ui":{}}}}}),
        json!({"id":2,"method":"tools/list","params":{}}),
        json!({"id":3,"method":"initialize","params":{"clientInfo":{"name":"another host"},"capabilities":{"extensions":{"io.modelcontextprotocol/ui":{}}}}}),
        json!({"id":4,"method":"tools/list","params":{}}),
    ];
    // One write, without waiting for initialize's response, exercises the
    // actual concurrent stdio transport used by imported plugin servers.
    let lines = messages
        .iter()
        .map(|message| format!("{message}\n"))
        .collect::<String>();
    input.write_all(lines.as_bytes()).unwrap();
    drop(input);
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let replies = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    for id in [2, 4] {
        let reply = replies.iter().find(|reply| reply["id"] == id).unwrap();
        let tools = reply["result"]["tools"].as_array().unwrap();
        assert!(tools.iter().any(|tool| tool["name"] == "view_close"));
        assert!(
            tools
                .iter()
                .all(|tool| { tool["inputSchema"]["properties"]["_atlasView"].is_object() })
        );
    }
}

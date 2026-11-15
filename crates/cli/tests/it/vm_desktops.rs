//! A VM desktop named in the settings, which this build cannot reach yet:
//! every command on it is refused in one wording, and what a refused
//! command records lands in the VM's own records, never in Second Login's.
use serde_json::Value;
use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

const VM: &str = "\n[[desktops]]\nname = \"work\"\nkind = \"vm\"\n\
                  id = \"0123456789abcdef01234567\"\n";
const USER: &str = "\n[[desktops]]\nname = \"main\"\nkind = \"user\"\nhelper_uid = 60001\n\
                    socket_path = \"absent.sock\"\nidentity_probe = \"absent\"\n\
                    input_socket = \"absent-input.sock\"\n";

/// Settings in a folder of their own: `text`, written as `atlas.toml`.
fn settings(text: &str) -> (tempfile::TempDir, PathBuf) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("atlas.toml");
    std::fs::write(&path, text).unwrap();
    (directory, path)
}

/// `atlas --desktop work <command>` with the settings at `path`, its home
/// and bindings in `home`: what it printed.
fn on_the_vm(path: &Path, home: &Path, command: &str) -> Value {
    let output = atlas_core::spawn::spawn(
        Command::new(env!("CARGO_BIN_EXE_atlas"))
            .env_clear()
            .env("PATH", "/usr/bin:/bin:/usr/sbin:/sbin")
            .env("HOME", home)
            .env(atlas_client::host_session::desktops::TEST_HOME, home)
            .env("ATLAS_JSON", "1")
            .args([
                "--config",
                path.to_str().unwrap(),
                "--desktop",
                "work",
                command,
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped()),
    )
    .and_then(std::process::Child::wait_with_output)
    .unwrap();
    assert!(!output.status.success(), "{command} was refused");
    let printed = if output.stdout.is_empty() {
        &output.stderr
    } else {
        &output.stdout
    };
    serde_json::from_slice(printed).unwrap()
}

/// What the output folder `root` holds at its top, by name.
fn top_of(root: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(root)
        .map(|entries| {
            entries
                .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

#[test]
fn every_command_on_a_vm_desktop_is_refused_naming_the_renamed_second_login() {
    let (directory, path) = settings(&format!(
        "default = \"main\"\noutput_dir = \"out\"\n{USER}{VM}"
    ));
    let home = directory.path().join("home");
    for command in ["status", "screenshot", "doctor"] {
        let refused = on_the_vm(&path, &home, command);
        assert_eq!(refused["ok"], false, "{command}: {refused}");
        assert_eq!(refused["code"], "platform", "{command}: {refused}");
        assert_eq!(
            refused["error"], "work is a VM desktop, which this build of Atlas cannot reach yet.",
            "{command}"
        );
        assert_eq!(
            refused["next"], "Run it again with --desktop main to use Second Login.",
            "{command}"
        );
    }
    // Listing two desktops binds Second Login's earlier sessions once,
    // which marks the folder; nothing else lands at its root.
    assert_eq!(
        top_of(&directory.path().join("out")),
        ["desktops", "desktops-seeded"]
    );
}

#[test]
fn every_command_on_a_vm_desktop_alone_is_refused_with_no_other_desktop_to_try() {
    let (directory, path) = settings(&format!("default = \"work\"\noutput_dir = \"out\"\n{VM}"));
    let home = directory.path().join("home");
    for command in ["status", "screenshot", "doctor"] {
        let refused = on_the_vm(&path, &home, command);
        assert_eq!(refused["code"], "platform", "{command}: {refused}");
        assert_eq!(
            refused["error"], "work is a VM desktop, which this build of Atlas cannot reach yet.",
            "{command}"
        );
        assert!(
            !refused["next"].as_str().unwrap().contains("--desktop"),
            "{command}: {refused}"
        );
    }
    let root = directory.path().join("out");
    assert_eq!(top_of(&root), ["desktops"]);
    assert_eq!(
        top_of(&root.join("desktops")),
        ["vm-0123456789abcdef01234567"]
    );
}

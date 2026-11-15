//! What an agent running `atlas` on the person's Mac can never reach: Atlas's
//! own folder, as settings or any other file, and the workers Atlas starts
//! for itself inside a desktop.
use serde_json::Value;
use std::{
    io::{Read, Write},
    os::fd::OwnedFd,
    path::Path,
    process::{Command, Stdio},
};

/// `atlas` with `arguments`, as an agent in this account runs it: what it
/// printed, and whether it succeeded.
fn atlas(arguments: &[&str]) -> (bool, Value) {
    let output = atlas_core::spawn::spawn(
        Command::new(env!("CARGO_BIN_EXE_atlas"))
            .env("ATLAS_JSON", "1")
            .args(arguments)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped()),
    )
    .and_then(std::process::Child::wait_with_output)
    .unwrap();
    let printed = if output.stdout.is_empty() {
        &output.stderr
    } else {
        &output.stdout
    };
    let said = String::from_utf8_lossy(printed);
    let last = said.lines().last().unwrap_or_default();
    (output.status.success(), serde_json::from_str(last).unwrap())
}

#[test]
fn settings_given_from_atlas_own_folder_are_refused_by_every_command() {
    let folder = atlas_core::config::home()
        .unwrap()
        .join(atlas_core::own_folder::FOLDER);
    for settings in [
        folder.join("VMs/0123456789abcdef01234567/Share/pairing-secret"),
        folder.clone(),
    ] {
        for command in ["status", "doctor", "config"] {
            let (succeeded, said) = atlas(&["--config", settings.to_str().unwrap(), command]);
            assert!(!succeeded, "{command}: {said}");
            assert_eq!(said["code"], "credentials", "{command}: {said}");
            assert_eq!(said["error"], atlas_core::own_folder::REFUSAL, "{command}");
        }
    }
}

/// `atlas job-launch` running `/usr/bin/touch` on `marker`, told to go on
/// on `input`: what it said on its status pipe, and its exit status.
fn launched(marker: &Path, input: Stdio, go_on: impl FnOnce()) -> (String, Option<i32>) {
    let directory = marker.parent().unwrap();
    let log = directory.join("job.log");
    std::fs::write(&log, b"").unwrap();
    let mut child = atlas_core::spawn::spawn(
        Command::new(env!("CARGO_BIN_EXE_atlas"))
            .arg("job-launch")
            .arg(format!("--log={}", log.display()))
            .arg("--argv0=touch")
            .arg("--")
            .arg("/usr/bin/touch")
            .arg(marker)
            .stdin(input)
            .stdout(Stdio::piped())
            .stderr(Stdio::null()),
    )
    .unwrap();
    go_on();
    let mut said = String::new();
    child
        .stdout
        .take()
        .unwrap()
        .read_to_string(&mut said)
        .unwrap();
    (said, child.wait().unwrap().code())
}

#[test]
fn a_job_launcher_run_by_hand_is_refused_and_runs_nothing() {
    let directory = tempfile::tempdir().unwrap();
    let marker = directory.path().join("ran");

    let (reading, mut writing) = atlas_core::spawn::pipe().unwrap();
    let (said, status) = launched(&marker, Stdio::from(OwnedFd::from(reading)), || {
        let _ = writing.write_all(b"go\n");
    });
    assert_eq!(status, Some(127), "{said}");
    assert_eq!(
        said.trim_end(),
        format!("error {}", atlas_daemon::worker::REFUSAL)
    );

    let (mut ours, theirs) = atlas_core::spawn::socket_pair().unwrap();
    let (said, status) = launched(&marker, Stdio::from(OwnedFd::from(theirs)), || {
        let _ = ours.write_all(b"go\n");
    });
    assert_eq!(status, Some(127), "{said}");
    assert!(said.contains(atlas_daemon::worker::REFUSAL), "{said}");

    assert!(!marker.exists(), "the program ran");
}

#[test]
fn an_environment_worker_run_by_hand_is_refused_before_it_reads_its_packet() {
    let directory = tempfile::tempdir().unwrap();
    let packet = directory.path().join("request/packet.json");

    let (succeeded, said) = atlas(&["environment-worker", packet.to_str().unwrap(), "--apply"]);
    assert!(!succeeded, "{said}");
    assert_eq!(said["code"], "denied", "{said}");
    assert_eq!(said["error"], atlas_daemon::worker::REFUSAL);
}

#[test]
fn an_actions_file_in_atlas_own_folder_is_refused_before_it_is_read() {
    let folder = atlas_core::config::home()
        .unwrap()
        .join(atlas_core::own_folder::FOLDER);
    let file = folder.join("VMs/0123456789abcdef01234567/Share/pairing-secret");

    let (succeeded, said) = atlas(&["input", &format!("@{}", file.display())]);
    assert!(!succeeded, "{said}");
    assert_eq!(said["code"], "credentials", "{said}");
    assert_eq!(said["error"], atlas_core::own_folder::REFUSAL);
}

#[test]
fn a_malformed_actions_file_never_echoes_what_it_holds() {
    let directory = tempfile::tempdir().unwrap();
    let canary = "CANARY-actions-6b1f";
    for body in [
        format!("{canary}\n"),
        format!("[{{\"type\": \"{canary}\"}}]\n"),
        format!("[{{\"type\": \"move\", \"x\": \"{canary}\"}}]\n"),
    ] {
        let file = directory.path().join("batch.json");
        std::fs::write(&file, &body).unwrap();
        let (succeeded, said) = atlas(&["input", &format!("@{}", file.display())]);
        assert!(!succeeded, "{body}: {said}");
        let text = said.to_string();
        assert!(!text.contains(canary), "{body}: {text}");
    }
}

#[test]
fn a_hand_started_helper_serve_is_refused_before_it_serves() {
    let (succeeded, said) = atlas(&[
        "helper-serve",
        "--uid",
        &nix::unistd::getuid().as_raw().to_string(),
        "--controller-uid",
        &(nix::unistd::getuid().as_raw() + 1).to_string(),
        "--socket",
        "/tmp/atlas-helper-serve-test.sock",
    ]);
    assert!(!succeeded, "{said}");
    assert_eq!(said["code"], "denied", "{said}");
}

#[test]
fn a_hand_started_input_service_is_refused_before_it_serves() {
    let (succeeded, said) = atlas(&[
        "input-service",
        "--uid",
        &nix::unistd::getuid().as_raw().to_string(),
        "--controller-uid",
        &(nix::unistd::getuid().as_raw() + 1).to_string(),
        "--socket",
        "/tmp/atlas-input-service-test.sock",
    ]);
    assert!(!succeeded, "{said}");
    assert_eq!(said["code"], "denied", "{said}");
    assert!(!std::path::Path::new("/tmp/atlas-input-service-test.sock").exists());
}

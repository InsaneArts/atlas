//! Unprivileged subprocess stand-ins for interrupted mutation transactions.
use super::{UID, rerun};
use crate::control::Controller;
use std::{
    fs,
    path::Path,
    process::{Command, Stdio},
    time::Duration,
};

/// Dispatch through the production executor handshake, then pause after a
/// temporary filesystem mutation, until told to go on or the test's folder
/// is gone. No privileged or desktop tools are used.
pub(crate) fn pause_executor(directory: &Path) {
    let script = [
        crate::onboarding::as_root::EXECUTOR,
        r#"
print -r -- $$ > "$4"
/bin/rm -f "$5"
/usr/bin/touch "$3" "$2"
while [[ ! -e $1 && -d ${1:h} ]]; do /bin/sleep 0.01; done
"#,
    ]
    .concat();
    let _ = crate::onboarding::as_root::tracked_output(
        Command::new("/bin/zsh")
            .args(["-f", "-c", &script, "zsh"])
            .args([
                directory.join("continue"),
                directory.join("paused"),
                directory.join("changed"),
                directory.join("executor"),
                directory.join("opener"),
            ]),
    );
}

/// Kill the actual transaction's caller while its dispatched executor is
/// paused. Collect evidence first and release the stand-in before assertions,
/// including when a deliberately removed fix permits competing control.
pub(crate) fn interrupted_executor(test: &str, mode: &str) -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("opener"), b"temporary opener").unwrap();
    let mut caller = rerun(test)
        .arg("--nocapture")
        .env("ATLAS_MUTATION_TEST_DIRECTORY", directory.path())
        .env("ATLAS_MUTATION_TEST_MODE", mode)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    for _ in 0..500 {
        if directory.path().join("paused").exists() || caller.try_wait().unwrap().is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let controller = Controller::at(UID, &directory.path().join("state"));
    let paused = directory.path().join("paused").exists();
    let held_during_dispatch = controller.acquire(true).is_err();
    let claimed_during_dispatch = controller.pending().unwrap().len() == 1;
    let _ = caller.kill();
    let _ = caller.wait();
    let before = controller.pending().unwrap();
    let recovery_blocked = controller.acquire(true).is_err();
    controller.clear().unwrap();
    let after = controller.pending().unwrap();
    let normal_blocked = controller.acquire(false).is_err();
    fs::write(directory.path().join("continue"), b"").unwrap();
    let pid = fs::read_to_string(directory.path().join("executor"))
        .ok()
        .and_then(|pid| pid.trim().parse::<i32>().ok());
    let mut exited = false;
    for _ in 0..500 {
        if pid.is_none_or(|pid| {
            nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid), None)
                == Err(nix::errno::Errno::ESRCH)
        }) {
            exited = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        paused && exited,
        "the stand-in pauses and then exits: {mode}"
    );
    assert!(
        held_during_dispatch,
        "setup and teardown exclude each other: {mode}"
    );
    assert!(
        claimed_during_dispatch,
        "the mutation is claimed before dispatch: {mode}"
    );
    assert!(
        recovery_blocked && normal_blocked,
        "the surviving executor excludes recovery: {mode}"
    );
    assert_eq!(
        after, before,
        "recovery preserves the executor's claim: {mode}"
    );
    assert!(directory.path().join("changed").exists());
    let held = controller.acquire(true).unwrap();
    controller.clear().unwrap();
    drop(held);
    assert!(controller.acquire(false).is_ok());
    directory
}

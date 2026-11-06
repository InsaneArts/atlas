use super::*;

#[test]
fn this_process_reads_its_own_footprint_and_peak() {
    let usage = own().unwrap();
    assert!(usage.footprint > 0);
    assert!(usage.resident > 0);
    assert!(usage.peak_footprint > 0);
    let value = serde_json::to_value(usage).unwrap();
    assert!(value["peakFootprint"].is_u64());
}

#[test]
fn this_process_is_described_as_its_own_executable_and_account() {
    let pid = nix::unistd::getpid().as_raw();
    let process = describe(pid).unwrap();
    assert_eq!(Some(process.instance), Instance::current());
    assert_eq!(process.uid, nix::unistd::geteuid().as_raw());
    assert_eq!(process.real_uid, nix::unistd::getuid().as_raw());
    assert_eq!(
        process.path.canonicalize().unwrap(),
        std::env::current_exe().unwrap().canonicalize().unwrap()
    );
}

#[test]
fn a_running_executable_of_this_account_is_found_with_its_memory() {
    let path = executable_of(std::process::id()).unwrap();
    let (process, usage) = running(&path).unwrap();
    assert_eq!(process.path, path);
    assert_eq!(process.real_uid, nix::unistd::getuid().as_raw());
    assert!(usage.footprint > 0);
    assert!(usage.peak_footprint > 0);
}

#[test]
fn a_macos_service_of_this_account_is_found_by_its_executable_when_it_runs() {
    // macOS starts the screen recorder in a login once something there
    // captures the screen; a login without one has nothing to read.
    let uid = nix::unistd::getuid().as_raw();
    let running_now = process::of_user(uid)
        .unwrap()
        .iter()
        .any(|process| process.name == "replayd" && process.ruid == uid && !process.zombie);
    let found = running(Path::new("/usr/libexec/replayd"));
    assert_eq!(found.is_some(), running_now, "{found:?}");
    if let Some((process, usage)) = found {
        assert_eq!(process.uid, uid);
        assert!(usage.footprint > 0);
    }
}

#[test]
fn an_executable_this_account_does_not_run_is_not_found() {
    assert_eq!(
        running(Path::new("/usr/libexec/atlas-no-such-process")),
        None
    );
}

#[test]
fn another_accounts_process_reads_as_nothing() {
    if nix::unistd::getuid().is_root() {
        return;
    }
    // launchd belongs to root, whose accounting the kernel does not share.
    assert_eq!(of(1), None);
}

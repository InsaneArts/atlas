use super::*;

#[test]
fn this_process_reads_its_own_footprint_and_peak() {
    let usage = own().unwrap();
    assert!(usage.footprint > 0);
    assert!(usage.resident > 0);
    assert!(usage.peak_footprint >= usage.footprint);
    let value = serde_json::to_value(usage).unwrap();
    assert!(value["peakFootprint"].is_u64());
}

#[test]
fn a_macos_service_of_this_account_is_read_when_it_runs() {
    // macOS starts the screen recorder in a login once something there
    // captures the screen; a login without one has nothing to read.
    let uid = nix::unistd::getuid().as_raw();
    let running = atlas_core::process::of_user(uid)
        .unwrap()
        .iter()
        .any(|process| process.name == "replayd" && process.ruid == uid && !process.zombie);
    let usage = named("replayd");
    assert_eq!(usage.is_some(), running, "{usage:?}");
    if let Some(usage) = usage {
        assert!(usage.footprint > 0);
        assert!(usage.peak_footprint >= usage.footprint);
    }
}

#[test]
fn a_process_this_account_does_not_run_reads_as_nothing() {
    assert_eq!(named("atlas-no-such-process"), None);
}

#[test]
fn another_accounts_process_reads_as_nothing() {
    if nix::unistd::getuid().is_root() {
        return;
    }
    // launchd belongs to root, whose accounting the kernel does not share.
    assert_eq!(of(1), None);
}

use super::*;

#[test]
fn hello_describes_this_process_in_its_session() {
    let reply = hello(&manager_name(), None, None);
    assert_eq!(reply["user"]["uid"], nix::unistd::getuid().as_raw());
    assert!(reply["user"]["name"].is_string());
    assert!(
        reply["user"]["home"]
            .as_str()
            .is_some_and(|home| home.starts_with('/'))
    );
    assert_eq!(reply["daemon"]["pid"], std::process::id());
    assert_eq!(reply["daemon"]["protocol"], 1);
    assert!(atlas_core::session::has_capability(
        &reply["daemon"],
        atlas_core::protocol::SESSION_PIN
    ));
    assert!(reply["permissions"]["screenCapture"].is_boolean());
    assert!(
        reply["display"]["scale"]
            .as_f64()
            .is_some_and(|scale| scale >= 1.0)
    );
    assert!(reply["daemon"]["resources"]["footprint"].is_u64());
    assert!(reply["daemon"]["resources"].get("screenRecorder").is_none());
}

#[test]
fn hello_reports_the_screen_recorders_memory_and_restarts_beside_its_own() {
    let recorder = recorder::Report {
        usage: Some(resources::Usage {
            footprint: 6 << 30,
            peak_footprint: 7 << 30,
            resident: 2 << 30,
        }),
        resets: 2,
        last_reset_at: Some(1_793_923_943_551),
        reset_failures: 0,
    };
    let reply = hello(&manager_name(), None, Some(recorder));
    let resources = &reply["daemon"]["resources"];
    assert!(resources["footprint"].is_u64());
    assert_eq!(
        resources["screenRecorder"],
        json!({"footprint": 6_u64 << 30, "peakFootprint": 7_u64 << 30, "resident": 2_u64 << 30,
            "resets": 2, "lastResetAt": 1_793_923_943_551_u64, "resetFailures": 0})
    );
}

#[test]
fn a_recorder_that_is_not_running_still_reports_its_restarts() {
    let reply = hello(
        &manager_name(),
        None,
        Some(recorder::Report {
            reset_failures: 1,
            ..recorder::Report::default()
        }),
    );
    assert_eq!(
        reply["daemon"]["resources"]["screenRecorder"],
        json!({"resets": 0, "resetFailures": 1})
    );
}

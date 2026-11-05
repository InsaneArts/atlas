use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

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
fn hello_reports_the_screen_recorders_memory_beside_its_own() {
    let recorder = resources::Usage {
        footprint: 6 << 30,
        peak_footprint: 7 << 30,
        resident: 2 << 30,
    };
    let reply = hello(&manager_name(), None, Some(recorder));
    let resources = &reply["daemon"]["resources"];
    assert!(resources["footprint"].is_u64());
    assert_eq!(
        resources["screenRecorder"],
        json!({"footprint": 6_u64 << 30, "peakFootprint": 7_u64 << 30, "resident": 2_u64 << 30})
    );
}

const READING: resources::Usage = resources::Usage {
    footprint: 1,
    peak_footprint: 2,
    resident: 1,
};

#[test]
fn the_screen_recorder_is_read_once_a_minute_however_often_status_is_asked() {
    static READS: AtomicUsize = AtomicUsize::new(0);
    let recorder = Recorder::new(|| {
        READS.fetch_add(1, Ordering::SeqCst);
        Some(READING)
    });
    let start = Instant::now();
    for seconds in [0, 1, 30, 59] {
        let at = start + Duration::from_secs(seconds);
        assert_eq!(recorder.usage_at(at), Some(READING));
    }
    assert_eq!(READS.load(Ordering::SeqCst), 1);
    assert_eq!(recorder.usage_at(start + RECORDER_READING), Some(READING));
    assert_eq!(READS.load(Ordering::SeqCst), 2);
}

#[test]
fn statuses_asked_together_share_one_reading() {
    static READS: AtomicUsize = AtomicUsize::new(0);
    let recorder = Recorder::new(|| {
        READS.fetch_add(1, Ordering::SeqCst);
        std::thread::sleep(Duration::from_millis(20));
        Some(READING)
    });
    let now = Instant::now();
    std::thread::scope(|scope| {
        for _ in 0..8 {
            scope.spawn(|| assert_eq!(recorder.usage_at(now), Some(READING)));
        }
    });
    assert_eq!(READS.load(Ordering::SeqCst), 1);
}

#[test]
fn a_refused_reading_reports_nothing_and_waits_the_minute_to_try_again() {
    static READS: AtomicUsize = AtomicUsize::new(0);
    let recorder = Recorder::new(|| {
        READS.fetch_add(1, Ordering::SeqCst);
        None
    });
    let start = Instant::now();
    assert_eq!(recorder.usage_at(start), None);
    assert_eq!(recorder.usage_at(start + Duration::from_secs(1)), None);
    assert_eq!(READS.load(Ordering::SeqCst), 1);
}

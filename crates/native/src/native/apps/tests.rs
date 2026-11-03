use super::*;

#[test]
fn this_session_lists_running_applications() {
    let apps = running();
    assert!(!apps.is_empty(), "a logged-in session always runs apps");

    for app in &apps {
        let app = json!(app);
        assert!(app["pid"].as_i64().is_some_and(|pid| pid > 0));
        assert!(
            ["regular", "accessory", "prohibited", "unknown"]
                .contains(&app["policy"].as_str().unwrap())
        );
        assert!(app["launchedAt"].is_null() || app["launchedAt"].is_u64());
    }
}

#[test]
fn a_bundle_id_lookup_uses_the_requested_process() {
    assert_eq!(
        bundle_id_with(812, |pid| {
            assert_eq!(pid, 812);
            Some("com.example.Probe".to_owned())
        })
        .as_deref(),
        Some("com.example.Probe")
    );
}

#[test]
fn a_bundle_id_lookup_can_find_no_app_or_bundle() {
    assert_eq!(
        bundle_id_with(812, |pid| {
            assert_eq!(pid, 812);
            None
        }),
        None
    );
}

#[test]
fn invalid_process_ids_never_reach_the_bundle_id_lookup() {
    for pid in [0, -1, i64::MIN, i64::MAX] {
        assert_eq!(bundle_id_with(pid, |_| panic!("looked up {pid}")), None);
    }
}

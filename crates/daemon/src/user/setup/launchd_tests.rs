use super::*;
use serde_json::json;

#[test]
fn launchctl_print_needs_a_definition_path() {
    let text = "gui/502/x = {\n\tpath = /Users/h/Library/LaunchAgents/x.plist\n\
                    \tstate = running\n\tpid = 42\n}";
    assert_eq!(
        parse_job(text).unwrap(),
        Job::Loaded {
            pid: Some(42),
            path: "/Users/h/Library/LaunchAgents/x.plist".into()
        }
    );
    assert!(parse_job("gui/502/x = {\n\tstate = waiting\n}").is_err());
    assert!(parse_job("\tpath = relative.plist\n").is_err());
}

/// Only a definition file with other bytes is replaced; none there (a
/// job from an older label) or the expected one is left alone.
#[test]
fn only_another_versions_definition_is_replaced() {
    let ours = "a".repeat(64);
    let earlier = "b".repeat(64);
    let job = |kind: &str, sha: Option<&str>| json!({"plistKind": kind, "plistSha256": sha});

    assert!(JobReport::from(&job("regular", Some(&earlier))).outdated(&ours));
    assert!(!JobReport::from(&job("regular", Some(&ours))).outdated(&ours));
    assert!(!JobReport::from(&job("absent", None)).outdated(&ours));
    assert!(
        !JobReport::from(&job("other", None)).outdated(&ours),
        "a link is never replaced"
    );
    assert!(!JobReport::from(&Value::Null).outdated(&ours));
}

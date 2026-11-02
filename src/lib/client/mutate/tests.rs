use super::*;

#[test]
fn signal_timeout_and_exit_are_distinct_completed_outcomes() {
    for reply in [
        json!({"ok":true,"exitCode":0,"timedOut":false,"signal":null}),
        json!({"ok":true,"exitCode":null,"timedOut":false,"signal":"SIGTERM"}),
        json!({"ok":true,"exitCode":null,"timedOut":true,"signal":"SIGKILL"}),
    ] {
        Mutation::Run.validate(&reply).unwrap();
    }

    for reply in [
        json!({"ok":true}),
        json!({"ok":true,"exitCode":null,"timedOut":false,"signal":15}),
    ] {
        assert!(Mutation::Run.validate(&reply).unwrap_err().uncertain);
    }

    assert!(
        Mutation::Input {
            count: 1,
            expect: None
        }
        .validate(&json!({"ok":true,"performed":0}))
        .unwrap_err()
        .uncertain
    );
}

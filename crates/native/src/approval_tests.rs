use super::*;

#[test]
fn execution_failure_after_approval_never_allows_another_approval() {
    for (status, has_pipe) in [(-60031, false), (-60031, true), (0, false)] {
        assert!(matches!(
            execution_started(status, has_pipe.then_some(())),
            Err(Refusal::Failed(_))
        ));
    }
    assert_eq!(execution_started(0, Some(())), Ok(()));
}

#[test]
fn this_macos_still_has_the_call_that_starts_the_approved_step() {
    assert!(execute().is_some());
}

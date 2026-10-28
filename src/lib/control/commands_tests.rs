//! Commands agents started with `run`: other agents act beside one while it
//! runs, recovery and everything strict wait for it, and one that died
//! blocks everyone as any unknown outcome does.
use super::*;

thread_local! {
    /// Set to the end of a record's name, the next replacement of that
    /// record on this thread stops before its rename, as a writer killed
    /// there would.
    pub(super) static INTERRUPT_REPLACE: std::cell::Cell<Option<&'static str>> = const { std::cell::Cell::new(None) };
}

/// Stop the next replacement of the record whose name ends with `name`
/// before its rename.
fn interrupt_replacing(name: &'static str) {
    INTERRUPT_REPLACE.with(|interrupt| interrupt.set(Some(name)));
}

/// Two controllers of one helper, as two commands would hold them.
fn pair(directory: &Path) -> (Controller, Controller) {
    (
        Controller::at(505, directory),
        Controller::at(505, directory),
    )
}

/// What the list beside the marker names.
fn marked(controller: &Controller) -> Vec<String> {
    commands::read(&controller.commands)
}

#[test]
fn an_agent_acts_beside_a_command_still_running() {
    let directory = tempfile::tempdir().unwrap();
    let (running, other) = pair(directory.path());
    let _command = running.quarantine_command().unwrap();

    let beside = other.acquire_beside_commands().unwrap();
    let change = other.quarantine().unwrap();
    other.clear_if(&change).unwrap();
    drop(change);
    drop(beside);

    assert_eq!(
        other.pending().unwrap().len(),
        1,
        "the command stays pending"
    );
}

#[test]
fn a_running_command_still_keeps_recovery_out() {
    let directory = tempfile::tempdir().unwrap();
    let (running, other) = pair(directory.path());
    let _command = running.quarantine_command().unwrap();

    let refused = other.acquire(true).unwrap_err();
    assert_eq!(refused.code, "busy");
    assert_eq!(refused.holder, Some(Holder::Agent));
    assert!(!refused.uncertain);
    assert!(refused.error.contains("still running"), "{}", refused.error);
    assert!(refused.error.contains("stop agents"), "{}", refused.error);
    assert_eq!(refused.details["running"], "command");
    assert!(other.clear().is_ok());
    assert_eq!(
        other.pending().unwrap().len(),
        1,
        "recovery never settles it"
    );
}

#[test]
fn an_update_waits_for_a_running_command() {
    let directory = tempfile::tempdir().unwrap();
    let (running, other) = pair(directory.path());
    let _command = running.quarantine_command().unwrap();

    for refused in [
        other.acquire(false).unwrap_err(),
        other.require_known().unwrap_err(),
    ] {
        assert_eq!(refused.code, "busy", "not uncertain: nothing is unknown");
        assert_eq!(refused.holder, Some(Holder::Agent));
        assert!(!refused.uncertain);
        assert!(!refused.error.contains("stop agents"), "{}", refused.error);
        assert_eq!(refused.details["running"], "command");
        assert_eq!(
            refused.details["next"],
            crate::ui::hints::COMMAND_RUNNING,
            "nobody is told to recover it"
        );
    }
}

#[test]
fn a_command_whose_process_died_blocks_every_agent() {
    if !crate::transaction_test::isolated(
        "control::commands_tests::a_command_whose_process_died_blocks_every_agent",
    ) {
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let (running, other) = pair(directory.path());
    drop(running.quarantine_command().unwrap());

    assert_eq!(
        other.acquire_beside_commands().unwrap_err().code,
        "uncertain"
    );
    assert_eq!(other.acquire(false).unwrap_err().code, "uncertain");
    assert_eq!(other.acquire_for_person().unwrap_err().code, "uncertain");
    drop(other.acquire(true).unwrap());
}

#[test]
fn an_unmarked_claim_still_blocks_agents_beside_it() {
    let directory = tempfile::tempdir().unwrap();
    let (running, other) = pair(directory.path());
    let _change = running.quarantine().unwrap();

    assert_eq!(
        other.acquire_beside_commands().unwrap_err().code,
        "uncertain"
    );
}

#[test]
fn an_orphaned_privileged_executor_is_never_taken_for_a_command() {
    if !crate::transaction_test::isolated(
        "control::commands_tests::an_orphaned_privileged_executor_is_never_taken_for_a_command",
    ) {
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let (running, other) = pair(directory.path());
    let command = running.quarantine_command().unwrap();
    let claims = ExecutorClaims::new(std::slice::from_ref(&command)).unwrap();
    // launchd is a stable, root-owned process. Observe it only: this test
    // never dispatches privileged work or changes a real helper's state.
    record_executor(nix::unistd::Pid::from_raw(1)).unwrap();
    let live = command.live.clone();
    drop(claims);
    drop(command);

    assert!(live.exists(), "the executor outlives its caller's claim");
    assert_eq!(
        other.acquire_beside_commands().unwrap_err().code,
        "uncertain"
    );
    let refused = other.acquire(true).unwrap_err();
    assert_eq!(refused.code, "busy");
    assert!(refused.error.contains("executor"), "{}", refused.error);
}

#[test]
fn an_unknown_outcome_beside_a_running_command_still_blocks_agents() {
    let directory = tempfile::tempdir().unwrap();
    let (running, other) = pair(directory.path());
    let _command = running.quarantine_command().unwrap();
    drop(running.quarantine().unwrap());

    assert_eq!(
        other.acquire_beside_commands().unwrap_err().code,
        "uncertain"
    );
    assert_eq!(other.acquire(false).unwrap_err().code, "uncertain");
}

#[test]
fn recovery_keeps_a_running_command_and_its_mark() {
    if !crate::transaction_test::isolated(
        "control::commands_tests::recovery_keeps_a_running_command_and_its_mark",
    ) {
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let (running, other) = pair(directory.path());
    drop(running.quarantine_command().unwrap());
    drop(running.quarantine().unwrap());
    let command = running.quarantine_command().unwrap();

    other.clear().unwrap();

    let kept = vec![command.generation().to_owned()];
    assert_eq!(other.pending().unwrap(), kept);
    assert_eq!(marked(&other), kept);
    assert!(other.acquire_beside_commands().is_ok());
}

#[test]
fn settling_a_command_removes_its_mark() {
    let directory = tempfile::tempdir().unwrap();
    let (running, other) = pair(directory.path());
    let command = running.quarantine_command().unwrap();
    assert_eq!(marked(&other), [command.generation().to_owned()]);

    running.clear_if(&command).unwrap();
    drop(command);

    assert!(!other.marker.exists());
    assert!(!other.commands.exists());
}

#[test]
fn a_malformed_command_list_fails_closed() {
    let directory = tempfile::tempdir().unwrap();
    let (running, other) = pair(directory.path());
    let _command = running.quarantine_command().unwrap();

    for garbage in [&b"not JSON"[..], b"{}", b"[1]"] {
        fs::write(&other.commands, garbage).unwrap();
        assert_eq!(
            other.acquire_beside_commands().unwrap_err().code,
            "uncertain",
            "{garbage:?}"
        );
    }
    fs::remove_file(&other.commands).unwrap();
    assert_eq!(
        other.acquire_beside_commands().unwrap_err().code,
        "uncertain",
        "a missing list marks nothing"
    );
}

#[test]
fn the_marker_stays_a_list_of_generations() {
    let directory = tempfile::tempdir().unwrap();
    let (running, other) = pair(directory.path());
    let command = running.quarantine_command().unwrap();

    let marker: Vec<String> = serde_json::from_slice(&fs::read(&other.marker).unwrap()).unwrap();
    assert_eq!(marker, [command.generation().to_owned()]);
    assert_eq!(other.pending().unwrap(), marker);
    assert!(
        other.in_progress(&command),
        "an older build sees it under way"
    );
}

#[test]
fn a_running_command_is_counted_and_shown_as_one() {
    let directory = tempfile::tempdir().unwrap();
    let (running, other) = pair(directory.path());
    let command = running.quarantine_command().unwrap();
    let change = running.quarantine().unwrap();

    let snapshot = other.pending_snapshot().unwrap();
    assert_eq!((snapshot.total, snapshot.commands), (2, 1));
    assert!(snapshot.unresolved.is_empty());
    let causes = other.pending_causes().unwrap();
    let of = |generation: &str| {
        causes
            .iter()
            .find(|cause| cause.generation == generation)
            .unwrap()
            .clone()
    };
    assert!(of(&command).running && of(&command).command);
    assert!(of(&change).running && !of(&change).command);
}

/// The marker as an older build reads it: a list of generations, or
/// `legacy-unknown` when it does not parse.
fn as_older_build_reads(controller: &Controller) -> Vec<String> {
    let text = fs::read_to_string(&controller.marker).unwrap();
    serde_json::from_str(&text).unwrap_or_else(|_| vec!["legacy-unknown".into()])
}

/// The replacements a writer killed partway left in `directory`.
fn left_behind(directory: &Path) -> usize {
    fs::read_dir(directory)
        .unwrap()
        .flatten()
        .filter(|entry| entry.file_name().to_string_lossy().ends_with(REPLACING))
        .count()
}

#[test]
fn a_change_interrupted_while_recorded_keeps_a_running_command_pending() {
    let directory = tempfile::tempdir().unwrap();
    let (running, other) = pair(directory.path());
    let command = running.quarantine_command().unwrap();
    let kept = vec![command.generation().to_owned()];

    interrupt_replacing("uncertain.json");
    assert!(other.quarantine().is_err());

    assert_eq!(as_older_build_reads(&other), kept);
    assert_eq!(other.pending().unwrap(), kept);
    assert!(
        other.in_progress(&command),
        "an older recovery waits for it"
    );
    assert_eq!(other.acquire(true).unwrap_err().code, "busy");
    assert!(other.acquire_beside_commands().is_ok());
    assert_eq!(left_behind(directory.path()), 1);

    other.clear().unwrap();
    assert_eq!(left_behind(directory.path()), 0, "recovery removes it");
    assert_eq!(other.pending().unwrap(), kept);
    assert_eq!(marked(&other), kept);
}

#[test]
fn a_settlement_interrupted_while_recorded_keeps_a_running_command_pending() {
    let directory = tempfile::tempdir().unwrap();
    let (running, other) = pair(directory.path());
    let command = running.quarantine_command().unwrap();
    let change = other.quarantine().unwrap();

    interrupt_replacing("uncertain.json");
    assert!(other.clear_if(&change).is_err());

    assert_eq!(
        as_older_build_reads(&other),
        [command.generation(), change.generation()]
    );
    assert_eq!(other.acquire(true).unwrap_err().code, "busy");
    assert!(
        other
            .pending_causes()
            .unwrap()
            .iter()
            .any(|cause| cause.generation == command.generation() && cause.command),
        "it still counts as a running command"
    );

    other.clear_if(&change).unwrap();
    drop(change);
    assert_eq!(
        as_older_build_reads(&other),
        [command.generation().to_owned()]
    );
}

#[test]
fn a_command_interrupted_while_marked_never_starts_and_keeps_the_others_marked() {
    let directory = tempfile::tempdir().unwrap();
    let (running, other) = pair(directory.path());
    let command = running.quarantine_command().unwrap();
    let kept = vec![command.generation().to_owned()];

    interrupt_replacing("commands.json");
    assert!(other.quarantine_command().is_err());

    assert_eq!(marked(&other), kept);
    assert_eq!(as_older_build_reads(&other), kept);
    assert!(other.acquire_beside_commands().is_ok());
}

#[test]
fn marking_over_a_corrupted_list_fails_closed() {
    let directory = tempfile::tempdir().unwrap();
    let (running, other) = pair(directory.path());
    let _first = running.quarantine_command().unwrap();
    fs::write(&other.commands, b"not JSON").unwrap();

    let _second = other.quarantine_command().unwrap();

    assert_eq!(
        other.acquire_beside_commands().unwrap_err().code,
        "uncertain",
        "the command whose mark was lost counts as unknown"
    );
}

#[test]
fn no_more_commands_start_once_the_most_that_may_run_do() {
    let directory = tempfile::tempdir().unwrap();
    let (running, other) = pair(directory.path());
    let _commands: Vec<Pending> = (0..RUNNING_COMMANDS_MAX)
        .map(|_| running.quarantine_command().unwrap())
        .collect();

    let refused = other.quarantine_command().unwrap_err();
    assert_eq!(refused.code, "busy");
    assert_eq!(refused.holder, Some(Holder::Agent));
    assert_eq!(refused.details["running"], "command");
    assert_eq!(other.pending().unwrap().len(), RUNNING_COMMANDS_MAX);
    let change = other.quarantine().unwrap();
    other.clear_if(&change).unwrap();
}

#[test]
fn an_abandoned_command_is_an_unknown_outcome_and_never_settled() {
    let directory = tempfile::tempdir().unwrap();
    let (running, other) = pair(directory.path());
    let command = running.quarantine_command().unwrap();
    let generation = command.generation().to_owned();

    running.abandon_command(command);

    assert_eq!(other.pending().unwrap(), [generation]);
    assert!(marked(&other).is_empty());
    assert_eq!(
        other.acquire_beside_commands().unwrap_err().code,
        "uncertain"
    );
}

use super::*;
use crate::{
    test_support::{ScriptedKeeper, vm_through_keeper},
    vm::{
        lifecycle::{
            Lifecycle,
            record::{Phase, Record},
        },
        runner::Operation,
    },
};
use atlas_core::files::Token;
use std::path::Path;

fn tempdir() -> tempfile::TempDir {
    tempfile::Builder::new().tempdir_in("/tmp").unwrap()
}

/// A VM client, its keeper and Atlas VM stood in for in `directory`, the
/// VM's power `power`.
fn vm(directory: &Path, power: &str) -> (Client, ScriptedKeeper) {
    let (client, keeper) = vm_through_keeper(directory);
    keeper.runner.set_power(power);
    (client, keeper)
}

fn record(client: &Client, directory: &Path) -> Record {
    Lifecycle::under(directory)
        .record(&client.desktop().id())
        .unwrap()
        .unwrap()
}

/// A start an earlier command kept in the VM's record and claimed as
/// Atlas VM's, then went without settling.
fn start_left(client: &Client, directory: &Path) -> Token {
    let change = Token::new();
    let mut kept = record(client, directory);
    kept.starts.push(change.clone());
    kept.sending(&Operation::Start(change.clone())).unwrap();
    Lifecycle::under(directory).keep_record(&kept).unwrap();
    claimed_by(client, &change, Executor::Vm);
    change
}

/// A change claimed as `executor`'s by a command that went without
/// settling it.
fn claimed_by(client: &Client, change: &Token, executor: Executor) {
    drop(
        client
            .controller
            .quarantine_by(change.as_str(), executor)
            .unwrap(),
    );
}

/// Each change a recovery's list names, with who carries it out, in order.
fn named(list: &Value) -> Vec<(String, String)> {
    let mut named: Vec<(String, String)> = list
        .as_array()
        .unwrap()
        .iter()
        .map(|change| {
            (
                change["change"].as_str().unwrap().to_owned(),
                change["by"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    named.sort();
    named
}

fn one(change: &Token, by: &str) -> Vec<(String, String)> {
    vec![(change.to_string(), by.to_owned())]
}

fn done() -> Option<Value> {
    Some(json!({"state": "done"}))
}

/// What the stand-in Atlas VM was asked to change.
fn changes_asked(keeper: &ScriptedKeeper) -> Vec<String> {
    keeper
        .runner
        .ops()
        .into_iter()
        .filter(|op| ["start", "stop", "window", "void"].contains(&op.as_str()))
        .collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_vms_recover_leaves_an_unanswered_atlas_vm_start_pending_and_names_who_carries_it_out() {
    let dir = tempdir();
    let (client, keeper) = vm(dir.path(), "starting");
    let making = start_left(&client, dir.path());
    keeper.runner.keep(making.as_str(), "start", None);
    let unheard = start_left(&client, dir.path());

    let recovered = client.recover_helper().await.unwrap();

    assert_eq!(recovered["ok"], true, "{recovered}");
    assert_eq!(recovered["op"], "recover");
    assert_eq!(recovered["desktop"], json!({"kind": "vm", "name": "work"}));
    assert_eq!(recovered["cleared"], json!([]), "{recovered}");
    let mut kept = [one(&making, "vm"), one(&unheard, "vm")].concat();
    kept.sort();
    assert_eq!(named(&recovered["kept"]), kept);
    assert_eq!(recovered["guest"], json!({"recovered": true}));
    assert!(changes_asked(&keeper).is_empty(), "nothing asked again");
    let starts = record(&client, dir.path()).starts;
    assert!(starts.contains(&making) && starts.contains(&unheard));
    assert_eq!(client.controller.pending().unwrap().len(), 2);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_vms_recover_clears_a_start_atlas_vm_answered_and_never_asks_a_new_one() {
    let dir = tempdir();
    let (client, keeper) = vm(dir.path(), "stopped");
    let started = start_left(&client, dir.path());
    keeper.runner.keep(started.as_str(), "start", done());
    let refused = start_left(&client, dir.path());
    keeper.runner.keep(
        refused.as_str(),
        "start",
        Some(json!({"state": "refused", "reason": "Its disk is in use."})),
    );

    let recovered = client.recover_helper().await.unwrap();

    let mut cleared = [one(&started, "vm"), one(&refused, "vm")].concat();
    cleared.sort();
    assert_eq!(named(&recovered["cleared"]), cleared, "{recovered}");
    assert_eq!(recovered["kept"], json!([]));
    assert!(changes_asked(&keeper).is_empty(), "no new start");
    assert_eq!(keeper.runner.power(), "stopped");
    let kept = record(&client, dir.path());
    assert!(kept.starts.is_empty(), "{kept:?}");
    assert!(kept.sent(&started).is_none() && kept.sent(&refused).is_none());
    let minute = kept.refused_start.expect("the refusal's minute begun");
    assert_eq!(minute.change, refused);
    assert_eq!(minute.reason, "Its disk is in use.");
    assert!(client.controller.pending().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_vms_recover_with_the_vm_stopped_recovers_atlas_vms_changes_and_keeps_the_guests_without_starting_it()
 {
    let dir = tempdir();
    let (client, keeper) = vm(dir.path(), "stopped");
    *keeper.away.lock().unwrap() = true;
    let started = start_left(&client, dir.path());
    keeper.runner.keep(started.as_str(), "start", done());
    let guests = Token::new();
    claimed_by(&client, &guests, Executor::Guest);

    let recovered = client.recover_helper().await.unwrap();

    assert_eq!(
        named(&recovered["cleared"]),
        one(&started, "vm"),
        "{recovered}"
    );
    assert_eq!(named(&recovered["kept"]), one(&guests, "guest"));
    assert_eq!(
        recovered["guest"],
        json!({"recovered": false,
            "why": "work is not running, so its guest's changes wait until it runs."})
    );
    assert!(changes_asked(&keeper).is_empty(), "never started");
    assert_eq!(keeper.runner.power(), "stopped");
    assert!(
        keeper.asked_for("recover").is_empty(),
        "the guest asked nothing"
    );
    assert_eq!(client.controller.pending().unwrap(), [guests.as_str()]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_vms_recover_keeps_a_station_claim_until_the_station_answers_it() {
    let dir = tempdir();
    let (client, keeper) = vm(dir.path(), "running");
    let stations = Token::new();
    claimed_by(&client, &stations, Executor::Station);

    let recovered = client.recover_helper().await.unwrap();

    assert_eq!(recovered["cleared"], json!([]), "{recovered}");
    assert_eq!(named(&recovered["kept"]), one(&stations, "station"));
    assert_eq!(
        keeper.asked_for("recover").len(),
        1,
        "the guest's own asked"
    );
    let asked: Vec<Value> = keeper
        .runner
        .asked_for("outcome")
        .into_iter()
        .map(|asked| asked["change"].clone())
        .collect();
    assert!(!asked.contains(&json!(stations)), "{asked:?}");
    assert_eq!(client.controller.pending().unwrap(), [stations.as_str()]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_vms_recover_holds_the_desktops_exclusion_until_it_answers() {
    let dir = tempdir();
    let (client, keeper) = vm(dir.path(), "running");
    let started = start_left(&client, dir.path());
    keeper.runner.keep(started.as_str(), "start", done());
    keeper.runner.slow_outcomes(Duration::from_millis(800));
    let other = client.controller.clone().waiting(Duration::ZERO);

    let recovering = tokio::spawn({
        let client = client.clone();
        async move { client.recover_helper().await }
    });
    while keeper.runner.asked_for("outcome").is_empty() {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let meanwhile = other.acquire_for_executor().map(drop);
    let recovered = recovering.await.unwrap().unwrap();

    assert_eq!(meanwhile.unwrap_err().code, Code::Busy);
    assert_eq!(named(&recovered["cleared"]), one(&started, "vm"));
    assert!(other.acquire_for_executor().is_ok(), "let go once answered");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_maintenances_own_window_stop_and_start_claims_are_settled_from_atlas_vms_outcomes_and_never_sent_again()
 {
    let dir = tempdir();
    let (client, keeper) = vm(dir.path(), "running");
    let (window, stop, start, opening) = (Token::new(), Token::new(), Token::new(), Token::new());
    for (change, op, outcome) in [
        (&window, "window-off", done()),
        (&stop, "stop", done()),
        (
            &start,
            "start",
            Some(json!({"state": "refused", "reason": "Its disk is in use."})),
        ),
        (&opening, "window-on", None),
    ] {
        claimed_by(&client, change, Executor::Vm);
        keeper.runner.keep(change.as_str(), op, outcome);
    }
    // Its window-on was the run before's, which cannot be told gone yet.
    keeper
        .runner
        .runs_again_as("0b0e1a5e-7a1d-4e3c-9a55-1a7a5f0e0002");

    let recovered = client.recover_helper().await.unwrap();

    let mut cleared = [one(&window, "vm"), one(&stop, "vm"), one(&start, "vm")].concat();
    cleared.sort();
    assert_eq!(named(&recovered["cleared"]), cleared, "{recovered}");
    assert_eq!(named(&recovered["kept"]), one(&opening, "vm"));
    assert!(changes_asked(&keeper).is_empty(), "nothing sent again");
    assert!(
        record(&client, dir.path()).refused_start.is_none(),
        "a maintenance's start is no start a command needed"
    );
    assert!(!keeper.runner.window_open());
    assert_eq!(client.controller.pending().unwrap(), [opening.as_str()]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn recover_during_maintenance_is_refused_in_its_words() {
    for (phase, words) in [
        (
            Phase::Open,
            "work is under maintenance: its window takes the person's input. Agents wait \
             until the person ends it.",
        ),
        (
            Phase::Closing,
            "work's maintenance is ending, but Atlas VM has not confirmed its window takes no \
             input, so agents still wait.",
        ),
    ] {
        let dir = tempdir();
        let (client, keeper) = vm(dir.path(), "running");
        let started = start_left(&client, dir.path());
        keeper.runner.keep(started.as_str(), "start", done());
        crate::test_support::under_maintenance(dir.path(), &client.desktop().id(), phase);

        let refused = client.recover_helper().await.unwrap_err();

        assert_eq!(refused.code, Code::Busy, "{phase:?}: {refused:?}");
        assert_eq!(refused.error, words, "{phase:?}");
        assert!(keeper.runner.asked_for("outcome").is_empty(), "{phase:?}");
        assert!(keeper.asked_for("observe").is_empty(), "{phase:?}");
        assert!(keeper.asked_for("recover").is_empty(), "{phase:?}");
        assert_eq!(client.controller.pending().unwrap(), [started.as_str()]);
        let other = client.controller.clone().waiting(Duration::ZERO);
        assert!(other.acquire_for_executor().is_ok(), "no exclusion kept");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_vms_recover_beside_a_command_still_running_there_is_refused_in_the_vms_words() {
    let dir = tempdir();
    let (client, keeper) = vm(dir.path(), "running");
    let running = client
        .controller
        .quarantine_command_by(Token::new().as_str(), Executor::Guest)
        .unwrap();

    let refused = client.recover_helper().await.unwrap_err();

    assert_eq!(refused.code, Code::Busy, "{refused:?}");
    assert_eq!(
        refused.error,
        "A command an agent started is still running on work."
    );
    assert_eq!(
        refused.details["next"],
        "Wait for the command running on work to finish, then recover."
    );
    assert!(!format!("{refused:?}").contains("helper"), "{refused:?}");
    assert!(keeper.asked_for("recover").is_empty());
    drop(running);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_vms_recover_cut_short_after_it_cleared_a_claim_says_that_claim_cleared_the_rest_kept_and_why()
 {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempdir();
    let (client, keeper) = vm(dir.path(), "stopped");
    let first = Token::new();
    claimed_by(&client, &first, Executor::Vm);
    keeper.runner.keep(first.as_str(), "stop", done());
    let refused = start_left(&client, dir.path());
    keeper.runner.keep(
        refused.as_str(),
        "start",
        Some(json!({"state": "refused", "reason": "Its disk is in use."})),
    );
    drop(Lifecycle::under(dir.path()).admit().unwrap());
    let folder = crate::test_support::lifecycle_record(dir.path(), &client.desktop().id())
        .parent()
        .unwrap()
        .to_owned();
    // The refusal's minute cannot be kept: the record's folder takes no
    // write.
    std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o500)).unwrap();

    let recovered = client.recover_helper().await;
    std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o700)).unwrap();
    let recovered = recovered.unwrap();

    assert_eq!(
        named(&recovered["cleared"]),
        one(&first, "vm"),
        "{recovered}"
    );
    assert_eq!(named(&recovered["kept"]), one(&refused, "vm"));
    let why = recovered["station"]["why"].as_str().unwrap();
    assert!(
        why.starts_with("Not every change Atlas VM answered on work was settled: "),
        "{why}"
    );
    assert!(!why.contains("helper"), "{why}");
    assert!(changes_asked(&keeper).is_empty(), "nothing asked again");
    assert_eq!(record(&client, dir.path()).starts, vec![refused.clone()]);
    assert_eq!(client.controller.pending().unwrap(), [refused.as_str()]);
}

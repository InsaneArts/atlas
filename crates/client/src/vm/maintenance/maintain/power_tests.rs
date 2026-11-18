use super::super::tests::{
    BEGIN, BOUNDARIES, Boundary, END, Person, asked, asked_within, lifecycle, maintenance, pending,
    quiet_vm, record, tempdir, until,
};
use super::*;
use crate::vm::lifecycle::grant;
use crate::{
    client::Client,
    test_support::{FIRST_RUN, ScriptedKeeper, under_maintenance},
    vm::{
        maintenance::Maintain,
        runner::{Runner, socket_in},
    },
};
use atlas_control::executors::Executor;
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

const RESTART: Maintain = Maintain {
    end: false,
    restart: true,
};

/// Where the first `of` in `ops`, from `from` on, is.
fn first(ops: &[String], of: &str, from: usize) -> usize {
    ops.iter()
        .skip(from)
        .position(|op| op == of)
        .map(|at| at + from)
        .unwrap_or_else(|| panic!("no {of} after {from} in {ops:?}"))
}

/// Leave `client`'s VM's maintenance, a restart, at `phase` as a begin cut
/// short at `boundary` in its stop, or with the VM stopped in its start,
/// left it: that change.
fn cut_short_in_power(
    client: &Client,
    keeper: &ScriptedKeeper,
    boundary: Boundary,
    phase: Phase,
) -> Token {
    let lifecycle = lifecycle(client);
    let id = client.desktop().id();
    under_maintenance(lifecycle.home(), &id, phase);
    let mut record = record(client);
    let change = Token::new();
    let maintenance = record.maintenance.as_mut().unwrap();
    maintenance.restart = true;
    let operation = if phase == Phase::Stopping {
        maintenance.stop = Some(change.clone());
        Operation::Stop(change.clone())
    } else {
        keeper.runner.set_power("stopped");
        maintenance.start = Some(change.clone());
        Operation::Start(change.clone())
    };
    lifecycle.keep_record(&record).unwrap();
    if matches!(boundary, Boundary::Kept) {
        return change;
    }
    drop(
        client
            .controller
            .quarantine_by(change.as_str(), Executor::Vm)
            .unwrap(),
    );
    match boundary {
        Boundary::Kept | Boundary::Claimed => {}
        Boundary::Sent => {
            record.sending(&operation).unwrap();
            lifecycle.keep_record(&record).unwrap();
        }
        Boundary::Answered => {
            let runner = Runner::at(socket_in(lifecycle.home(), &id));
            let reply = runner
                .change(
                    &mut record,
                    |record| lifecycle.keep_record(record),
                    &operation,
                )
                .unwrap();
            assert_eq!(reply.outcome, Some(Outcome::Done), "{reply:?}");
        }
    }
    change
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_vm_whose_guest_cannot_answer_is_stopped_confirmed_stopped_and_started_again_before_its_window_opens()
 {
    let dir = tempdir();
    let (client, keeper) = quiet_vm(dir.path());
    *keeper.leftovers.lock().unwrap() = "unknown";
    // A change of the guest's, its outcome not known: never the
    // maintenance's to clear.
    let guests = Token::new();
    drop(
        client
            .controller
            .quarantine_by(guests.as_str(), Executor::Daemon)
            .unwrap(),
    );
    let person = Person::default();

    let begun = asked(&client, RESTART, person.allowing()).await.unwrap();

    assert_eq!(begun["maintenance"], "on", "{begun}");
    assert_eq!(
        begun["pending"], "A change on work is still pending; Atlas settles it once work answers.",
        "{begun}"
    );
    let ops = keeper.runner.ops();
    let stop = first(&ops, "stop", 0);
    let stopped = first(&ops, "state", stop);
    let start = first(&ops, "start", stopped);
    let window = first(&ops, "window", start);
    assert!(
        stop < stopped && stopped < start && start < window,
        "{ops:?}"
    );
    for op in ["stop", "start", "window"] {
        assert_eq!(keeper.runner.asked_for(op).len(), 1, "{op} asked once");
    }
    let kept = maintenance(&client).unwrap();
    assert_eq!(kept.phase, Phase::Open);
    assert!(kept.restart);
    let (stop, start) = (kept.stop.clone().unwrap(), kept.start.clone().unwrap());
    assert_eq!(
        keeper.runner.begun(),
        vec![
            stop.to_string(),
            start.to_string(),
            kept.open.clone().unwrap().to_string()
        ]
    );
    assert_eq!(kept.opened_by.as_deref(), Some(FIRST_RUN));
    assert!(
        record(&client).starts.is_empty(),
        "never a start a command needed"
    );
    assert_eq!(
        pending(&client),
        vec![guests.to_string()],
        "the guest's kept"
    );
    assert!(keeper.runner.window_open());
    let grants = person.asked();
    assert_eq!(grants.len(), 1);
    assert!(grants[0].restart, "the person is told work there ends");
    assert!(
        keeper.asked_for("read").is_empty(),
        "the guest is not asked what runs"
    );

    let ended = asked(&client, END, person.allowing()).await.unwrap();

    assert_eq!(ended["maintenance"], "none", "{ended}");
    assert_eq!(
        pending(&client),
        vec![guests.to_string()],
        "still the guest's"
    );
    assert!(
        record(&client).operations.is_empty(),
        "its stop and start forgotten with it"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_stopped_vm_is_started_for_maintenance_without_restart_as_it_runs_nothing() {
    let dir = tempdir();
    let (client, keeper) = quiet_vm(dir.path());
    *keeper.leftovers.lock().unwrap() = "unknown";
    keeper.runner.set_power("stopped");
    let person = Person::default();

    let begun = asked(&client, BEGIN, person.allowing()).await.unwrap();

    assert_eq!(begun["maintenance"], "on", "{begun}");
    assert!(begun.get("pending").is_none(), "{begun}");
    assert!(keeper.runner.asked_for("stop").is_empty(), "never stopped");
    assert_eq!(keeper.runner.asked_for("start").len(), 1);
    let ops = keeper.runner.ops();
    assert!(
        first(&ops, "start", 0) < first(&ops, "window", 0),
        "{ops:?}"
    );
    let kept = maintenance(&client).unwrap();
    assert_eq!(kept.phase, Phase::Open);
    assert!(!kept.restart);
    assert!(kept.stop.is_none());
    assert_eq!(
        keeper.runner.asked_for("start")[0]["change"],
        json!(kept.start.clone().unwrap())
    );
    assert!(record(&client).starts.is_empty());
    assert!(pending(&client).is_empty());
    assert!(keeper.runner.window_open());
    assert!(!person.asked()[0].restart);
    assert!(
        keeper.asked_for("read").is_empty(),
        "a stopped VM's guest is not asked"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn no_start_is_asked_while_the_vm_is_not_confirmed_stopped() {
    let dir = tempdir();
    let (client, keeper) = quiet_vm(dir.path());
    keeper.runner.stalling_stop();
    let person = Person::default();

    let stopping = asked_within(
        &client,
        RESTART,
        Duration::from_millis(300),
        person.allowing(),
    )
    .await
    .unwrap_err();
    let kept = maintenance(&client).unwrap();
    let claimed = pending(&client);
    keeper.runner.set_power("stopped");
    keeper.runner.blind(true);
    let unconfirmed = asked_within(
        &client,
        RESTART,
        Duration::from_millis(300),
        person.allowing(),
    )
    .await
    .unwrap_err();
    let after = maintenance(&client).unwrap();
    let starts_before = keeper.runner.asked_for("start").len();
    let windows_before = keeper.runner.asked_for("window").len();
    keeper.runner.blind(false);
    let begun = asked(&client, BEGIN, person.allowing()).await.unwrap();

    assert_eq!(stopping.code, Code::NotReady, "{stopping:?}");
    assert_eq!(
        stopping.error,
        "Atlas VM has not said whether it stopped work; agents wait. Run atlas desktop \
         maintain work again."
    );
    assert_eq!(kept.phase, Phase::Stopping);
    let stop = kept.stop.unwrap();
    assert_eq!(claimed, vec![stop.to_string()], "claimed until answered");
    assert_eq!(unconfirmed.code, Code::NotReady, "{unconfirmed:?}");
    assert_eq!(
        unconfirmed.error,
        "work is not confirmed stopped yet, so it is not started for maintenance. Run atlas \
         desktop maintain work again."
    );
    assert_eq!(after.phase, Phase::Stopping);
    assert_eq!(starts_before, 0, "no start while not confirmed stopped");
    assert_eq!(windows_before, 0);
    assert_eq!(begun["maintenance"], "on", "{begun}");
    assert_eq!(keeper.runner.begun()[0], stop.to_string(), "stopped once");
    assert_eq!(keeper.runner.begun().len(), 3);
    assert!(
        keeper
            .runner
            .asked_for("stop")
            .iter()
            .all(|asked| asked["change"] == json!(stop)),
        "never under another change"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_restart_cut_short_goes_on_from_its_kept_phase_and_asks_no_change_twice() {
    for phase in [Phase::Stopping, Phase::Starting] {
        for boundary in BOUNDARIES {
            let dir = tempdir();
            let (client, keeper) = quiet_vm(dir.path());
            let change = cut_short_in_power(&client, &keeper, boundary, phase);
            let person = Person::default();

            let begun = asked(&client, BEGIN, person.allowing()).await;

            let at = format!("{phase:?} {boundary:?}");
            let begun = begun.unwrap_or_else(|error| panic!("{at}: {error:?}"));
            assert_eq!(begun["maintenance"], "on", "{at}: {begun}");
            assert!(begun.get("pending").is_none(), "{at}: nothing left pending");
            assert_eq!(maintenance(&client).unwrap().phase, Phase::Open, "{at}");
            let done = keeper.runner.begun();
            assert_eq!(done[0], change.to_string(), "{at}: under the same change");
            let changes = if phase == Phase::Stopping { 3 } else { 2 };
            assert_eq!(done.len(), changes, "{at}: each done once: {done:?}");
            let op = if phase == Phase::Stopping {
                "stop"
            } else {
                "start"
            };
            assert!(
                keeper
                    .runner
                    .asked_for(op)
                    .iter()
                    .all(|asked| asked["change"] == json!(change)),
                "{at}: never under another change"
            );
            assert!(person.asked()[0].restart, "{at}: the restart kept");
            assert!(pending(&client).is_empty(), "{at}");
            assert!(keeper.runner.window_open(), "{at}");
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_start_atlas_vm_refuses_forgets_the_maintenance_and_lets_agents_in_again() {
    let dir = tempdir();
    let (client, keeper) = quiet_vm(dir.path());
    keeper.runner.set_power("stopped");
    keeper.runner.refusing(Some("The VM's disk is busy."));
    let person = Person::default();

    let refused = asked(&client, BEGIN, person.allowing()).await.unwrap_err();

    assert_eq!(refused.code, Code::State, "{refused:?}");
    assert_eq!(
        refused.error,
        "Atlas VM did not start work: The VM's disk is busy."
    );
    assert!(maintenance(&client).is_none(), "forgotten");
    assert!(record(&client).operations.is_empty(), "its start with it");
    assert!(pending(&client).is_empty());
    assert_eq!(keeper.asked_for("release").len(), 1);
    assert!(keeper.runner.asked_for("window").is_empty());
    assert!(client.maintenance_held().is_none());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_end_of_a_maintenance_cut_short_while_starting_voids_its_start_and_turns_input_off() {
    let dir = tempdir();
    let (client, keeper) = quiet_vm(dir.path());
    // Sent, and never read by Atlas VM.
    let start = cut_short_in_power(&client, &keeper, Boundary::Sent, Phase::Starting);
    let person = Person::default();

    let ended = asked(&client, END, person.allowing()).await.unwrap();

    assert_eq!(ended["maintenance"], "none", "{ended}");
    let voided = keeper.runner.asked_for("void");
    assert_eq!(voided.len(), 1);
    assert_eq!(voided[0]["change"], json!(start));
    assert!(keeper.runner.asked_for("start").is_empty(), "never started");
    let windows = keeper.runner.asked_for("window");
    assert_eq!(windows.len(), 1);
    assert_eq!(windows[0]["input"], false);
    assert!(pending(&client).is_empty(), "its claim cleared once voided");
    assert!(record(&client).operations.is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_start_not_yet_taken_is_sent_only_once_the_run_answering_now_confirms_the_vm_stopped() {
    for boundary in [Boundary::Kept, Boundary::Claimed, Boundary::Sent] {
        let dir = tempdir();
        let (client, keeper) = quiet_vm(dir.path());
        // Kept at starting, then the station went; Atlas VM ran again, and
        // its new run cannot yet confirm the VM's power.
        let start = cut_short_in_power(&client, &keeper, boundary, Phase::Starting);
        keeper
            .runner
            .runs_again_as("0b0e1a5e-7a1d-4e3c-9a55-1a7a5f0e0002");
        keeper.runner.blind(true);
        let person = Person::default();

        let unconfirmed = asked_within(
            &client,
            BEGIN,
            Duration::from_millis(300),
            person.allowing(),
        )
        .await
        .unwrap_err();
        let kept = maintenance(&client).unwrap();
        let starts = keeper.runner.asked_for("start").len();
        keeper.runner.blind(false);
        let begun = asked(&client, BEGIN, person.allowing()).await;

        let at = format!("{boundary:?}");
        assert_eq!(unconfirmed.code, Code::NotReady, "{at}: {unconfirmed:?}");
        assert!(
            unconfirmed
                .error
                .starts_with("work is not confirmed stopped yet"),
            "{at}: {unconfirmed:?}"
        );
        assert_eq!(starts, 0, "{at}: no start sent");
        assert_eq!(kept.phase, Phase::Starting, "{at}: kept");
        assert_eq!(kept.start, Some(start.clone()), "{at}");
        let begun = begun.unwrap_or_else(|error| panic!("{at}: {error:?}"));
        assert_eq!(begun["maintenance"], "on", "{at}: {begun}");
        assert_eq!(
            keeper.runner.begun()[0],
            start.to_string(),
            "{at}: under its own change"
        );
        assert_eq!(keeper.runner.asked_for("start").len(), 1, "{at}");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_begin_whose_maintenance_was_replaced_while_the_person_was_asked_is_refused_and_asks_nothing()
 {
    let dir = tempdir();
    let (client, keeper) = quiet_vm(dir.path());
    let id = client.desktop().id();
    // A, without a restart, kept at holding.
    let first = under_maintenance(dir.path(), &id, Phase::Holding);
    let lifecycle = lifecycle(&client).clone();
    let replaced = Arc::new(Mutex::new(None));
    // While the person is asked, A ends and B, a restart, is left at
    // stopping.
    let replacing = {
        let (lifecycle, id, replaced) = (lifecycle.clone(), id.clone(), replaced.clone());
        move |_: &grant::Operation| {
            let mut record = lifecycle.record(&id).unwrap().unwrap();
            record.maintenance_settled();
            lifecycle.keep_record(&record).unwrap();
            let next = under_maintenance(lifecycle.home(), &id, Phase::Stopping);
            let mut record = lifecycle.record(&id).unwrap().unwrap();
            record.maintenance.as_mut().unwrap().restart = true;
            lifecycle.keep_record(&record).unwrap();
            *replaced.lock().unwrap() = Some(next);
            Ok(Token::new())
        }
    };
    let before = {
        let record = record(&client);
        record.maintenance.unwrap()
    };

    let refused = asked(&client, BEGIN, replacing).await.unwrap_err();

    assert_eq!(before.change, first);
    assert!(!before.restart);
    assert_eq!(refused.code, Code::Denied, "{refused:?}");
    assert_eq!(
        refused.error,
        "work's maintenance changed while this begin waited, so opening its window needs your \
         grant again; nothing was asked of it. Run atlas desktop maintain work again."
    );
    let next = replaced.lock().unwrap().clone().unwrap();
    let kept = maintenance(&client).unwrap();
    assert_eq!(kept.change, next, "B left as it is");
    assert_eq!(kept.phase, Phase::Stopping);
    assert!(kept.restart);
    assert!(kept.stop.is_none());
    for op in ["stop", "start", "window", "void"] {
        assert!(keeper.runner.asked_for(op).is_empty(), "no {op} asked");
    }
    assert!(keeper.asked_for("hold").is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_stop_atlas_vm_answers_pending_is_followed_past_a_window_changes_patience_until_it_is_done()
 {
    let dir = tempdir();
    let (client, keeper) = quiet_vm(dir.path());
    // Atlas VM takes the stop and says it pending, the guest shutting down;
    // the VM is stopped well after a window change would be given up.
    keeper.runner.stalling_stop();
    let person = Person::default();
    let patience = super::super::Patience {
        within: Duration::from_millis(300),
        powering: Duration::from_secs(10),
        every: Duration::from_millis(50),
    };

    let beginning = tokio::task::spawn_blocking({
        let (client, allowing) = (client.clone(), person.allowing());
        move || super::super::Maintainer::of(&client, patience)?.run(RESTART, &allowing, &|_| {})
    });
    until("the stop asked", || {
        !keeper.runner.asked_for("stop").is_empty()
    })
    .await;
    tokio::time::sleep(Duration::from_millis(900)).await;
    let outcomes = keeper.runner.asked_for("outcome").len();
    keeper.runner.set_power("stopped");
    let begun = beginning.await.unwrap().unwrap();

    assert!(
        outcomes > 2,
        "followed by its outcome meanwhile: {outcomes}"
    );
    assert_eq!(begun["maintenance"], "on", "{begun}");
    assert_eq!(keeper.runner.asked_for("stop").len(), 1, "asked once");
    assert_eq!(keeper.runner.asked_for("start").len(), 1);
    assert!(pending(&client).is_empty());
}

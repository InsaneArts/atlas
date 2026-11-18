use super::*;
use crate::{
    activity_log::Op,
    test_support::{ScriptedKeeper, vm_through_keeper},
};
use atlas_core::process::Instance;
use std::path::Path;

/// Waited for briefly, so a test that waits does not wait long.
const QUICK: Patience = Patience {
    within: Duration::from_secs(10),
    every: Duration::from_millis(50),
};

/// A VM client, its keeper and Atlas VM stood in for in `directory`, the
/// VM's power `power`.
fn vm(directory: &Path, power: &str) -> (Client, ScriptedKeeper) {
    let (client, keeper) = vm_through_keeper(directory);
    keeper.runner.set_power(power);
    (client, keeper)
}

fn lifecycle(directory: &Path) -> Lifecycle {
    Lifecycle::under(directory)
}

fn record(client: &Client, directory: &Path) -> Record {
    lifecycle(directory)
        .record(&client.desktop().id())
        .unwrap()
        .unwrap()
}

/// A start an earlier command kept in the VM's record and claimed as
/// Atlas VM's, then went without settling.
fn left_by_an_earlier_command(client: &Client, directory: &Path) -> Token {
    let change = Token::new();
    let mut kept = record(client, directory);
    kept.starts.push(change.clone());
    kept.sending(&Operation::Start(change.clone())).unwrap();
    lifecycle(directory).keep_record(&kept).unwrap();
    drop(
        client
            .controller
            .quarantine_by(change.as_str(), Executor::Vm)
            .unwrap(),
    );
    change
}

/// What holds a VM from being started, done to it in a directory.
type Holding = fn(&Client, &Path);

fn tempdir() -> tempfile::TempDir {
    tempfile::Builder::new().tempdir_in("/tmp").unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_stopped_vm_is_started_its_start_kept_and_claimed_before_it_is_asked_and_cleared_once_done()
 {
    let dir = tempdir();
    let (client, keeper) = vm(dir.path(), "stopped");
    keeper.runner.taking(Duration::from_millis(500));

    let starting = tokio::spawn({
        let client = client.clone();
        async move { ensure_running_within(&client, QUICK).await }
    });
    while keeper.runner.asked_for("start").is_empty() {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let asked = keeper.runner.asked_for("start")[0]["change"].clone();
    let kept = record(&client, dir.path());
    let pending = client.controller.pending().unwrap();
    let started = starting.await.unwrap().unwrap().unwrap();

    assert_eq!(kept.starts, vec![started.change.clone()], "kept first");
    assert_eq!(kept.sent(&started.change).unwrap().op, "start");
    assert_eq!(pending, vec![started.change.to_string()], "then claimed");
    assert_eq!(asked, json!(started.change));
    assert_eq!(keeper.runner.asked_for("start").len(), 1);
    assert_eq!(keeper.runner.power(), "running");
    assert_eq!(
        started.said(),
        json!({"desktop": "work", "change": started.change})
    );
    let settled = record(&client, dir.path());
    assert!(settled.starts.is_empty(), "{settled:?}");
    assert!(
        settled.sent(&started.change).is_none(),
        "forgotten once cleared"
    );
    assert!(client.controller.pending().unwrap().is_empty());
    let rows = crate::activity_log::recent(client.output_dir().unwrap(), 8);
    let row = rows
        .iter()
        .find(|row| row.operation == Some(Op::Start))
        .expect("the start is in the activity log");
    assert_eq!(row.ok, Some(true));
    assert_eq!(
        row.verb, None,
        "the row says the VM started, not the command"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_running_vm_goes_on_with_nothing_asked_of_atlas_vm_but_its_power() {
    let dir = tempdir();
    let (client, keeper) = vm(dir.path(), "running");

    let started = ensure_running_within(&client, QUICK).await.unwrap();

    assert_eq!(started, None);
    assert!(keeper.runner.asked_for("start").is_empty());
    assert!(keeper.runner.asked_for("outcome").is_empty());
    assert_eq!(keeper.runner.asked_for("state").len(), 1);
    assert!(keeper.asked_for("observe").is_empty(), "nothing waited for");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_vm_is_never_started_while_the_station_is_stopped_removed_uninstalled_or_handed_over() {
    let cases: [(&str, Holding); 4] = [
        ("stopped", |client, _| {
            client.controller.note_stop(true).unwrap()
        }),
        ("being removed", |client, directory| {
            let mut kept = record(client, directory);
            kept.state = State::Removing;
            lifecycle(directory).keep_record(&kept).unwrap();
        }),
        ("uninstall", |client, directory| {
            drop(
                lifecycle(directory)
                    .uninstalling(&Token::new(), &[client.desktop().id()])
                    .unwrap(),
            );
        }),
        ("handover", |client, directory| {
            let folder = crate::station::Station::under(directory).path_of(&client.desktop().id());
            std::fs::create_dir_all(&folder).unwrap();
            handover::Record::new(Token::new(), Instance::new(41, [100, 0]))
                .keep(&folder)
                .unwrap();
        }),
    ];
    for (case, hold) in cases {
        let dir = tempdir();
        let (client, keeper) = vm(dir.path(), "stopped");
        hold(&client, dir.path());

        let refused = ensure_running_within(&client, QUICK).await.unwrap_err();

        let said = match case {
            "stopped" => "The person stopped agents on work",
            "being removed" => "work is being removed",
            "uninstall" => "uninstall",
            _ => "handing work's session to the new build",
        };
        assert!(refused.error.contains(said), "{case}: {refused:?}");
        assert!(keeper.runner.asked_for("start").is_empty(), "{case}");
        assert_eq!(keeper.runner.power(), "stopped", "{case}");
        assert!(client.controller.pending().unwrap().is_empty(), "{case}");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_start_whose_reply_is_lost_is_followed_by_what_atlas_vm_says_it_came_to() {
    let dir = tempdir();
    let (client, keeper) = vm(dir.path(), "stopped");
    keeper.runner.dropping(1);

    let started = ensure_running_within(&client, QUICK)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(keeper.runner.asked_for("start").len(), 1, "asked once");
    let outcomes = keeper.runner.asked_for("outcome");
    assert!(!outcomes.is_empty());
    assert!(
        outcomes
            .iter()
            .all(|asked| asked["change"] == json!(started.change))
    );
    assert!(record(&client, dir.path()).starts.is_empty());
    assert!(client.controller.pending().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_start_whose_command_was_killed_once_atlas_vm_took_it_is_cleared_by_the_next_command() {
    let dir = tempdir();
    let (client, keeper) = vm(dir.path(), "running");
    let change = left_by_an_earlier_command(&client, dir.path());
    keeper
        .runner
        .keep(change.as_str(), "start", Some(json!({"state": "done"})));
    assert_eq!(
        client.controller.pending().unwrap(),
        vec![change.to_string()]
    );

    let started = ensure_running_within(&client, QUICK).await.unwrap();

    assert_eq!(started, None, "it was started by the command that asked");
    assert!(keeper.runner.asked_for("start").is_empty());
    assert_eq!(
        keeper.runner.asked_for("outcome")[0]["change"],
        json!(change)
    );
    assert!(record(&client, dir.path()).starts.is_empty());
    assert!(client.controller.pending().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_start_atlas_vm_is_still_making_is_waited_for_and_never_asked_again() {
    let dir = tempdir();
    let (client, keeper) = vm(dir.path(), "starting");
    let change = left_by_an_earlier_command(&client, dir.path());
    keeper.runner.keep(change.as_str(), "start", None);

    let (started, ()) = tokio::join!(ensure_running_within(&client, QUICK), async {
        // As the start goes on, the power it asked for comes.
        tokio::time::sleep(Duration::from_millis(300)).await;
        keeper.runner.set_power("running");
    });

    let started = started.unwrap();
    assert_eq!(started, None);
    assert!(
        keeper.runner.asked_for("start").is_empty(),
        "never asked again"
    );
    assert!(record(&client, dir.path()).starts.is_empty());
    assert!(client.controller.pending().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_start_atlas_vm_has_not_finished_keeps_the_vm_said_to_be_starting_and_its_claim_pending()
{
    let dir = tempdir();
    let (client, keeper) = vm(dir.path(), "starting");
    let change = left_by_an_earlier_command(&client, dir.path());
    keeper.runner.keep(change.as_str(), "start", None);
    let brief = Patience {
        within: Duration::from_millis(400),
        every: Duration::from_millis(50),
    };

    let refused = ensure_running_within(&client, brief).await.unwrap_err();

    assert_eq!(refused.code, Code::NotReady, "{refused:?}");
    assert!(
        refused
            .error
            .contains("Atlas VM has not said yet what an earlier start of work came to"),
        "{refused:?}"
    );
    assert!(keeper.runner.asked_for("start").is_empty());
    assert_eq!(record(&client, dir.path()).starts, vec![change.clone()]);
    assert_eq!(
        client.controller.pending().unwrap(),
        vec![change.to_string()]
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_start_atlas_vm_never_heard_of_is_asked_again_as_the_very_same_start() {
    let dir = tempdir();
    let (client, keeper) = vm(dir.path(), "stopped");
    let change = left_by_an_earlier_command(&client, dir.path());

    let started = ensure_running_within(&client, QUICK)
        .await
        .unwrap()
        .unwrap();

    let starts = keeper.runner.asked_for("start");
    assert_eq!(starts.len(), 1, "{starts:?}");
    assert_eq!(starts[0]["change"], json!(change), "the very same start");
    assert_eq!(started.change, change);
    assert!(record(&client, dir.path()).starts.is_empty());
    assert!(client.controller.pending().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_start_whose_claim_cannot_be_cleared_is_kept_for_the_next_command() {
    // The marker unreadable, then malformed: neither is ever taken as cleared.
    for malformed in [false, true] {
        let dir = tempdir();
        let (client, keeper) = vm(dir.path(), "running");
        let change = left_by_an_earlier_command(&client, dir.path());
        keeper
            .runner
            .keep(change.as_str(), "start", Some(json!({"state": "done"})));
        let marker = client.controller.marker_path().to_owned();
        std::fs::remove_file(&marker).unwrap();
        if malformed {
            std::fs::write(&marker, b"{").unwrap();
        } else {
            std::fs::create_dir(&marker).unwrap();
        }

        let refused = ensure_running_within(&client, QUICK).await.unwrap_err();
        let kept = record(&client, dir.path());
        if !malformed {
            std::fs::remove_dir(&marker).unwrap();
        }
        std::fs::write(&marker, serde_json::to_vec(&[change.as_str()]).unwrap()).unwrap();
        let next = ensure_running_within(&client, QUICK).await.unwrap();

        assert_eq!(refused.code, Code::State, "{malformed}: {refused:?}");
        assert!(
            refused
                .error
                .contains("could not be cleared, so the start is kept"),
            "{malformed}: {refused:?}"
        );
        assert_eq!(kept.starts, vec![change.clone()], "{malformed}");
        assert!(kept.sent(&change).is_some(), "{malformed}");
        assert_eq!(next, None, "{malformed}");
        assert!(record(&client, dir.path()).starts.is_empty(), "{malformed}");
        assert!(
            client.controller.pending().unwrap().is_empty(),
            "{malformed}"
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_refusal_found_by_recovery_begins_its_minute_before_its_start_is_forgotten() {
    let dir = tempdir();
    let (client, keeper) = vm(dir.path(), "stopped");
    let change = left_by_an_earlier_command(&client, dir.path());
    keeper.runner.keep(
        change.as_str(),
        "start",
        Some(json!({"state": "refused", "reason": "Its disk is in use."})),
    );
    // The claim cannot be cleared at first: the minute is kept all the same.
    let marker = client.controller.marker_path().to_owned();
    std::fs::write(&marker, b"{").unwrap();

    let uncleared = ensure_running_within(&client, QUICK).await.unwrap_err();
    let kept = record(&client, dir.path());
    std::fs::write(&marker, serde_json::to_vec(&[change.as_str()]).unwrap()).unwrap();
    let held = ensure_running_within(&client, QUICK).await.unwrap_err();

    assert!(
        uncleared.error.contains("could not be cleared"),
        "{uncleared:?}"
    );
    assert_eq!(kept.starts, vec![change.clone()]);
    let minute = kept.refused_start.unwrap();
    assert_eq!(minute.reason, "Its disk is in use.");
    assert!(
        atlas_core::time::now_ms() - minute.at_ms < 10_000,
        "begun when seen"
    );
    assert_eq!(
        held.error,
        "Atlas VM refused to start work: Its disk is in use."
    );
    assert!(
        held.details["next"]
            .as_str()
            .unwrap()
            .contains("once a minute has passed"),
        "{held:?}"
    );
    assert!(
        keeper.runner.asked_for("start").is_empty(),
        "never asked again"
    );
    assert!(record(&client, dir.path()).starts.is_empty());
    assert!(client.controller.pending().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_start_refused_while_its_earlier_request_may_still_come_is_never_cleared_on_that_alone() {
    let dir = tempdir();
    let (client, keeper) = vm(dir.path(), "stopped");
    let change = left_by_an_earlier_command(&client, dir.path());
    let socket = crate::vm::runner::socket_in(dir.path(), &client.desktop().id());
    // Atlas VM busy with another change, the earlier command's request for
    // this start still on its way, read only once that change is done.
    keeper.runner.taking(Duration::from_millis(1200));
    let busy = std::thread::spawn({
        let socket = socket.clone();
        move || crate::vm::control::ask(&socket, &json!({"op": "stop", "change": Token::new()}))
    });
    while keeper.runner.asked_for("stop").is_empty() {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    keeper.runner.taking(Duration::ZERO);
    keeper
        .runner
        .queueing(change.as_str(), Duration::from_millis(1500));
    let original = std::thread::spawn({
        let change = change.clone();
        move || crate::vm::control::ask(&socket, &json!({"op": "start", "change": change}))
    });
    while keeper.runner.asked_for("start").is_empty() {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    let started = ensure_running_within(&client, QUICK).await.unwrap();
    busy.join().unwrap().unwrap();
    original.join().unwrap().unwrap();

    assert_eq!(started.map(|started| started.change), Some(change.clone()));
    let starts = keeper.runner.asked_for("start");
    assert!(
        starts.len() > 2,
        "refused busy, then asked again: {starts:?}"
    );
    assert!(
        starts.iter().all(|asked| asked["change"] == json!(change)),
        "never a start of another change: {starts:?}"
    );
    let begun: Vec<String> = keeper
        .runner
        .begun()
        .into_iter()
        .filter(|begun| *begun != change.to_string())
        .collect();
    assert_eq!(begun.len(), 1, "only the stop besides: {begun:?}");
    assert_eq!(
        keeper
            .runner
            .begun()
            .iter()
            .filter(|begun| **begun == change.to_string())
            .count(),
        1,
        "the start done once"
    );
    assert!(record(&client, dir.path()).starts.is_empty());
    assert!(client.controller.pending().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_persons_stop_given_while_a_start_waits_for_its_claim_holds_it_before_it_is_asked() {
    use fs2::FileExt;
    let dir = tempdir();
    let (client, keeper) = vm(dir.path(), "stopped");
    let change = left_by_an_earlier_command(&client, dir.path());
    // The marker's lock held, so asking again waits for its claim.
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(client.controller.marker_lock_path())
        .unwrap();
    lock.lock_exclusive().unwrap();

    let (held, ()) = tokio::join!(ensure_running_within(&client, QUICK), async {
        tokio::time::sleep(Duration::from_millis(300)).await;
        client.controller.note_stop(true).unwrap();
        FileExt::unlock(&lock).unwrap();
    });

    let held = held.unwrap_err();
    assert_eq!(held.code, Code::Stopped, "{held:?}");
    assert!(keeper.runner.asked_for("start").is_empty(), "never asked");
    assert_eq!(
        record(&client, dir.path()).starts,
        vec![change.clone()],
        "kept"
    );
    assert_eq!(
        client.controller.pending().unwrap(),
        vec![change.to_string()]
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_marker_held_past_the_wait_returns_in_time_with_nothing_asked() {
    use fs2::FileExt;
    let dir = tempdir();
    let (client, keeper) = vm(dir.path(), "stopped");
    std::fs::create_dir_all(client.controller.marker_lock_path().parent().unwrap()).unwrap();
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(client.controller.marker_lock_path())
        .unwrap();
    lock.lock_exclusive().unwrap();
    let brief = Patience {
        within: Duration::from_millis(1500),
        every: Duration::from_millis(50),
    };
    let began = Instant::now();

    let refused = ensure_running_within(&client, brief).await.unwrap_err();

    assert!(
        began.elapsed() < Duration::from_secs(5),
        "{:?}",
        began.elapsed()
    );
    assert_eq!(refused.code, Code::NotReady, "{refused:?}");
    assert!(keeper.runner.asked_for("start").is_empty());
    assert!(
        record(&client, dir.path()).starts.is_empty(),
        "never claimed"
    );
    FileExt::unlock(&lock).unwrap();
    assert!(client.controller.pending().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_handover_that_holds_the_vm_while_a_start_is_admitted_keeps_it_from_being_asked() {
    let dir = tempdir();
    let (client, keeper) = vm(dir.path(), "stopped");
    // An update's handover: the VM's exclusion first, then its record.
    let updating = atlas_control::Controller::of_at(&client.desktop(), &dir.path().join("state"));
    let exclusion = updating
        .acquire_for_service_change(false, Duration::from_secs(1))
        .unwrap();
    let folder = crate::station::Station::under(dir.path()).path_of(&client.desktop().id());
    std::fs::create_dir_all(&folder).unwrap();

    let (refused, ()) = tokio::join!(ensure_running_within(&client, QUICK), async {
        tokio::time::sleep(Duration::from_millis(300)).await;
        handover::Record::new(Token::new(), Instance::new(41, [100, 0]))
            .keep(&folder)
            .unwrap();
        drop(exclusion);
    });

    let refused = refused.unwrap_err();
    assert!(
        refused
            .error
            .contains("handing work's session to the new build"),
        "{refused:?}"
    );
    assert!(keeper.runner.asked_for("start").is_empty());
    assert!(client.controller.pending().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn two_commands_at_once_give_one_start() {
    let dir = tempdir();
    let (client, keeper) = vm(dir.path(), "stopped");
    keeper.runner.taking(Duration::from_millis(800));
    let another = client.clone();

    let (one, other) = tokio::join!(
        ensure_running_within(&client, QUICK),
        ensure_running_within(&another, QUICK)
    );

    let (one, other) = (one.unwrap(), other.unwrap());
    assert_eq!(keeper.runner.asked_for("start").len(), 1);
    assert_eq!(
        usize::from(one.is_some()) + usize::from(other.is_some()),
        1,
        "{one:?} {other:?}"
    );
    assert!(record(&client, dir.path()).starts.is_empty());
    assert!(client.controller.pending().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_start_atlas_vm_refused_is_said_in_its_words_and_not_asked_again_for_a_minute() {
    let dir = tempdir();
    let (client, keeper) = vm(dir.path(), "stopped");
    keeper.runner.refusing(Some("Its disk is in use."));

    let first = ensure_running_within(&client, QUICK).await.unwrap_err();
    let again = ensure_running_within(&client, QUICK).await.unwrap_err();
    let asked_meanwhile = keeper.runner.asked_for("start").len();
    let mut kept = record(&client, dir.path());
    let refused = kept.refused_start.clone().unwrap();
    kept.refused_start.as_mut().unwrap().at_ms -= 61_000;
    lifecycle(dir.path()).keep_record(&kept).unwrap();
    keeper.runner.refusing(None);
    let later = ensure_running_within(&client, QUICK).await.unwrap();

    assert_eq!(
        first.error,
        "Atlas VM refused to start work: Its disk is in use."
    );
    assert_eq!(again.error, first.error);
    assert!(
        again.details["next"]
            .as_str()
            .unwrap()
            .contains("once a minute has passed"),
        "{again:?}"
    );
    assert_eq!(asked_meanwhile, 1, "not asked again within the minute");
    assert_eq!(refused.reason, "Its disk is in use.");
    assert!(later.is_some());
    assert_eq!(keeper.runner.asked_for("start").len(), 2);
    let settled = record(&client, dir.path());
    assert!(settled.refused_start.is_none(), "{settled:?}");
    assert!(client.controller.pending().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_macs_limit_on_running_vms_is_said_in_atlas_vms_own_words() {
    let dir = tempdir();
    let (client, keeper) = vm(dir.path(), "stopped");
    keeper.runner.refusing(Some(
        "This Mac already runs 2 macOS VMs, as many as it may.",
    ));

    let refused = ensure_running_within(&client, QUICK).await.unwrap_err();

    assert_eq!(
        refused.error,
        "Atlas VM refused to start work: This Mac already runs 2 macOS VMs, as many as it may."
    );
    assert!(client.controller.pending().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_vm_atlas_vm_finds_not_made_is_refused_with_what_to_do() {
    let dir = tempdir();
    let (client, keeper) = vm(dir.path(), "absent");

    let refused = ensure_running_within(&client, QUICK).await.unwrap_err();

    assert!(refused.error.contains("not made"), "{refused:?}");
    assert!(
        refused.details["next"]
            .as_str()
            .unwrap()
            .contains("atlas desktop remove work"),
        "{refused:?}"
    );
    assert!(keeper.runner.asked_for("start").is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_start_atlas_vm_does_not_take_while_busy_is_asked_again_once_it_is_free() {
    let dir = tempdir();
    let (client, keeper) = vm(dir.path(), "stopped");
    // Another change of Atlas VM's own is being made meanwhile.
    let other = Token::new();
    keeper.runner.taking(Duration::from_millis(1500));
    let busy = std::thread::spawn({
        let socket = crate::vm::runner::socket_in(dir.path(), &client.desktop().id());
        move || crate::vm::control::ask(&socket, &json!({"op": "stop", "change": other}))
    });
    while keeper.runner.asked_for("stop").is_empty() {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    keeper.runner.taking(Duration::ZERO);

    let started = ensure_running_within(&client, QUICK).await.unwrap();
    busy.join().unwrap().unwrap();

    assert!(started.is_some());
    let starts = keeper.runner.asked_for("start");
    assert!(
        starts.len() >= 2,
        "refused while busy, then taken: {starts:?}"
    );
    assert_ne!(
        starts[0]["change"], starts[1]["change"],
        "each a start of its own"
    );
    assert!(record(&client, dir.path()).starts.is_empty());
    assert!(client.controller.pending().unwrap().is_empty());
}

#[test]
fn a_record_with_no_start_kept_is_written_as_an_older_atlas_reads_it() {
    let dir = tempdir();
    let reservation = crate::vm::lifecycle::Reservation {
        name: "work".to_owned(),
        id: DesktopId::parse(crate::test_support::VM_ID).unwrap(),
        named: true,
        resumed: false,
    };
    let record = Record::adding(
        &reservation,
        dir.path().join("settings.toml"),
        crate::vm::lifecycle::Provenance {
            program: "/Applications/Atlas.app/Contents/MacOS/atlas".into(),
            requirement: "identifier \"com.insanearts.atlas\"".to_owned(),
        },
    );

    let written = serde_json::to_value(&record).unwrap();

    assert!(written.get("starts").is_none(), "{written}");
    assert!(written.get("refusedStart").is_none(), "{written}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_signal_that_stops_the_command_while_atlas_vm_reads_the_power_keeps_any_start_unasked() {
    use atlas_control::interrupt::Interrupt;
    use nix::sys::signal::Signal;
    let dir = tempdir();
    let (mut client, keeper) = vm(dir.path(), "stopped");
    let interrupt = Interrupt::new_for_test(Duration::from_millis(200));
    client.controller = client.controller.clone().interrupted_by(interrupt.clone());
    // The first read, before the locks, at once; the one under them held.
    keeper.runner.slow_states(1, Duration::from_millis(600));

    let (refused, ()) = tokio::join!(ensure_running_within(&client, QUICK), async {
        while keeper.runner.asked_for("state").len() < 2 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        interrupt.ask_for_test(Signal::SIGINT);
    });

    let refused = refused.unwrap_err();
    assert!(
        refused.error.contains("nothing more was started"),
        "{refused:?}"
    );
    assert!(keeper.runner.asked_for("start").is_empty(), "no start sent");
    assert!(record(&client, dir.path()).starts.is_empty(), "none begun");
    assert!(client.controller.pending().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_signal_given_while_a_start_atlas_vm_never_heard_of_waits_for_its_claim_keeps_it_pending()
{
    use atlas_control::interrupt::Interrupt;
    use fs2::FileExt;
    use nix::sys::signal::Signal;
    let dir = tempdir();
    let (mut client, keeper) = vm(dir.path(), "stopped");
    let interrupt = Interrupt::new_for_test(Duration::from_millis(200));
    client.controller = client.controller.clone().interrupted_by(interrupt.clone());
    let change = left_by_an_earlier_command(&client, dir.path());
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(client.controller.marker_lock_path())
        .unwrap();
    lock.lock_exclusive().unwrap();

    let (refused, ()) = tokio::join!(ensure_running_within(&client, QUICK), async {
        while keeper.runner.asked_for("outcome").is_empty() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
        interrupt.ask_for_test(Signal::SIGINT);
        FileExt::unlock(&lock).unwrap();
    });

    let refused = refused.unwrap_err();
    assert!(
        refused.error.contains("nothing more was started"),
        "{refused:?}"
    );
    assert!(
        keeper.runner.asked_for("start").is_empty(),
        "never sent again"
    );
    assert_eq!(
        record(&client, dir.path()).starts,
        vec![change.clone()],
        "kept"
    );
    assert_eq!(
        client.controller.pending().unwrap(),
        vec![change.to_string()]
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_vm_under_maintenance_is_never_started_when_needed() {
    use crate::vm::lifecycle::record::Phase;
    for phase in [Phase::Holding, Phase::Open, Phase::Closing] {
        let dir = tempdir();
        let (client, keeper) = vm(dir.path(), "stopped");
        crate::test_support::under_maintenance(dir.path(), &client.desktop().id(), phase);

        let refused = ensure_running_within(&client, QUICK).await.unwrap_err();

        assert_eq!(refused.code, Code::Busy, "{phase:?}: {refused:?}");
        assert!(refused.error.starts_with("work"), "{phase:?}: {refused:?}");
        assert!(refused.error.contains("maintenance"), "{phase:?}");
        assert!(keeper.runner.asked_for("start").is_empty(), "{phase:?}");
        assert_eq!(keeper.runner.power(), "stopped", "{phase:?}");
        assert!(client.controller.pending().unwrap().is_empty(), "{phase:?}");
    }
}

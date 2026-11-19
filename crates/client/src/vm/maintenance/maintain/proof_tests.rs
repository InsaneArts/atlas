use super::super::tests::{
    BEGIN, Boundary, END, Person, asked, asked_within, cut_short, lifecycle, maintenance, pending,
    quiet_vm, record, tempdir, until,
};
use crate::{
    client::Client,
    test_support::{FIRST_RUN, under_maintenance},
    vm::{
        lifecycle::record::{Ended, Phase},
        runner::socket_in,
    },
};
use atlas_core::{error::Code, files::Token};
use serde_json::{Value, json};
use std::{path::PathBuf, time::Duration};

/// How long an end that cannot be proven is given.
const SHORT: Duration = Duration::from_secs(1);

/// The run Atlas VM runs as once the one that opened the window died.
const NEXT_RUN: &str = "0b0e1a5e-7a1d-4e3c-9a55-1a7a5f0e0002";

/// Where `client`'s VM's Atlas VM listens.
fn runner_socket(client: &Client) -> PathBuf {
    socket_in(lifecycle(client).home(), &client.desktop().id())
}

/// The ending a refused end of `client`'s VM's maintenance left: kept
/// ending, with no proof, agents held, none let in.
fn left_ending(client: &Client, keeper: &crate::test_support::ScriptedKeeper) {
    let kept = maintenance(client).unwrap();
    assert_eq!(kept.phase, Phase::Closing);
    assert!(kept.ended.is_none(), "no proof kept");
    assert!(client.maintenance_held().is_some(), "agents wait");
    assert!(keeper.asked_for("release").is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn maintenance_whose_atlas_vm_died_with_input_on_ends_only_once_its_next_run_says_every_earlier_vm_gone_and_turns_input_off()
 {
    let dir = tempdir();
    let (client, keeper) = quiet_vm(dir.path());
    let person = Person::default();
    asked(&client, BEGIN, person.allowing()).await.unwrap();
    // Killed with its window taking input; launchd runs it again.
    keeper.runner.runs_again_as(NEXT_RUN);

    let unproven = asked_within(&client, END, SHORT, person.allowing())
        .await
        .unwrap_err();
    let off = keeper.runner.asked_for("window");
    left_ending(&client, &keeper);
    keeper.runner.saying_earlier_gone(Some(true));
    let ended = asked(&client, END, person.refusing()).await.unwrap();

    assert_eq!(unproven.code, Code::NotReady, "{unproven:?}");
    assert_eq!(
        unproven.error,
        "Atlas VM ran again since it opened work's window, and whether that window is gone \
         cannot be told yet, so agents wait. Run atlas desktop maintain work --end again later."
    );
    assert_eq!(off.len(), 2, "the window-on and its own window-off");
    assert_eq!(off[1]["input"], false);
    assert_eq!(ended["maintenance"], "none", "{ended}");
    assert_eq!(ended["ended"]["by"], NEXT_RUN, "{ended}");
    assert_eq!(ended["ended"]["earlier"], FIRST_RUN, "{ended}");
    assert_eq!(ended["ended"]["earlierGone"], true, "{ended}");
    assert_eq!(person.asked().len(), 2, "no grant to go on");
    let windows = keeper.runner.asked_for("window");
    assert_eq!(windows.len(), 3);
    assert_eq!(
        windows[2]["change"], off[1]["change"],
        "the same window-off, answered from its record"
    );
    assert_eq!(keeper.runner.begun().len(), 2, "turned off once");
    assert!(!keeper.runner.window_open());
    assert!(maintenance(&client).is_none());
    assert!(pending(&client).is_empty());
    assert_eq!(keeper.asked_for("release").len(), 1);
    assert!(client.maintenance_held().is_none());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_lost_socket_alone_never_ends_maintenance() {
    let dir = tempdir();
    let (client, keeper) = quiet_vm(dir.path());
    let person = Person::default();
    asked(&client, BEGIN, person.allowing()).await.unwrap();
    keeper.runner.runs_again_as(NEXT_RUN);
    keeper.runner.saying_earlier_gone(Some(true));
    // Atlas VM's socket goes once it took the window-off, before anything
    // more is asked.
    let socket = runner_socket(&client);
    keeper.runner.on_window(move || {
        let _ = std::fs::remove_file(&socket);
    });

    let lost = asked_within(&client, END, SHORT, person.allowing())
        .await
        .unwrap_err();
    left_ending(&client, &keeper);
    let again = asked_within(&client, END, Duration::from_millis(300), person.refusing())
        .await
        .unwrap_err();

    assert_eq!(lost.code, Code::NotReady, "{lost:?}");
    assert_eq!(
        lost.error,
        "Atlas VM, which runs work, is not answering, so its window may still take your \
         input; agents wait. Run atlas desktop maintain work --end again."
    );
    assert!(
        matches!(again.code, Code::NotReady | Code::Transport),
        "{again:?}"
    );
    left_ending(&client, &keeper);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_later_run_that_cannot_yet_tell_the_earlier_vm_gone_keeps_maintenance_pending() {
    let dir = tempdir();
    let (client, keeper) = quiet_vm(dir.path());
    let person = Person::default();
    asked(&client, BEGIN, person.allowing()).await.unwrap();
    keeper.runner.runs_again_as(NEXT_RUN);

    let unproven = asked_within(&client, END, SHORT, person.allowing())
        .await
        .unwrap_err();
    left_ending(&client, &keeper);
    // Every earlier run's VM said gone, but the VM's power not seen just now.
    keeper.runner.saying_earlier_gone(Some(true));
    keeper.runner.blind(true);
    let unconfirmed = asked_within(&client, END, SHORT, person.refusing())
        .await
        .unwrap_err();

    for refused in [&unproven, &unconfirmed] {
        assert_eq!(refused.code, Code::NotReady, "{refused:?}");
        assert!(
            refused
                .error
                .starts_with("Atlas VM ran again since it opened work's window"),
            "{refused:?}"
        );
    }
    left_ending(&client, &keeper);
    let held = client.maintenance_held().unwrap();
    assert!(
        held.error.starts_with("work's maintenance is ending"),
        "{held:?}"
    );
    let resumed = client.maintenance().resume_refused("work").unwrap();
    assert_eq!(resumed.code, Code::Busy, "{resumed:?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_runner_that_does_not_say_whether_earlier_runs_are_gone_keeps_maintenance_pending_with_advice()
 {
    let dir = tempdir();
    let (client, keeper) = quiet_vm(dir.path());
    let person = Person::default();
    asked(&client, BEGIN, person.allowing()).await.unwrap();
    keeper.runner.runs_again_as(NEXT_RUN);
    keeper.runner.saying_earlier_gone(None);

    let refused = asked(&client, END, person.allowing()).await.unwrap_err();

    assert_eq!(refused.code, Code::Update, "{refused:?}");
    assert_eq!(
        refused.error,
        "work's Atlas VM is from an older Atlas and cannot prove its earlier run's window gone, \
         so agents wait. Update Atlas when convenient, then run atlas desktop maintain work \
         --end again."
    );
    left_ending(&client, &keeper);
    assert!(!refused.error.contains("helper"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_proof_that_ended_maintenance_is_kept_before_the_maintenance_goes_and_a_cut_short_end_finishes_from_it()
 {
    let dir = tempdir();
    let (client, keeper) = quiet_vm(dir.path());
    let person = Person::default();
    asked(&client, BEGIN, person.allowing()).await.unwrap();
    keeper.runner.runs_again_as(NEXT_RUN);
    keeper.runner.saying_earlier_gone(Some(true));
    // The keeper does not answer to let agents in: the end stops right
    // after its proof is kept.
    *keeper.away.lock().unwrap() = true;

    let owed = asked(&client, END, person.allowing()).await.unwrap_err();
    let kept = maintenance(&client).unwrap();
    let written: Value = serde_json::from_slice(
        &std::fs::read(lifecycle(&client).record_at(&client.desktop().id())).unwrap(),
    )
    .unwrap();
    let held = client.maintenance_held();
    // Nothing of Atlas VM is reached from here.
    std::fs::remove_file(runner_socket(&client)).unwrap();
    let asked_of_runner = keeper.runner.ops().len();
    *keeper.away.lock().unwrap() = false;
    let ended = asked(&client, END, person.refusing()).await.unwrap();

    assert_eq!(owed.code, Code::NotReady, "{owed:?}");
    let close = kept.close.clone().unwrap();
    assert_eq!(
        kept.ended,
        Some(Ended {
            by: NEXT_RUN.to_owned(),
            close: close.clone(),
            earlier: Some(FIRST_RUN.to_owned()),
            earlier_gone: Some(true),
        })
    );
    assert_eq!(
        written["maintenance"]["ended"],
        json!({"by": NEXT_RUN, "close": close, "earlier": FIRST_RUN, "earlierGone": true})
    );
    assert!(held.is_some(), "agents wait meanwhile");
    assert_eq!(ended["maintenance"], "none", "{ended}");
    assert_eq!(
        keeper.runner.ops().len(),
        asked_of_runner,
        "finished from the record alone"
    );
    assert_eq!(person.asked().len(), 2);
    assert_eq!(keeper.asked_for("release").len(), 2);
    assert!(maintenance(&client).is_none());
    assert!(pending(&client).is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_window_on_left_pending_by_a_run_that_died_is_settled_by_the_next_run_and_the_end_still_turns_input_off()
 {
    let dir = tempdir();
    let (client, keeper) = quiet_vm(dir.path());
    let id = client.desktop().id();
    under_maintenance(dir.path(), &id, Phase::Holding);
    // The first run began the window-on and died opening it.
    let open = cut_short(&client, Boundary::Sent, Phase::Opening, true);
    keeper.runner.keep(open.as_str(), "window-on", None);
    keeper.runner.runs_again_as(NEXT_RUN);
    let person = Person::default();

    let unsettled = asked_within(&client, END, Duration::from_millis(300), person.allowing())
        .await
        .unwrap_err();
    let before = maintenance(&client).unwrap();
    let windows_before = keeper.runner.asked_for("window").len();
    keeper.runner.saying_earlier_gone(Some(true));
    let ended = asked(&client, END, person.refusing()).await.unwrap();

    assert_eq!(unsettled.code, Code::NotReady, "{unsettled:?}");
    assert!(
        unsettled
            .error
            .starts_with("Atlas VM has not said what the request to open work's window came to"),
        "{unsettled:?}"
    );
    assert!(!before.settled);
    assert_eq!(windows_before, 0, "no window-off before it is settled");
    assert_eq!(ended["maintenance"], "none", "{ended}");
    let windows = keeper.runner.asked_for("window");
    assert_eq!(windows.len(), 1, "a window-off all the same");
    assert_eq!(windows[0]["input"], false);
    assert!(
        keeper
            .runner
            .begun()
            .iter()
            .all(|begun| *begun != open.to_string()),
        "the window-on never done"
    );
    assert!(!keeper.runner.window_open());
    assert!(pending(&client).is_empty());
    assert!(record(&client).operations.is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_window_off_done_by_a_run_gone_since_is_asked_again_of_the_run_answering_now() {
    let dir = tempdir();
    let (client, keeper) = quiet_vm(dir.path());
    let person = Person::default();
    asked(&client, BEGIN, person.allowing()).await.unwrap();
    // The next run turned the window off, its reply lost, and died too.
    let close = cut_short(&client, Boundary::Kept, Phase::Closing, false);
    keeper.runner.runs_again_as(NEXT_RUN);
    keeper
        .runner
        .keep(close.as_str(), "window-off", Some(json!({"state": "done"})));
    let mut kept = record(&client);
    kept.sending(&crate::vm::runner::Operation::Window {
        change: close.clone(),
        input: false,
        setup: false,
    })
    .unwrap();
    lifecycle(&client).keep_record(&kept).unwrap();
    keeper
        .runner
        .runs_again_as("0b0e1a5e-7a1d-4e3c-9a55-1a7a5f0e0003");
    keeper.runner.saying_earlier_gone(Some(true));
    *keeper.away.lock().unwrap() = true;

    let owed = asked(&client, END, person.refusing()).await.unwrap_err();

    assert_eq!(owed.code, Code::NotReady, "{owed:?}");
    let windows = keeper.runner.asked_for("window");
    let offs: Vec<&Value> = windows
        .iter()
        .filter(|window| window["input"] == false)
        .collect();
    assert_eq!(offs.len(), 2, "{offs:?}");
    assert_eq!(offs[0]["change"], json!(close));
    assert_ne!(offs[1]["change"], json!(close), "a change of its own");
    let ended = maintenance(&client).unwrap().ended.unwrap();
    assert_eq!(ended.by, "0b0e1a5e-7a1d-4e3c-9a55-1a7a5f0e0003");
    assert_eq!(ended.earlier.as_deref(), Some(FIRST_RUN));
    assert_ne!(ended.close, close);
    assert_eq!(
        Token::parse(offs[1]["change"].as_str().unwrap()),
        Some(ended.close)
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_run_that_finishes_recovering_partway_through_an_end_is_ended_in_that_same_end() {
    let dir = tempdir();
    let (client, keeper) = quiet_vm(dir.path());
    let person = Person::default();
    asked(&client, BEGIN, person.allowing()).await.unwrap();
    keeper.runner.runs_again_as(NEXT_RUN);

    let ending = tokio::spawn({
        let (client, allowing) = (client.clone(), person.allowing());
        async move { asked(&client, END, allowing).await }
    });
    until("the window turned off", || {
        keeper.runner.asked_for("window").len() == 2
    })
    .await;
    let states_off = keeper.runner.asked_for("state").len();
    until("its state asked again", || {
        keeper.runner.asked_for("state").len() >= states_off + 2
    })
    .await;
    // Its recovery done, it now sees every earlier run's VM gone.
    keeper.runner.saying_earlier_gone(Some(true));
    let ended = ending.await.unwrap().unwrap();

    assert_eq!(ended["maintenance"], "none", "{ended}");
    assert_eq!(ended["ended"]["by"], NEXT_RUN, "{ended}");
    assert_eq!(ended["ended"]["earlier"], FIRST_RUN, "{ended}");
    assert_eq!(person.asked().len(), 2, "one end, one grant");
    assert_eq!(
        keeper.runner.asked_for("window").len(),
        2,
        "turned off once"
    );
    assert!(maintenance(&client).is_none());
    assert!(client.maintenance_held().is_none());
}

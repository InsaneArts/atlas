use super::*;
use crate::{
    test_support::{FIRST_RUN, ScriptedKeeper, under_maintenance, vm_through_keeper},
    vm::runner::socket_in,
};
use std::{
    path::Path,
    sync::{Arc, Mutex},
};

/// How long a test gives a window change.
pub(super) const WITHIN: Duration = Duration::from_secs(10);

pub(super) const BEGIN: Maintain = Maintain {
    end: false,
    restart: false,
};
pub(super) const END: Maintain = Maintain {
    end: true,
    restart: false,
};

pub(super) fn tempdir() -> tempfile::TempDir {
    tempfile::Builder::new().tempdir_in("/tmp").unwrap()
}

/// A VM client, its keeper and Atlas VM stood in for in `directory`: the VM
/// runs, and its guest says no work agents started runs there.
pub(super) fn quiet_vm(directory: &Path) -> (Client, ScriptedKeeper) {
    let (client, keeper) = vm_through_keeper(directory);
    *keeper.leftovers.lock().unwrap() = "none";
    (client, keeper)
}

/// The person as a test has them: every grant asked of them, kept.
#[derive(Clone, Default)]
pub(super) struct Person(Arc<Mutex<Vec<grant::Operation>>>);

impl Person {
    /// Allowing whatever is asked.
    pub(super) fn allowing(&self) -> impl Fn(&grant::Operation) -> Result<Token> + Send + 'static {
        let asked = self.0.clone();
        move |operation| {
            asked.lock().unwrap().push(operation.clone());
            Ok(Token::new())
        }
    }

    /// Refusing whatever is asked.
    pub(super) fn refusing(&self) -> impl Fn(&grant::Operation) -> Result<Token> + Send + 'static {
        let asked = self.0.clone();
        move |operation| {
            asked.lock().unwrap().push(operation.clone());
            Err(Error::new(
                Code::Denied,
                "The person did not allow it in Atlas.app; nothing was done.",
            ))
        }
    }

    pub(super) fn asked(&self) -> Vec<grant::Operation> {
        self.0.lock().unwrap().clone()
    }
}

/// What `client`'s maintenance came to, as the person `asked`, each
/// window change waited for `within`, each step said to `say`.
pub(super) async fn asked_saying(
    client: &Client,
    asked: Maintain,
    within: Duration,
    grants: impl Fn(&grant::Operation) -> Result<Token> + Send + 'static,
    say: impl Fn(&str) + Send + 'static,
) -> Result<Value> {
    let client = client.clone();
    tokio::task::spawn_blocking(move || maintained_by(&client, asked, within, &grants, &say))
        .await
        .unwrap()
}

pub(super) async fn asked_within(
    client: &Client,
    asked: Maintain,
    within: Duration,
    grants: impl Fn(&grant::Operation) -> Result<Token> + Send + 'static,
) -> Result<Value> {
    asked_saying(client, asked, within, grants, |_| {}).await
}

pub(super) async fn asked(
    client: &Client,
    asked: Maintain,
    grants: impl Fn(&grant::Operation) -> Result<Token> + Send + 'static,
) -> Result<Value> {
    asked_within(client, asked, WITHIN, grants).await
}

pub(super) fn lifecycle(client: &Client) -> &Lifecycle {
    client.lifecycle.as_ref().unwrap()
}

pub(super) fn record(client: &Client) -> Record {
    lifecycle(client)
        .record(&client.desktop().id())
        .unwrap()
        .unwrap()
}

pub(super) fn maintenance(client: &Client) -> Option<Maintenance> {
    record(client).maintenance
}

/// Every window change kept as sent in `client`'s VM's record.
pub(super) fn windows_sent(client: &Client) -> Vec<Token> {
    record(client)
        .operations
        .into_iter()
        .filter(|sent| sent.op == "window")
        .map(|sent| sent.change)
        .collect()
}

pub(super) fn pending(client: &Client) -> Vec<String> {
    client.controller.pending().unwrap()
}

/// Wait, within ten seconds, until `holds`.
pub(super) async fn until(what: &str, holds: impl Fn() -> bool) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    while !holds() {
        assert!(tokio::time::Instant::now() < deadline, "never {what}");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_maintenance_is_kept_and_every_agent_held_before_atlas_vm_is_asked_to_open_its_window_and_the_window_change_stays_claimed_until_answered()
 {
    let dir = tempdir();
    let (client, keeper) = quiet_vm(dir.path());
    keeper.runner.taking(Duration::from_millis(500));
    let person = Person::default();

    let beginning = tokio::spawn({
        let (client, allowing) = (client.clone(), person.allowing());
        async move { asked(&client, BEGIN, allowing).await }
    });
    until("the window asked", || {
        !keeper.runner.asked_for("window").is_empty()
    })
    .await;
    let asking = maintenance(&client).unwrap();
    let held = keeper.asked_for("hold");
    let claimed = pending(&client);
    let sent = windows_sent(&client);
    let begun = beginning.await.unwrap().unwrap();

    let open = asking.open.clone().unwrap();
    assert_eq!(asking.phase, Phase::Opening);
    assert_eq!(held.len(), 1, "held first");
    assert_eq!(held[0]["change"], json!(asking.change));
    assert_eq!(claimed, vec![open.to_string()], "then claimed");
    assert_eq!(sent, vec![open.clone()], "and kept before it was sent");
    let window = &keeper.runner.asked_for("window")[0];
    assert_eq!(window["change"], json!(open));
    assert_eq!(window["input"], true);
    assert_eq!(begun["maintenance"], "on", "{begun}");
    assert_eq!(begun["already"], false);
    let open_now = maintenance(&client).unwrap();
    assert_eq!(open_now.phase, Phase::Open);
    assert_eq!(open_now.opened_by.as_deref(), Some(FIRST_RUN));
    assert!(pending(&client).is_empty(), "cleared once answered");
    assert!(keeper.runner.window_open());
    let grants = person.asked();
    assert_eq!(grants.len(), 1);
    assert_eq!(grants[0].kind, Kind::Maintain);
    assert!(!grants[0].end && !grants[0].restart);
    assert_eq!(grants[0].name, "work");
    assert!(client.maintenance_held().is_some(), "agents wait");
    assert!(!begun.to_string().contains("helper"), "{begun}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn maintenance_is_refused_without_the_persons_grant_and_asks_atlas_vm_nothing() {
    let dir = tempdir();
    let (client, keeper) = quiet_vm(dir.path());
    let person = Person::default();

    let refused = asked(&client, BEGIN, person.refusing()).await.unwrap_err();

    assert_eq!(refused.code, Code::Denied, "{refused:?}");
    assert_eq!(person.asked().len(), 1);
    assert!(maintenance(&client).is_none());
    assert!(keeper.asked_for("hold").is_empty());
    assert!(keeper.runner.asked_for("window").is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn maintenance_is_refused_while_work_an_agent_started_still_runs_and_nothing_is_kept() {
    let dir = tempdir();
    let (client, keeper) = quiet_vm(dir.path());
    *keeper.leftovers.lock().unwrap() = "running";
    let person = Person::default();

    let refused = asked(&client, BEGIN, person.allowing()).await.unwrap_err();

    assert_eq!(refused.code, Code::Busy, "{refused:?}");
    assert_eq!(
        refused.error,
        "Work an agent started still runs on work, so its window cannot take your input yet. \
         Wait for it to end, or add --restart to stop work and start it again for \
         maintenance; that work ends."
    );
    assert!(person.asked().is_empty(), "the person is not asked");
    assert!(maintenance(&client).is_none());
    assert!(keeper.asked_for("hold").is_empty());
    assert!(keeper.runner.asked_for("window").is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn maintenance_is_refused_while_the_guest_cannot_say_whether_work_runs_with_advice_to_restart()
 {
    let dir = tempdir();
    let (client, keeper) = quiet_vm(dir.path());
    *keeper.leftovers.lock().unwrap() = "unknown";
    let person = Person::default();

    let refused = asked(&client, BEGIN, person.allowing()).await.unwrap_err();

    assert_eq!(refused.code, Code::NotReady, "{refused:?}");
    assert!(
        refused
            .error
            .starts_with("work's guest cannot say whether agents' work still runs there."),
        "{refused:?}"
    );
    assert!(refused.error.contains("add --restart"), "{refused:?}");
    assert!(person.asked().is_empty());
    assert!(maintenance(&client).is_none());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn maintenance_is_refused_beside_a_change_whose_outcome_is_unknown() {
    let dir = tempdir();
    let (client, keeper) = quiet_vm(dir.path());
    let unknown = Token::new();
    drop(
        client
            .controller
            .quarantine_by(unknown.as_str(), Executor::Vm)
            .unwrap(),
    );
    let person = Person::default();

    let refused = asked(&client, BEGIN, person.allowing()).await.unwrap_err();

    assert!(
        refused.details["next"]
            .as_str()
            .is_some_and(|next| next.contains("add --restart")),
        "{refused:?}"
    );
    assert!(person.asked().is_empty());
    assert!(maintenance(&client).is_none());
    assert!(keeper.asked_for("hold").is_empty());
    assert!(keeper.runner.asked_for("window").is_empty());
    assert_eq!(
        pending(&client),
        vec![unknown.to_string()],
        "left as it was"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn only_a_vm_desktop_is_opened_for_maintenance() {
    let dir = tempdir();
    let client = crate::test_support::client(dir.path());
    let person = Person::default();

    let refused = asked(&client, BEGIN, person.allowing()).await.unwrap_err();

    assert_eq!(refused.code, Code::Usage, "{refused:?}");
    assert!(person.asked().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_window_atlas_vm_refused_to_open_ends_the_maintenance_under_the_same_exclusion() {
    let dir = tempdir();
    let (client, keeper) = quiet_vm(dir.path());
    keeper
        .runner
        .refusing_window(Some("No window can show now."));
    let person = Person::default();

    let refused = asked(&client, BEGIN, person.allowing()).await.unwrap_err();

    assert_eq!(refused.code, Code::State, "{refused:?}");
    assert_eq!(
        refused.error,
        "Atlas VM did not open work's window: No window can show now."
    );
    assert!(maintenance(&client).is_none(), "forgotten");
    assert!(
        windows_sent(&client).is_empty(),
        "with what was sent for it"
    );
    assert!(pending(&client).is_empty());
    assert_eq!(keeper.asked_for("release").len(), 1, "agents let in again");
    assert!(client.maintenance_held().is_none());
    assert!(!keeper.runner.window_open());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_window_on_whose_reply_is_lost_is_followed_by_its_outcome_and_never_asked_twice() {
    let dir = tempdir();
    let (client, keeper) = quiet_vm(dir.path());
    keeper.runner.dropping_window(1);
    let person = Person::default();

    let begun = asked(&client, BEGIN, person.allowing()).await.unwrap();

    assert_eq!(begun["maintenance"], "on", "{begun}");
    assert_eq!(keeper.runner.asked_for("window").len(), 1);
    assert_eq!(keeper.runner.begun().len(), 1);
    assert!(!keeper.runner.asked_for("outcome").is_empty(), "followed");
    let open = maintenance(&client).unwrap();
    assert_eq!(open.phase, Phase::Open);
    assert_eq!(open.opened_by.as_deref(), Some(FIRST_RUN));
    assert!(pending(&client).is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_window_on_atlas_vm_does_not_answer_stays_pending_with_its_claim_and_recovery_leaves_it()
{
    let dir = tempdir();
    let (client, keeper) = quiet_vm(dir.path());
    keeper.runner.silent_at_window();
    let person = Person::default();

    let lost = asked_within(
        &client,
        BEGIN,
        Duration::from_millis(300),
        person.allowing(),
    )
    .await
    .unwrap_err();
    let recovered = client.recover_helper().await.unwrap_err();

    assert_eq!(lost.code, Code::NotReady, "{lost:?}");
    assert_eq!(
        lost.error,
        "Atlas VM has not said whether it opened work's window; agents wait. Run atlas desktop \
         maintain work again."
    );
    let opening = maintenance(&client).unwrap();
    assert_eq!(opening.phase, Phase::Opening);
    let open = opening.open.unwrap();
    assert_eq!(windows_sent(&client), vec![open.clone()]);
    assert_eq!(
        pending(&client),
        vec![open.to_string()],
        "claimed until answered"
    );
    assert_eq!(recovered.code, Code::Busy, "{recovered:?}");
    assert_eq!(pending(&client), vec![open.to_string()], "left by recovery");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_second_maintain_while_one_is_on_is_answered_from_its_record_and_opens_no_second_window()
{
    let dir = tempdir();
    let (client, keeper) = quiet_vm(dir.path());
    let person = Person::default();

    let first = asked(&client, BEGIN, person.allowing()).await.unwrap();
    let again = asked(&client, BEGIN, person.allowing()).await.unwrap();

    assert_eq!(again["maintenance"], "on", "{again}");
    assert_eq!(again["already"], true);
    assert_eq!(again["change"], first["change"]);
    assert_eq!(keeper.runner.asked_for("window").len(), 1);
    assert_eq!(person.asked().len(), 1, "nothing more to allow");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_exclusion_is_held_from_the_first_look_through_the_last_cleanup() {
    let dir = tempdir();
    let (client, keeper) = quiet_vm(dir.path());
    keeper.runner.taking(Duration::from_millis(800));
    let person = Person::default();

    let beginning = tokio::spawn({
        let (client, allowing) = (client.clone(), person.allowing());
        async move { asked(&client, BEGIN, allowing).await }
    });
    until("the window asked", || {
        !keeper.runner.asked_for("window").is_empty()
    })
    .await;
    let other = client
        .controller
        .clone()
        .waiting(Duration::ZERO)
        .acquire_for_executor()
        .err();
    let acquired = client
        .controller
        .clone()
        .waiting(Duration::ZERO)
        .acquire(false)
        .err();
    beginning.await.unwrap().unwrap();

    assert_eq!(other.map(|refused| refused.code), Some(Code::Busy));
    assert_eq!(acquired.map(|refused| refused.code), Some(Code::Busy));
    assert!(
        client
            .controller
            .clone()
            .waiting(Duration::ZERO)
            .acquire_for_executor()
            .is_ok(),
        "let go once done"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_keeper_that_does_not_say_every_agent_stopped_leaves_the_maintenance_holding_with_no_window()
 {
    let dir = tempdir();
    let (client, keeper) = quiet_vm(dir.path());
    *keeper.holds.lock().unwrap() = false;
    let person = Person::default();

    let held = asked(&client, BEGIN, person.allowing()).await.unwrap_err();
    let holding = maintenance(&client).unwrap();
    *keeper.holds.lock().unwrap() = true;
    let begun = asked(&client, BEGIN, person.allowing()).await.unwrap();

    assert_eq!(held.code, Code::NotReady, "{held:?}");
    assert_eq!(
        held.error,
        "Agents on work are held, but its keeper has not said their work there stopped, so \
         its window is not open yet. Run atlas desktop maintain work again."
    );
    assert_eq!(holding.phase, Phase::Holding);
    assert_eq!(holding.open, None, "no window asked");
    assert_eq!(keeper.asked_for("hold").len(), 2);
    assert_eq!(keeper.runner.asked_for("window").len(), 1, "only once held");
    assert_eq!(
        begun["change"],
        json!(holding.change),
        "the same maintenance"
    );
    assert_eq!(maintenance(&client).unwrap().phase, Phase::Open);
}

/// Where a begin or an end was cut short.
#[derive(Clone, Copy, Debug)]
pub(super) enum Boundary {
    /// Its window change kept, not yet claimed.
    Kept,
    /// Claimed as Atlas VM's, not yet sent.
    Claimed,
    /// Kept as sent, and never read by Atlas VM.
    Sent,
    /// Done by Atlas VM, its reply lost.
    Answered,
}

pub(super) const BOUNDARIES: [Boundary; 4] = [
    Boundary::Kept,
    Boundary::Claimed,
    Boundary::Sent,
    Boundary::Answered,
];

/// Leave `client`'s VM's maintenance at `phase` as one cut short at
/// `boundary` in its window change, `input` on or off, left it: that
/// change.
pub(super) fn cut_short(client: &Client, boundary: Boundary, phase: Phase, input: bool) -> Token {
    let lifecycle = lifecycle(client);
    let id = client.desktop().id();
    let mut record = record(client);
    let change = Token::new();
    let maintenance = record.maintenance.as_mut().unwrap();
    maintenance.phase = phase;
    if input {
        maintenance.open = Some(change.clone());
    } else {
        // A window-off is kept only once its window-on is settled.
        maintenance.settled = true;
        maintenance.close = Some(change.clone());
    }
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
    let window = Operation::Window {
        change: change.clone(),
        input,
    };
    match boundary {
        Boundary::Kept | Boundary::Claimed => {}
        Boundary::Sent => {
            record.sending(&window).unwrap();
            lifecycle.keep_record(&record).unwrap();
        }
        Boundary::Answered => {
            let runner = Runner::at(socket_in(lifecycle.home(), &id));
            let reply = runner
                .change(&mut record, |record| lifecycle.keep_record(record), &window)
                .unwrap();
            assert_eq!(reply.outcome, Some(Outcome::Done), "{reply:?}");
        }
    }
    change
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_begin_cut_short_at_each_boundary_goes_on_under_the_same_change_and_opens_the_window_once()
 {
    for boundary in BOUNDARIES {
        let dir = tempdir();
        let (client, keeper) = quiet_vm(dir.path());
        let id = client.desktop().id();
        under_maintenance(dir.path(), &id, Phase::Holding);
        let open = cut_short(&client, boundary, Phase::Opening, true);
        let person = Person::default();

        let begun = asked(&client, BEGIN, person.allowing()).await;

        let begun = begun.unwrap_or_else(|error| panic!("{boundary:?}: {error:?}"));
        assert_eq!(begun["maintenance"], "on", "{boundary:?}: {begun}");
        let kept = maintenance(&client).unwrap();
        assert_eq!(kept.phase, Phase::Open, "{boundary:?}");
        assert_eq!(kept.open, Some(open.clone()), "{boundary:?}");
        assert_eq!(kept.opened_by.as_deref(), Some(FIRST_RUN), "{boundary:?}");
        assert_eq!(
            keeper.runner.begun(),
            vec![open.to_string()],
            "{boundary:?}: done once"
        );
        assert!(
            keeper
                .runner
                .asked_for("window")
                .iter()
                .all(|asked| asked["change"] == json!(open)),
            "{boundary:?}: never under another change"
        );
        assert!(pending(&client).is_empty(), "{boundary:?}");
        assert_eq!(windows_sent(&client), vec![open], "{boundary:?}");
        assert!(keeper.runner.window_open(), "{boundary:?}");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_end_cut_short_at_each_boundary_goes_on_under_the_same_change_with_no_new_grant() {
    for boundary in BOUNDARIES {
        let dir = tempdir();
        let (client, keeper) = quiet_vm(dir.path());
        let person = Person::default();
        asked(&client, BEGIN, person.allowing()).await.unwrap();
        let close = cut_short(&client, boundary, Phase::Closing, false);

        let ended = asked(&client, END, person.refusing()).await;

        let ended = ended.unwrap_or_else(|error| panic!("{boundary:?}: {error:?}"));
        assert_eq!(ended["maintenance"], "none", "{boundary:?}: {ended}");
        assert_eq!(person.asked().len(), 1, "{boundary:?}: the begin's alone");
        assert!(maintenance(&client).is_none(), "{boundary:?}");
        assert_eq!(
            keeper.runner.begun().last(),
            Some(&close.to_string()),
            "{boundary:?}"
        );
        assert_eq!(keeper.runner.begun().len(), 2, "{boundary:?}: done once");
        assert!(pending(&client).is_empty(), "{boundary:?}");
        assert!(windows_sent(&client).is_empty(), "{boundary:?}");
        assert!(!keeper.runner.window_open(), "{boundary:?}");
        assert!(client.maintenance_held().is_none(), "{boundary:?}");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ending_maintenance_turns_input_off_and_ends_only_on_the_answer_of_the_run_that_opened_it()
{
    let dir = tempdir();
    let (client, keeper) = quiet_vm(dir.path());
    let person = Person::default();
    let begun = asked(&client, BEGIN, person.allowing()).await.unwrap();

    let ended = asked(&client, END, person.allowing()).await.unwrap();

    assert_eq!(ended["maintenance"], "none", "{ended}");
    assert_eq!(ended["input"], "off");
    assert_eq!(ended["change"], begun["change"]);
    let grants = person.asked();
    assert_eq!(grants.len(), 2);
    assert!(grants[1].end, "the end is allowed apart");
    let windows = keeper.runner.asked_for("window");
    assert_eq!(windows.len(), 2);
    assert_eq!(windows[1]["input"], false);
    assert!(!keeper.runner.window_open());
    assert!(maintenance(&client).is_none());
    assert!(
        windows_sent(&client).is_empty(),
        "every window change forgotten"
    );
    assert!(pending(&client).is_empty());
    let released = keeper.asked_for("release");
    assert_eq!(released.len(), 1, "agents let in last");
    assert_eq!(released[0]["change"], begun["change"]);
    assert!(client.maintenance_held().is_none());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_end_whose_reply_is_lost_stays_pending_and_the_next_end_finishes_it_with_no_new_grant() {
    let dir = tempdir();
    let (client, keeper) = quiet_vm(dir.path());
    let person = Person::default();
    asked(&client, BEGIN, person.allowing()).await.unwrap();
    keeper.runner.silent_at_window();

    let lost = asked_within(&client, END, Duration::from_millis(300), person.allowing())
        .await
        .unwrap_err();
    let closing = maintenance(&client).unwrap();
    let held = client.maintenance_held();
    keeper.runner.silent(false);
    let ended = asked(&client, END, person.refusing()).await.unwrap();

    assert_eq!(lost.code, Code::NotReady, "{lost:?}");
    assert_eq!(
        lost.error,
        "Atlas VM, which runs work, is not answering, so its window may still take your input; \
         agents wait. Run atlas desktop maintain work --end again."
    );
    assert_eq!(closing.phase, Phase::Closing);
    let close = closing.close.unwrap();
    assert!(
        held.is_some_and(|held| held.error.starts_with("work's maintenance is ending")),
        "agents wait in the ending's words"
    );
    assert_eq!(ended["maintenance"], "none", "{ended}");
    assert_eq!(person.asked().len(), 2, "the begin and the first end");
    assert_eq!(keeper.runner.begun().last(), Some(&close.to_string()));
    assert!(pending(&client).is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_end_atlas_vm_refuses_keeps_the_vm_under_maintenance_in_its_words() {
    let dir = tempdir();
    let (client, keeper) = quiet_vm(dir.path());
    let person = Person::default();
    asked(&client, BEGIN, person.allowing()).await.unwrap();
    keeper
        .runner
        .refusing_window(Some("The window would not close."));

    let refused = asked(&client, END, person.allowing()).await.unwrap_err();
    let held = client.maintenance_held();
    keeper.runner.refusing_window(None);
    let ended = asked(&client, END, person.refusing()).await.unwrap();

    assert_eq!(refused.code, Code::State, "{refused:?}");
    assert_eq!(
        refused.error,
        "Atlas VM could not turn off work's window: The window would not close."
    );
    assert!(held.is_some(), "agents still wait");
    assert_eq!(ended["maintenance"], "none", "{ended}");
    let windows = keeper.runner.asked_for("window");
    assert_eq!(windows.len(), 3);
    assert_ne!(
        windows[1]["change"], windows[2]["change"],
        "asked again under a change of its own"
    );
    assert!(pending(&client).is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn closing_the_window_by_hand_ends_no_maintenance() {
    let dir = tempdir();
    let (client, keeper) = quiet_vm(dir.path());
    let person = Person::default();
    asked(&client, BEGIN, person.allowing()).await.unwrap();

    keeper.runner.close_by_hand();

    assert_eq!(maintenance(&client).unwrap().phase, Phase::Open);
    let held = client.maintenance_held().unwrap();
    assert!(
        held.error.starts_with("work is under maintenance"),
        "{held:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_end_with_no_maintenance_kept_still_lets_its_keeper_admit_agents_and_asks_nothing_more()
{
    let dir = tempdir();
    let (client, keeper) = quiet_vm(dir.path());
    let person = Person::default();

    let ended = asked(&client, END, person.allowing()).await.unwrap();

    assert_eq!(ended["maintenance"], "none", "{ended}");
    assert_eq!(ended["already"], true);
    assert!(person.asked().is_empty());
    assert_eq!(keeper.asked_for("release").len(), 1);
    assert!(keeper.runner.asked_for("window").is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_window_on_done_by_one_run_and_answered_by_the_next_is_kept_as_the_first_runs_and_the_next_runs_window_off_alone_ends_nothing()
 {
    let dir = tempdir();
    let (client, keeper) = quiet_vm(dir.path());
    let id = client.desktop().id();
    under_maintenance(dir.path(), &id, Phase::Holding);
    // The first run opened the window and went before its reply reached the
    // station; the next answers from here.
    let open = cut_short(&client, Boundary::Answered, Phase::Opening, true);
    keeper.runner.runs_again_as("the next run");
    let person = Person::default();

    let begun = asked(&client, BEGIN, person.allowing()).await.unwrap();
    let opened = maintenance(&client).unwrap();
    let refused = asked(&client, END, person.allowing()).await.unwrap_err();

    assert_eq!(begun["maintenance"], "on", "{begun}");
    assert_eq!(
        opened.opened_by.as_deref(),
        Some(FIRST_RUN),
        "the run that did it"
    );
    assert_eq!(keeper.runner.begun()[0], open.to_string());
    assert_eq!(refused.code, Code::NotReady, "{refused:?}");
    assert_eq!(
        refused.error,
        "Atlas VM ran again since it opened work's window, and whether that window is gone \
         cannot be told yet, so agents wait. Run atlas desktop maintain work --end again later."
    );
    let ending = maintenance(&client).unwrap();
    assert_eq!(ending.phase, Phase::Closing);
    assert!(ending.ended.is_none(), "not ended by the next run alone");
    assert!(client.maintenance_held().is_some());
    assert!(keeper.asked_for("release").is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_window_on_never_sent_needs_a_window_off_from_any_run_to_end() {
    let dir = tempdir();
    let (client, keeper) = quiet_vm(dir.path());
    let id = client.desktop().id();
    under_maintenance(dir.path(), &id, Phase::Holding);
    cut_short(&client, Boundary::Claimed, Phase::Opening, true);
    keeper.runner.runs_again_as("the next run");
    let person = Person::default();

    let ended = asked(&client, END, person.allowing()).await.unwrap();

    assert_eq!(ended["maintenance"], "none", "{ended}");
    let windows = keeper.runner.asked_for("window");
    assert_eq!(windows.len(), 1, "never the window-on");
    assert_eq!(windows[0]["input"], false);
    assert!(pending(&client).is_empty(), "its claim cleared with it");
}

/// Every line `say` was told, kept.
#[derive(Clone, Default)]
pub(super) struct Said(Arc<Mutex<Vec<String>>>);

impl Said {
    pub(super) fn saying(&self) -> impl Fn(&str) + Send + 'static {
        let said = self.0.clone();
        move |line| said.lock().unwrap().push(line.to_owned())
    }

    pub(super) fn lines(&self) -> Vec<String> {
        self.0.lock().unwrap().clone()
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn each_step_is_said_naming_the_vm_as_its_card_does() {
    let dir = tempdir();
    let (client, _keeper) = quiet_vm(dir.path());
    let person = Person::default();
    let (began, ending, going_on) = (Said::default(), Said::default(), Said::default());

    asked_saying(&client, BEGIN, WITHIN, person.allowing(), began.saying())
        .await
        .unwrap();
    cut_short(&client, Boundary::Kept, Phase::Closing, false);
    asked_saying(&client, END, WITHIN, person.allowing(), going_on.saying())
        .await
        .unwrap();
    asked_saying(
        &client,
        BEGIN,
        WITHIN,
        person.allowing(),
        Said::default().saying(),
    )
    .await
    .unwrap();
    asked_saying(&client, END, WITHIN, person.allowing(), ending.saying())
        .await
        .unwrap();

    assert_eq!(
        began.lines(),
        vec![
            "Asking you in Atlas.app to allow opening the window of the VM \u{201c}work\u{201d} \
             for maintenance.",
            "Holding every agent's work on work until you end it.",
            "Asking Atlas VM to open work's window.",
        ]
    );
    assert_eq!(
        going_on.lines()[0],
        "Going on with ending maintenance of the VM \u{201c}work\u{201d}."
    );
    assert_eq!(
        ending.lines(),
        vec![
            "Asking you in Atlas.app to allow ending maintenance of the VM \u{201c}work\u{201d}.",
            "Asking Atlas VM to turn off work's window.",
        ]
    );
    for line in [began.lines(), going_on.lines(), ending.lines()].concat() {
        assert!(!line.contains("helper"), "{line}");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_window_on_still_on_its_way_is_voided_before_the_window_off_and_never_opens_the_window_after_it()
 {
    let dir = tempdir();
    let (client, keeper) = quiet_vm(dir.path());
    let id = client.desktop().id();
    under_maintenance(dir.path(), &id, Phase::Holding);
    // Sent by a begin that went before Atlas VM read it.
    let open = cut_short(&client, Boundary::Sent, Phase::Opening, true);
    keeper.runner.silent_at_window();
    let person = Person::default();

    let lost = asked_within(&client, END, Duration::from_millis(300), person.allowing())
        .await
        .unwrap_err();
    let first = keeper.runner.asked_for("window");
    keeper.runner.silent(false);
    // The begin's window-on arrives at last, after the first window-off.
    let lifecycle = lifecycle(&client);
    let mut record = record(&client);
    let late = Runner::at(socket_in(lifecycle.home(), &id))
        .change(
            &mut record,
            |record| lifecycle.keep_record(record),
            &Operation::Window {
                change: open.clone(),
                input: true,
            },
        )
        .unwrap();
    let window_after_late = keeper.runner.window_open();
    let released_before = keeper.asked_for("release").len();
    let ended = asked(&client, END, person.refusing()).await.unwrap();

    assert_eq!(lost.code, Code::NotReady, "{lost:?}");
    assert_eq!(keeper.runner.asked_for("void").len(), 1, "voided first");
    assert_eq!(keeper.runner.asked_for("void")[0]["change"], json!(open));
    assert!(first.is_empty(), "the window-off was lost on its way");
    assert!(!late.ok, "the late window-on is never done: {late:?}");
    assert!(!window_after_late);
    assert_eq!(released_before, 0, "agents held while it was not known");
    assert_eq!(ended["maintenance"], "none", "{ended}");
    assert!(!keeper.runner.window_open());
    assert_eq!(keeper.asked_for("release").len(), 1);
    assert!(
        keeper
            .runner
            .begun()
            .iter()
            .all(|begun| *begun != open.to_string()),
        "the window-on was never begun"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_window_off_kept_before_the_window_on_was_settled_never_proves_the_end() {
    let dir = tempdir();
    let (client, keeper) = quiet_vm(dir.path());
    let id = client.desktop().id();
    under_maintenance(dir.path(), &id, Phase::Holding);
    cut_short(&client, Boundary::Sent, Phase::Opening, true);
    // A window-off an earlier end kept and had done, before anything
    // settled the window-on.
    let early = Token::new();
    let lifecycle = lifecycle(&client);
    let mut kept = record(&client);
    let maintenance = kept.maintenance.as_mut().unwrap();
    maintenance.phase = Phase::Closing;
    maintenance.close = Some(early.clone());
    lifecycle.keep_record(&kept).unwrap();
    Runner::at(socket_in(lifecycle.home(), &id))
        .change(
            &mut kept,
            |record| lifecycle.keep_record(record),
            &Operation::Window {
                change: early.clone(),
                input: false,
            },
        )
        .unwrap();
    let person = Person::default();

    let ended = asked(&client, END, person.refusing()).await.unwrap();

    assert_eq!(ended["maintenance"], "none", "{ended}");
    let windows = keeper.runner.asked_for("window");
    let last = windows.last().unwrap();
    assert_eq!(last["input"], false);
    assert_ne!(last["change"], json!(early), "a fresh window-off proves it");
    assert!(person.asked().is_empty(), "going on with its end");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_atlas_vm_that_cannot_void_a_change_keeps_the_maintenance_with_advice_to_update() {
    let dir = tempdir();
    let (client, keeper) = quiet_vm(dir.path());
    let person = Person::default();
    asked(&client, BEGIN, person.allowing()).await.unwrap();
    keeper.runner.older();

    let refused = asked(&client, END, person.allowing()).await.unwrap_err();

    assert_eq!(refused.code, Code::Update, "{refused:?}");
    assert!(refused.error.contains("Update Atlas"), "{refused:?}");
    assert_eq!(keeper.runner.asked_for("window").len(), 1, "no window-off");
    assert!(keeper.asked_for("release").is_empty());
    assert!(client.maintenance_held().is_some());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_end_going_on_without_a_grant_is_refused_once_another_maintenance_took_its_place() {
    let dir = tempdir();
    let (client, keeper) = quiet_vm(dir.path());
    let person = Person::default();
    asked(&client, BEGIN, person.allowing()).await.unwrap();
    cut_short(&client, Boundary::Kept, Phase::Closing, false);
    let lifecycle = lifecycle(&client).clone();
    let id = client.desktop().id();
    let replaced = Arc::new(Mutex::new(None));
    // Between this end's first look and its locks, the first maintenance
    // ends and the person begins another.
    let replacing = {
        let replaced = replaced.clone();
        move |line: &str| {
            if line.starts_with("Going on") {
                let mut record = lifecycle.record(&id).unwrap().unwrap();
                record.maintenance_settled();
                lifecycle.keep_record(&record).unwrap();
                let next = under_maintenance(lifecycle.home(), &id, Phase::Open);
                *replaced.lock().unwrap() = Some(next);
            }
        }
    };
    let windows_before = keeper.runner.asked_for("window").len();

    let refused = asked_saying(&client, END, WITHIN, person.refusing(), replacing)
        .await
        .unwrap_err();

    assert_eq!(refused.code, Code::Denied, "{refused:?}");
    assert!(
        refused.error.contains("needs your grant again"),
        "{refused:?}"
    );
    assert_eq!(person.asked().len(), 1, "the begin's alone");
    let next = replaced.lock().unwrap().clone().unwrap();
    let kept = maintenance(&client).unwrap();
    assert_eq!(kept.change, next, "the other maintenance is left as it is");
    assert_eq!(kept.phase, Phase::Open);
    assert_eq!(keeper.runner.asked_for("window").len(), windows_before);
    assert!(keeper.runner.asked_for("void").is_empty());
    assert!(keeper.asked_for("release").is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_end_whose_keeper_does_not_answer_its_release_stays_ended_and_the_next_end_releases() {
    let dir = tempdir();
    let (client, keeper) = quiet_vm(dir.path());
    let person = Person::default();
    asked(&client, BEGIN, person.allowing()).await.unwrap();
    // The keeper still holds agents, and no longer answers.
    *keeper.away.lock().unwrap() = true;

    let owed = asked(&client, END, person.allowing()).await.unwrap_err();
    let kept = maintenance(&client).unwrap();
    let held = client.maintenance_held();
    *keeper.away.lock().unwrap() = false;
    let ended = asked(&client, END, person.refusing()).await.unwrap();

    assert_eq!(owed.code, Code::NotReady, "{owed:?}");
    assert!(
        owed.error.contains("did not answer to let agents in again"),
        "{owed:?}"
    );
    assert!(kept.ended.is_some(), "kept ended, its release owed");
    assert!(held.is_some(), "agents wait meanwhile");
    assert_eq!(ended["maintenance"], "none", "{ended}");
    assert_eq!(person.asked().len(), 2, "no grant to go on");
    assert_eq!(
        keeper.runner.asked_for("window").len(),
        2,
        "turned off once"
    );
    assert_eq!(keeper.asked_for("release").len(), 2);
    assert!(maintenance(&client).is_none());
}

#[test]
fn a_keeper_is_absent_only_when_nothing_is_at_its_socket_whatever_its_hello_said() {
    let dir = tempdir();
    let client = crate::test_support::vm_client(dir.path(), None);
    let id = client.desktop().id();
    let socket = dir.path().join("keeper.sock");
    let listening = std::os::unix::net::UnixListener::bind(&socket).unwrap();
    // A link whose hello did not answer.
    let unreachable = Link::Unreachable(crate::link::Unreachable {
        selected: Arc::new(client.selected.clone()),
        timeout: ASKING,
        why: Some("work's session keeper did not answer".to_owned()),
    });

    let there = Reach::listening(&client.selected, &id, Ok(socket.clone()));
    drop(listening);
    std::fs::remove_file(&socket).unwrap();
    let gone = Reach::listening(&client.selected, &id, Ok(socket));
    let untold = Reach::listening(
        &client.selected,
        &id,
        Err(Error::new(Code::State, "No folder.")),
    );

    assert!(matches!(there, Reach::At(_)));
    assert!(matches!(gone, Reach::Absent));
    assert!(matches!(untold, Reach::Unknown));
    assert!(!matches!(
        Reach::of(&unreachable, &client.selected, &id),
        Reach::Unknown
    ));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_command_started_while_the_person_is_asked_refuses_the_begin_in_the_vms_words() {
    let dir = tempdir();
    let (client, keeper) = quiet_vm(dir.path());
    let running = Arc::new(Mutex::new(None));
    let starting = {
        let (controller, running) = (client.controller.clone(), running.clone());
        move |_: &grant::Operation| {
            *running.lock().unwrap() = Some(controller.quarantine_command().unwrap());
            Ok(Token::new())
        }
    };

    let refused = asked(&client, BEGIN, starting).await.unwrap_err();

    assert_eq!(refused.code, Code::Busy, "{refused:?}");
    assert_eq!(
        refused.error,
        "A command an agent started is still running on work. Wait for it to end."
    );
    assert_eq!(refused.details["running"], "command", "its details kept");
    assert_eq!(refused.holder, Some(atlas_core::error::Holder::Agent));
    assert!(
        refused.details["next"]
            .as_str()
            .is_some_and(|next| next.contains("atlas desktop maintain work")),
        "{refused:?}"
    );
    assert!(!serde_json::to_string(&refused).unwrap().contains("helper"));
    assert!(maintenance(&client).is_none());
    assert!(keeper.asked_for("hold").is_empty());
    drop(running.lock().unwrap().take());
}

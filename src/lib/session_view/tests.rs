use super::*;
use crate::host_session::{Host, Instance};

/// Now, in nanoseconds since 1970: a fixed moment the rows are dated from.
const NOW: u128 = 1_793_170_000_000_000_000;
const SECOND: u128 = 1_000_000_000;

fn key(id: &str) -> Key {
    Key::of(Host::Claude, id)
}

/// An agent's row of `session`, `ago` seconds before [`NOW`].
fn row(session: Option<&Key>, ago: u128, fields: Value) -> Row {
    let mut row = json!({"at": (NOW - ago * SECOND).to_string(), "operation": "input",
        "actor": "agent", "verb": "click", "app": "Opie", "ok": true});
    if let Some(session) = session {
        row["session"] = json!(session);
    }
    for (name, value) in fields.as_object().unwrap() {
        row[name] = value.clone();
    }
    Row::parse(&row).unwrap()
}

/// A note of this process for `session`, begun `ago` seconds before [`NOW`].
fn note(session: Option<&Key>, ago: u64) -> Note {
    Note {
        session: session.cloned(),
        host: session.map(|_| Host::Claude),
        folder: None,
        named: None,
        process: Instance::current().unwrap(),
        since: u64::try_from(NOW / 1_000_000).unwrap() - ago * 1000,
    }
}

fn cause(session: Option<&Key>) -> Cause {
    Cause {
        generation: "1".into(),
        running: false,
        command: false,
        session: session.cloned(),
    }
}

/// The scene of `rows`, with no note, cause or unknown outcome.
fn scene(rows: &[Row]) -> Scene<'_> {
    Scene {
        rows,
        processes: processes(),
        holder: None,
        waiting: &[],
        running: &[],
        causes: &[],
        pending: false,
        agent_holds: false,
        now: NOW,
    }
}

fn processes() -> &'static Snapshot {
    static SNAPSHOT: std::sync::OnceLock<Snapshot> = std::sync::OnceLock::new();
    SNAPSHOT.get_or_init(|| {
        Snapshot::sample([Instance::current().unwrap()], |_| {
            crate::process::State::Running
        })
    })
}

#[test]
fn a_session_holding_the_helper_acts_and_one_with_a_waiting_note_waits() {
    let (left, right, quiet) = (key("left"), key("right"), key("quiet"));
    let rows = [
        row(Some(&left), 1, json!({})),
        row(Some(&right), 20, json!({"verb": "open", "app": "Notes"})),
        row(Some(&quiet), 300, json!({})),
    ];
    let holder = note(Some(&left), 1);
    let waiting = [note(Some(&right), 8)];
    let scene = Scene {
        holder: Some(&holder),
        waiting: &waiting,
        ..scene(&rows)
    };

    let acting = scene.shown(&Who::Host(left), false);
    assert_eq!(acting.state, SessionState::Acting);
    assert!(acting.holds_helper && acting.live);
    assert_eq!(acting.latest.unwrap()["says"], "clicked in Opie");

    let waits = scene.shown(&Who::Host(right), false);
    assert_eq!(waits.state, SessionState::Waiting);
    assert_eq!(waits.waiting_for, Some(WaitingFor::Helper));
    assert_eq!(waits.waiting_seconds, Some(8));
    assert!(!waits.holds_helper);
    assert_eq!(waits.latest.unwrap()["says"], "opened Notes");

    let idle = scene.shown(&Who::Host(quiet), false);
    assert_eq!(idle.state, SessionState::Idle);
    assert_eq!(idle.waiting_for, None);
    assert_eq!(idle.latest.unwrap()["ageSeconds"], 300);
}

#[test]
fn a_running_command_is_its_sessions_work_while_another_holds_the_helper() {
    let (left, right) = (key("left"), key("right"));
    let rows = [
        row(Some(&right), 1, json!({"verb": "type", "app": "TextEdit"})),
        row(
            Some(&left),
            90,
            json!({"operation": "run", "verb": "run", "started": true, "ok": null}),
        ),
    ];
    let holder = note(Some(&right), 1);
    let causes = [Cause {
        running: true,
        command: true,
        ..cause(Some(&left))
    }];
    let busy = Scene {
        holder: Some(&holder),
        causes: &causes,
        ..scene(&rows)
    };

    let running = busy.shown(&Who::Host(left.clone()), false);
    assert_eq!(running.state, SessionState::Acting);
    assert!(!running.holds_helper, "the lock is the other session's");
    assert_eq!(running.latest.unwrap()["says"], "running a command…");

    let typing = busy.shown(&Who::Host(right), false);
    assert_eq!(typing.state, SessionState::Acting);
    assert!(typing.holds_helper);

    let unmarked = [Cause {
        running: true,
        ..cause(Some(&left))
    }];
    let quiet = Scene {
        causes: &unmarked,
        ..scene(&rows)
    };
    assert_eq!(
        quiet.shown(&Who::Host(left), false).state,
        SessionState::Idle,
        "only a marked command is work of its own"
    );
}

#[test]
fn a_holder_note_whose_command_ended_and_an_unfinished_row_hold_nothing() {
    let (left, right) = (key("left"), key("right"));
    let rows = [
        row(Some(&right), 2, json!({"ok": null, "started": true})),
        row(Some(&left), 3, json!({})),
    ];
    let mut gone = note(Some(&left), 3);
    gone.process = Instance::new(i32::MAX, [1, 0]);
    let scene = Scene {
        holder: Some(&gone),
        ..scene(&rows)
    };

    assert_eq!(
        scene.shown(&Who::Host(left), false).state,
        SessionState::Idle
    );
    assert_eq!(
        scene.shown(&Who::Host(right), false).state,
        SessionState::Idle
    );

    let later = Scene {
        now: NOW + 10 * SECOND,
        ..scene
    };
    assert_eq!(
        later.shown(&Who::Host(key("right")), false).state,
        SessionState::Idle,
        "a row begun long ago stands for nobody"
    );
}

#[test]
fn the_session_whose_action_is_unknown_needs_the_person_and_the_rest_wait_for_recovery() {
    let (left, right, quiet) = (key("left"), key("right"), key("quiet"));
    let rows = [
        row(Some(&right), 5, json!({})),
        row(
            Some(&left),
            10,
            json!({"verb": "type", "ok": false, "code": "uncertain"}),
        ),
        row(Some(&left), 20, json!({})),
        row(Some(&quiet), 600, json!({})),
    ];
    let causes = [cause(Some(&left))];
    let scene = Scene {
        causes: &causes,
        pending: true,
        ..scene(&rows)
    };

    let causer = scene.shown(&Who::Host(left), false);
    assert_eq!(causer.state, SessionState::NeedsYou);
    assert_eq!(causer.uncertain_row.as_deref(), Some(rows[1].id.as_str()));
    assert_eq!(causer.uncertain_verb.as_deref(), Some("type"));

    let other = scene.shown(&Who::Host(right), false);
    assert_eq!(other.state, SessionState::Waiting);
    assert_eq!(other.waiting_for, Some(WaitingFor::Recovery));
    assert_eq!(other.waiting_seconds, None);
    assert_eq!(other.uncertain_row, None);

    assert_eq!(
        scene.shown(&Who::Host(quiet), false).state,
        SessionState::Idle,
        "one that did nothing lately is idle"
    );

    let settled = Scene {
        pending: false,
        ..scene
    };
    assert_eq!(
        settled.shown(&Who::Host(key("left")), false).state,
        SessionState::Idle,
        "a cause the marker no longer counts blames nobody"
    );
}

#[test]
fn a_command_killed_without_a_row_blames_no_session() {
    let (left, right) = (key("left"), key("right"));
    let rows = [
        row(Some(&left), 5, json!({})),
        row(Some(&right), 6, json!({})),
    ];
    let causes = [cause(None)];
    let scene = Scene {
        causes: &causes,
        pending: true,
        ..scene(&rows)
    };

    for who in [Who::Host(left), Who::Host(right)] {
        let shown = scene.shown(&who, false);
        assert_eq!(shown.state, SessionState::Waiting);
        assert_eq!(shown.waiting_for, Some(WaitingFor::Recovery));
    }
    assert_eq!(scene.terminal(), None, "no Terminal row, so no Terminal");
}

#[test]
fn a_change_no_host_claimed_blames_nobody_even_after_a_kill_left_a_row_under_way() {
    let left = key("left");
    let rows = [
        row(
            None,
            5,
            json!({"ok": null, "started": true, "verb": "type"}),
        ),
        row(Some(&left), 6, json!({})),
    ];
    let causes = [cause(None)];
    let scene = Scene {
        causes: &causes,
        pending: true,
        ..scene(&rows)
    };

    for who in [Who::Terminal, Who::Host(left)] {
        let shown = scene.shown(&who, false);
        assert_eq!(shown.state, SessionState::Waiting, "{who:?}");
        assert_eq!(shown.waiting_for, Some(WaitingFor::Recovery));
        assert_eq!(shown.uncertain_row, None);
    }
}

#[test]
fn a_row_under_way_holds_nothing_while_an_outcome_is_unknown() {
    let left = key("left");
    let rows = [
        row(
            None,
            1,
            json!({"ok": null, "started": true, "verb": "type"}),
        ),
        row(Some(&left), 6, json!({})),
    ];
    let causes = [cause(None)];
    let mut killed = note(None, 1);
    killed.process = Instance::new(i32::MAX, [1, 0]);
    let scene = Scene {
        holder: Some(&killed),
        causes: &causes,
        pending: true,
        ..scene(&rows)
    };

    let terminal = scene.shown(&Who::Terminal, false);
    assert!(!terminal.holds_helper, "its killed command holds nothing");
    assert_eq!(terminal.state, SessionState::Waiting);
    assert_eq!(terminal.waiting_for, Some(WaitingFor::Recovery));
    assert_eq!(
        scene.shown(&Who::Host(left), false).waiting_for,
        Some(WaitingFor::Recovery)
    );
    let nothing = Scene {
        holder: None,
        ..scene
    };
    assert!(!nothing.shown(&Who::Terminal, false).holds_helper);

    // While an outcome is unknown, only a live note naming a session, or
    // the input service, says who holds the helper: a live note naming
    // none, as recovery's or the person's, leaves the row standing for
    // nobody.
    let running = note(None, 1);
    let held = Scene {
        holder: Some(&running),
        ..nothing
    };
    assert!(!held.shown(&Who::Terminal, false).holds_helper);
    let left_note = note(Some(&key("left")), 1);
    let left_holds = Scene {
        holder: Some(&left_note),
        ..nothing
    };
    assert!(
        left_holds
            .shown(&Who::Host(key("left")), false)
            .holds_helper
    );
    let service = Scene {
        agent_holds: true,
        ..nothing
    };
    assert!(!service.shown(&Who::Terminal, false).holds_helper);

    // A past start does not prove that its command still holds control.
    let settled = Scene {
        causes: &[],
        pending: false,
        ..nothing
    };
    assert_eq!(
        settled.shown(&Who::Terminal, false).state,
        SessionState::Idle
    );
}

#[test]
fn the_row_named_for_an_unknown_outcome_is_the_one_that_left_it() {
    let left = key("left");
    let rows = [
        row(
            Some(&left),
            2,
            json!({"verb": "click", "ok": false, "code": "uncertain"}),
        ),
        row(
            Some(&left),
            4,
            json!({"operation": "screenshot", "verb": "screenshot"}),
        ),
        row(
            Some(&left),
            6,
            json!({"verb": "type", "ok": null, "started": true}),
        ),
        row(Some(&left), 8, json!({"verb": "open", "app": "Notes"})),
        row(
            Some(&left),
            60,
            json!({"verb": "drag", "ok": null, "started": true}),
        ),
    ];
    let causes = [cause(Some(&left))];
    let scene = Scene {
        causes: &causes,
        pending: true,
        ..scene(&rows)
    };

    let shown = scene.shown(&Who::Host(left), false);
    assert_eq!(shown.state, SessionState::NeedsYou);
    assert_eq!(
        shown.uncertain_row.as_deref(),
        Some(rows[2].id.as_str()),
        "not the refusal after it, nor a cut short drag it went on from"
    );
    assert_eq!(shown.uncertain_verb.as_deref(), Some("type"));
}

#[test]
fn an_agent_no_host_named_holding_the_helper_is_terminal_acting() {
    let left = key("left");
    let rows = [row(Some(&left), 30, json!({}))];
    let scene = Scene {
        agent_holds: true,
        ..scene(&rows)
    };

    let terminal = scene.shown(&Who::Terminal, false);
    assert_eq!(terminal.state, SessionState::Acting);
    assert!(terminal.holds_helper);
    let now = u64::try_from(NOW / 1_000_000).unwrap();
    assert_eq!(scene.terminal(), Some((now, now)), "listed with no row yet");
    assert_eq!(
        scene.shown(&Who::Host(left.clone()), false).state,
        SessionState::Idle
    );

    let holder = note(Some(&left), 1);
    let named = Scene {
        holder: Some(&holder),
        ..scene
    };
    assert_eq!(
        named.shown(&Who::Host(left), false).state,
        SessionState::Acting,
        "a note naming a session wins"
    );
    assert_eq!(named.shown(&Who::Terminal, false).state, SessionState::Idle);
}

#[test]
fn rows_from_before_sessions_were_named_keep_no_terminal_live() {
    let left = key("left");
    let rows = [
        row(Some(&left), 60, json!({})),
        row(None, 120, json!({})),
        row(None, 600, json!({})),
    ];
    assert_eq!(scene(&rows).terminal(), None);

    let later = [
        row(None, 30, json!({})),
        row(Some(&left), 60, json!({})),
        row(None, 120, json!({})),
    ];
    let ms = |ago: u128| u64::try_from((NOW - ago * SECOND) / 1_000_000).unwrap();
    assert_eq!(scene(&later).terminal(), Some((ms(30), ms(30))));
}

#[test]
fn a_row_naming_no_key_is_nobodys() {
    let rows = [row(None, 5, json!({"session": "not a key"}))];
    assert_eq!(scene(&rows).terminal(), None);
    assert_eq!(scene(&rows).shown(&Who::Terminal, false).latest, None);
}

#[test]
fn rows_without_a_session_gather_under_terminal() {
    let left = key("left");
    let rows = [
        row(None, 30, json!({"verb": "open", "app": "Notes"})),
        row(
            None,
            40,
            json!({"actor": "viewer", "verb": "click", "app": "Notes"}),
        ),
        row(Some(&left), 50, json!({})),
        row(None, 120, json!({})),
        row(None, 2 * 60 * 60, json!({})),
    ];
    let listed = scene(&rows);

    let ms = |ago: u128| u64::try_from((NOW - ago * SECOND) / 1_000_000).unwrap();
    assert_eq!(
        listed.terminal(),
        Some((ms(30), ms(30))),
        "from its rows since one named a session"
    );
    let terminal = listed.shown(&Who::Terminal, false);
    assert_eq!(terminal.state, SessionState::Idle);
    assert_eq!(
        terminal.latest.unwrap()["says"],
        "opened Notes",
        "the person's own row is never Terminal's"
    );

    let old = [row(None, 2 * 60 * 60, json!({}))];
    assert_eq!(
        scene(&old).terminal(),
        None,
        "rows older than an hour list no Terminal"
    );
}

#[test]
fn an_ended_session_is_not_live_unless_it_still_matters_now() {
    let left = key("left");
    let rows = [row(Some(&left), 30, json!({}))];
    let ended = scene(&rows).shown(&Who::Host(left.clone()), true);
    assert_eq!(ended.state, SessionState::Ended);
    assert!(!ended.live);

    let causes = [cause(Some(&left))];
    let blamed = Scene {
        causes: &causes,
        pending: true,
        ..scene(&rows)
    };
    let shown = blamed.shown(&Who::Host(left), true);
    assert_eq!(
        shown.state,
        SessionState::NeedsYou,
        "its unknown action outlives it"
    );
    assert!(!shown.live);
    assert!(shown.relevant());
}

#[test]
fn sessions_show_only_while_two_are_live() {
    let (left, right) = (key("left"), key("right"));
    let rows = [
        row(Some(&left), 5, json!({})),
        row(Some(&right), 6, json!({})),
    ];
    let scene = scene(&rows);
    let one = scene.shown(&Who::Host(left), false);
    let two = scene.shown(&Who::Host(right.clone()), false);
    let gone = scene.shown(&Who::Host(right), true);

    assert!(!sessions_shown([&one]));
    assert!(!sessions_shown([&one, &gone]));
    assert!(sessions_shown([&one, &two]));
    assert!(!sessions_shown([]));
}

#[test]
fn states_order_as_the_apps_list_them() {
    let mut states = [
        SessionState::Ended,
        SessionState::Idle,
        SessionState::Waiting,
        SessionState::Acting,
        SessionState::NeedsYou,
    ];
    states.sort();
    assert_eq!(
        states,
        [
            SessionState::NeedsYou,
            SessionState::Acting,
            SessionState::Waiting,
            SessionState::Idle,
            SessionState::Ended
        ]
    );
    assert_eq!(json!(SessionState::NeedsYou), "needsYou");
    assert_eq!(json!(WaitingFor::Recovery), "recovery");
}

#[test]
fn pending_recovery_beats_an_uncorrelated_service_grant() {
    for age in [0, 1, 4, 6] {
        let rows = [row(None, age, json!({"started": true, "ok": null}))];
        for pending in [false, true] {
            let scene = Scene {
                pending,
                agent_holds: pending,
                ..scene(&rows)
            };
            let shown = scene.shown(&Who::Terminal, false);
            assert!(!shown.holds_helper);
            assert_eq!(
                shown.state,
                if pending {
                    SessionState::Waiting
                } else {
                    SessionState::Idle
                }
            );
        }
    }
}

#[test]
fn a_live_named_holder_can_coexist_with_anothers_unknown_outcome() {
    let (left, right) = (key("holder"), key("causer"));
    let holder = note(Some(&left), 1);
    let causes = [cause(Some(&right))];
    let scene = Scene {
        holder: Some(&holder),
        causes: &causes,
        pending: true,
        agent_holds: true,
        ..scene(&[])
    };
    assert!(scene.shown(&Who::Host(left), false).holds_helper);
    let ended = scene.shown(&Who::Host(right), true);
    assert!(!ended.live && !ended.holds_helper && ended.relevant());
    assert_eq!(ended.state, SessionState::NeedsYou);
}

#[test]
fn unknown_process_samples_leave_current_and_history_states_unended() {
    use crate::{host_session::HostSession, process::State};
    let instance = Instance::new(129, [1, 0]);
    let record = Record::new(
        &HostSession::new(Host::Codex, "unknown")
            .unwrap()
            .run_by(Some(instance)),
        u64::try_from(NOW / 1_000_000).unwrap(),
        None,
    );
    for processes in [
        Snapshot::sample([instance], |_| State::Unknown),
        Snapshot::default(),
        Snapshot::sample((1..=129).map(|pid| Instance::new(pid, [1, 0])), |_| {
            State::Running
        }),
    ] {
        assert_eq!(processes.state(instance), State::Unknown);
        let (shown, lifetime) = Scene {
            processes: &processes,
            ..scene(&[])
        }
        .project(&record);
        let history = HistorySource {
            records: std::slice::from_ref(&record),
            rows: &[],
            captures: &BTreeSet::new(),
            copies: &BTreeMap::new(),
            tasks: &[],
            now: record.last_seen,
        }
        .history_with(&processes, |_| false);
        assert_eq!(
            (shown.state, history[0].state),
            (SessionState::Idle, SessionState::Idle)
        );
        assert_eq!((lifetime.ended, history[0].ended_at), (None, None));
        assert!(!shown.live && !shown.relevant() && !lifetime.active);
    }
}

#[test]
fn only_a_known_end_projects_to_ended_history() {
    use crate::{host_session::HostSession, process::State};
    let now = u64::try_from(NOW / 1_000_000).unwrap();
    let instance = Instance::new(10, [1, 0]);
    let legacy = Record::new(&HostSession::new(Host::Named, "quiet").unwrap(), now, None);
    let mut explicit = legacy.clone();
    explicit.ended = Some(now);
    let gone = Record::new(
        &HostSession::new(Host::Codex, "gone")
            .unwrap()
            .run_by(Some(instance)),
        now,
        None,
    );
    let processes = Snapshot::sample([instance], |_| State::Gone);
    for (record, expected) in [
        (legacy, SessionState::Idle),
        (explicit, SessionState::Ended),
        (gone, SessionState::Ended),
    ] {
        let now = now + crate::time::ms(LIVE_FOR) + 1;
        let (shown, lifetime) = Scene {
            processes: &processes,
            now: u128::from(now) * 1_000_000,
            ..scene(&[])
        }
        .project(&record);
        let history = HistorySource {
            records: std::slice::from_ref(&record),
            rows: &[],
            captures: &BTreeSet::new(),
            copies: &BTreeMap::new(),
            tasks: &[],
            now,
        }
        .history_with(&processes, |_| false);
        assert_eq!((shown.state, history[0].state), (expected, expected));
        let ended = (expected == SessionState::Ended).then_some(record.last_seen);
        assert_eq!((lifetime.ended, history[0].ended_at), (ended, ended));
        assert!(!shown.live && !lifetime.active);
    }
}

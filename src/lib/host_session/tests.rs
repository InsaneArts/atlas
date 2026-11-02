use super::{recovery::owns_every_unknown, *};
use crate::control::Cause;

#[test]
fn a_session_owns_the_unknown_only_when_every_one_is_its_own_and_none_runs() {
    let (own, other) = (Key::of(Host::Pi, "own"), Key::of(Host::Pi, "other"));
    let cause = |session: Option<&Key>, running: bool| Cause {
        generation: crate::files::token(),
        running,
        command: false,
        session: session.cloned(),
    };

    assert!(!owns_every_unknown(&[], &own), "nothing is unknown");
    assert!(owns_every_unknown(&[cause(Some(&own), false)], &own));
    assert!(!owns_every_unknown(&[cause(Some(&own), true)], &own));
    assert!(!owns_every_unknown(
        &[cause(Some(&own), false), cause(Some(&other), false)],
        &own
    ));
    assert!(!owns_every_unknown(
        &[cause(Some(&own), false), cause(None, false)],
        &own
    ));
}

#[test]
fn a_running_command_is_nobodys_unknown_outcome_and_recovery_waits_for_it() {
    let (own, other) = (Key::of(Host::Pi, "own"), Key::of(Host::Pi, "other"));
    let unknown = Cause {
        generation: crate::files::token(),
        running: false,
        command: false,
        session: Some(own.clone()),
    };
    let command = Cause {
        generation: crate::files::token(),
        running: true,
        command: true,
        session: Some(other),
    };
    let of = Recovery::of;
    let beside = [unknown.clone(), command.clone()];

    assert!(owns_every_unknown(&beside, &own));
    assert_eq!(
        of(&beside, Asker::Agent(Some(&own))),
        Some(Recovery::OwnOnceCommandEnds)
    );
    assert_eq!(
        of(&beside, Asker::Person),
        Some(Recovery::PersonOnceCommandEnds)
    );
    assert_eq!(
        of(&[unknown], Asker::Agent(Some(&own))),
        Some(Recovery::Own)
    );
    assert_eq!(
        of(std::slice::from_ref(&command), Asker::Person),
        None,
        "work under way"
    );
    assert!(!owns_every_unknown(&[command], &own), "nothing of its own");
    for step in [
        Recovery::OwnOnceCommandEnds.step(),
        Recovery::PersonOnceCommandEnds.step(),
    ] {
        assert!(step.contains("once the running command ends"), "{step}");
    }
    assert!(Recovery::OwnOnceCommandEnds.own());
}

#[test]
fn the_guide_restarts_after_a_stopped_command_only_when_recovery_says_so() {
    let core = crate::agent::CORE
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    assert!(core.contains(
        "If your host stopped a command of yours, run `atlas doctor`; restart only when it says all is yours."
    ));

    let (own, other) = (Key::of(Host::Pi, "own"), Key::of(Host::Pi, "other"));
    let cause = |session: Option<&Key>| Cause {
        generation: crate::files::token(),
        running: false,
        command: false,
        session: session.cloned(),
    };
    let asker = Asker::Agent(Some(&own));
    assert_eq!(Recovery::of(&[], asker), None, "only waiting");
    for (causes, yours) in [
        (vec![cause(Some(&own))], true),
        (vec![cause(Some(&own)), cause(Some(&other))], false),
        (vec![cause(Some(&own)), cause(None)], false),
        (vec![cause(Some(&other))], false),
    ] {
        let recovery = Recovery::of(&causes, asker).unwrap();
        assert_eq!(recovery.own(), yours, "{recovery:?}");
        assert_eq!(
            recovery.step().contains("atlas restart"),
            yours,
            "{}",
            recovery.step()
        );
    }
}

#[test]
fn only_the_session_that_claimed_every_unknown_change_recovers_it_itself() {
    let (own, other) = (Key::of(Host::Pi, "own"), Key::of(Host::Pi, "other"));
    let cause = |session: Option<&Key>, running: bool| Cause {
        generation: crate::files::token(),
        running,
        command: false,
        session: session.cloned(),
    };
    let of = Recovery::of;

    assert_eq!(
        of(&[], Asker::Agent(Some(&own))),
        None,
        "nothing is pending"
    );
    assert_eq!(
        of(&[cause(Some(&own), true)], Asker::Person),
        None,
        "a change still being carried out is work in progress"
    );
    let mine = [cause(Some(&own), false)];
    assert_eq!(of(&mine, Asker::Agent(Some(&own))), Some(Recovery::Own));
    assert_eq!(
        of(&mine, Asker::Agent(Some(&other))),
        Some(Recovery::Others)
    );
    assert_eq!(of(&mine, Asker::Agent(None)), Some(Recovery::Others));
    assert_eq!(of(&mine, Asker::Person), Some(Recovery::Person));
    for shared in [
        [cause(Some(&own), false), cause(Some(&other), false)],
        [cause(Some(&own), false), cause(None, false)],
        [cause(Some(&own), false), cause(Some(&own), true)],
    ] {
        assert_eq!(
            of(&shared, Asker::Agent(Some(&own))),
            Some(Recovery::Others)
        );
    }

    assert!(Recovery::Own.step().contains("atlas restart"));
    assert!(Recovery::Person.step().contains("atlas restart"));
    let others = Recovery::Others.step();
    assert!(
        !others.contains("atlas "),
        "no command for another's action"
    );
    assert!(others.contains("tell the person"), "{others}");
}

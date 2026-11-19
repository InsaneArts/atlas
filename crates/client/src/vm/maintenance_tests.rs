use super::*;
use crate::vm::lifecycle::record::{Ended, Phase};
use atlas_core::files::Token;

fn maintenance(phase: Phase) -> Maintenance {
    Maintenance {
        change: Token::new(),
        phase,
        restart: false,
        since_ms: 1,
        stop: None,
        start: None,
        open: None,
        close: None,
        opened_by: None,
        settled: false,
        ended: None,
    }
}

fn vm() -> DesktopId {
    DesktopId::parse(crate::test_support::VM_ID).unwrap()
}

#[test]
fn a_record_with_no_maintenance_is_written_as_an_older_atlas_reads_it() {
    let home = tempfile::tempdir().unwrap();
    crate::test_support::under_maintenance(home.path(), &vm(), Phase::Open);
    let lifecycle = Lifecycle::under(home.path());
    let mut record = lifecycle.record(&vm()).unwrap().unwrap();
    record.maintenance = None;
    lifecycle.keep_record(&record).unwrap();

    let written: Value =
        serde_json::from_slice(&std::fs::read(lifecycle.record_at(&vm())).unwrap()).unwrap();

    assert!(written.get("maintenance").is_none(), "{written}");
    assert_eq!(Kept::read(Some(&lifecycle), &vm()), (Kept::None, None));
}

#[test]
fn a_kept_maintenance_is_read_with_the_vms_name_and_one_asked_off_or_ended_is_ending() {
    let home = tempfile::tempdir().unwrap();
    let lifecycle = Lifecycle::under(home.path());
    let change = crate::test_support::under_maintenance(home.path(), &vm(), Phase::Holding);

    let (kept, name) = Kept::read(Some(&lifecycle), &vm());

    assert_eq!(name.as_deref(), Some("work"));
    assert_eq!(kept.maintenance().map(|kept| &kept.change), Some(&change));
    assert!(matches!(kept, Kept::On(_)), "{kept:?}");
    for phase in [Phase::Holding, Phase::Opening, Phase::Open] {
        assert!(matches!(Kept::of(Some(maintenance(phase))), Kept::On(_)));
    }
    assert!(matches!(
        Kept::of(Some(maintenance(Phase::Closing))),
        Kept::Ending(_)
    ));
    let mut ended = maintenance(Phase::Open);
    ended.ended = Some(Ended {
        by: "run-1".to_owned(),
        close: Token::new(),
        earlier: None,
        earlier_gone: None,
    });
    assert!(matches!(Kept::of(Some(ended)), Kept::Ending(_)));
    assert_eq!(Kept::of(None), Kept::None);
}

#[test]
fn a_maintenance_stopping_or_starting_its_vm_holds_agents_and_a_record_without_either_reads_as_before()
 {
    for phase in [Phase::Stopping, Phase::Starting] {
        let kept = Kept::of(Some(maintenance(phase)));
        assert!(matches!(kept, Kept::On(_)), "{kept:?}");
        assert!(kept.held("work").is_some());
        assert_eq!(kept.said("work")["state"], "on");
    }
    let mut ended = maintenance(Phase::Closing);
    ended.ended = Some(Ended {
        by: "run-1".to_owned(),
        close: Token::new(),
        earlier: None,
        earlier_gone: None,
    });

    let written = serde_json::to_value(&ended).unwrap();

    for absent in ["stop", "start"] {
        assert!(written.get(absent).is_none(), "{written}");
    }
    assert_eq!(
        written["ended"].as_object().unwrap().len(),
        2,
        "by and close alone: {written}"
    );
    let read: Maintenance = serde_json::from_value(written).unwrap();
    assert_eq!(read, ended);
}

#[test]
fn agents_are_held_in_the_vms_own_words_and_only_the_person_ends_it() {
    let on = Kept::On(maintenance(Phase::Open)).held("work").unwrap();
    let ending = Kept::Ending(maintenance(Phase::Closing))
        .held("work")
        .unwrap();
    let unreadable = Kept::Unreadable.held("work").unwrap();

    assert_eq!(on.code, Code::Busy);
    assert_eq!(on.holder, Some(Holder::Person));
    assert_eq!(
        on.error,
        "work is under maintenance: its window takes the person's input. Agents wait until the \
         person ends it."
    );
    assert_eq!(
        on.details["next"],
        "Ask the person; they end it with atlas desktop maintain work --end."
    );
    assert_eq!(ending.code, Code::Busy);
    assert_eq!(
        ending.details["next"],
        "Run atlas desktop maintain work --end again."
    );
    assert_eq!(unreadable.code, Code::State);
    assert_eq!(unreadable.holder, None);
    assert!(Kept::None.held("work").is_none());
    for said in [&on, &ending, &unreadable] {
        assert!(!said.error.contains("helper"), "{said:?}");
    }
}

#[test]
fn the_persons_resume_is_refused_in_words_that_say_what_ends_the_maintenance() {
    let on = Kept::On(maintenance(Phase::Open))
        .resume_refused("work")
        .unwrap();
    let ending = Kept::Ending(maintenance(Phase::Closing))
        .resume_refused("work")
        .unwrap();
    let unreadable = Kept::Unreadable.resume_refused("work").unwrap();

    assert_eq!(on.code, Code::Busy);
    assert!(
        on.error
            .contains("End it with atlas desktop maintain work --end, then resume.")
    );
    assert!(
        ending
            .error
            .contains("Run atlas desktop maintain work --end again, then resume.")
    );
    assert_eq!(unreadable.code, Code::State);
    assert!(Kept::None.resume_refused("work").is_none());
}

#[test]
fn a_status_says_the_maintenance_and_one_that_does_not_say_holds_agents() {
    let on = Kept::On(maintenance(Phase::Open)).said("work");
    let ending = Kept::Ending(maintenance(Phase::Closing)).said("work");
    let unknown = Kept::Unreadable.said("work");
    let none = Kept::None.said("work");

    assert_eq!(on["state"], "on");
    assert_eq!(on["phase"], "open");
    assert_eq!(ending["state"], "ending");
    assert_eq!(unknown["state"], "unknown");
    assert_eq!(none, json!({"state": "none"}));
    assert_eq!(holds("work", &none), None);
    assert_eq!(holds("work", &on).as_deref(), on["reason"].as_str());
    assert_eq!(holds("work", &ending).as_deref(), ending["reason"].as_str());
    assert!(holds("work", &Value::Null).is_some(), "not said");
}

#[test]
fn atlas_on_a_vm_is_not_updated_while_its_maintenance_is_kept_and_the_person_is_told_to_end_it_first()
 {
    let on = Kept::On(maintenance(Phase::Open))
        .update_refused("work")
        .unwrap();
    let ending = Kept::Ending(maintenance(Phase::Closing))
        .update_refused("work")
        .unwrap();
    let unreadable = Kept::Unreadable.update_refused("work").unwrap();

    assert_eq!(on.code, Code::Busy);
    assert_eq!(
        on.error,
        "work is under maintenance, so its Atlas is not updated."
    );
    assert_eq!(
        on.details["next"],
        "End it with atlas desktop maintain work --end, then run atlas update."
    );
    assert_eq!(ending.code, Code::Busy);
    assert_eq!(
        ending.details["next"],
        "Run atlas desktop maintain work --end again, then run atlas update."
    );
    assert_eq!(unreadable.code, Code::State);
    assert!(
        unreadable.details["next"]
            .as_str()
            .unwrap()
            .contains("end it with atlas desktop maintain work --end")
    );
    for refused in [&on, &ending, &unreadable] {
        assert!(!refused.error.to_lowercase().contains("helper"));
    }
    assert!(Kept::None.update_refused("work").is_none());
}

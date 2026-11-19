use super::*;
use serde_json::json;

fn target() -> Target {
    Target {
        build: "b2".repeat(32),
        version: "0.1.3".to_owned(),
    }
}

fn staged() -> Staged {
    Staged {
        path: "/Users/guest/Library/Application Support/Atlas/open/x/Atlas.app".to_owned(),
        digest: "ab".repeat(32),
    }
}

/// Every phase an update goes through, each naming changes of its own.
fn phases() -> Vec<Phase> {
    let (stage, update, remove) = (Token::new(), Token::new(), Token::new());
    let outcome = json!({"ok": true, "op": "update", "update": "released", "why": null});
    vec![
        Phase::Intended,
        Phase::Staging {
            stage: stage.clone(),
            digest: None,
        },
        Phase::Staged {
            stage: stage.clone(),
            staged: staged(),
        },
        Phase::Asked {
            stage: stage.clone(),
            staged: staged(),
            update: update.clone(),
        },
        Phase::Settled {
            stage: stage.clone(),
            update: Some(update.clone()),
            outcome: outcome.clone(),
        },
        Phase::Cleaning {
            stage: stage.clone(),
            update: Some(update.clone()),
            outcome: outcome.clone(),
            node: "1:100".to_owned(),
            remove: remove.clone(),
        },
        Phase::Done {
            issued: vec![stage, update, remove],
            outcome,
        },
    ]
}

#[test]
fn every_phase_is_kept_and_read_back_whole() {
    let folder = tempfile::tempdir().unwrap();
    assert_eq!(Record::kept(folder.path()).unwrap(), None);

    for phase in phases() {
        let record = Record {
            target: target(),
            phase,
            earlier: Vec::new(),
        };
        record.keep(folder.path()).unwrap();

        assert_eq!(Record::kept(folder.path()).unwrap(), Some(record));
    }
    Record::forget(folder.path()).unwrap();
    assert_eq!(Record::kept(folder.path()).unwrap(), None);
    Record::forget(folder.path()).expect("one already gone is forgotten");
}

#[test]
fn each_phase_names_every_change_asked_so_far_and_the_one_it_waits_on() {
    let phases = phases();
    let named: Vec<usize> = phases.iter().map(|phase| phase.issued().len()).collect();

    assert_eq!(named, [0, 1, 1, 2, 2, 3, 3]);
    let waits: Vec<bool> = phases.iter().map(|phase| phase.open().is_some()).collect();
    assert_eq!(waits, [false, true, false, true, false, true, false]);
    let Phase::Asked { update, .. } = &phases[3] else {
        unreachable!()
    };
    assert_eq!(phases[3].open(), Some(update));
    let settled: Vec<bool> = phases
        .iter()
        .map(|phase| phase.outcome().is_some())
        .collect();
    assert_eq!(settled, [false, false, false, false, true, true, true]);
}

#[test]
fn a_record_that_cannot_be_read_is_never_taken_for_none() {
    let folder = tempfile::tempdir().unwrap();
    std::fs::write(folder.path().join(RECORD), b"{not a record").unwrap();
    assert!(Record::kept(folder.path()).is_err());

    std::fs::remove_file(folder.path().join(RECORD)).unwrap();
    std::fs::create_dir(folder.path().join(RECORD)).unwrap();
    assert!(
        Record::kept(folder.path()).is_err(),
        "a folder in its place"
    );
}

#[test]
fn an_update_begun_in_place_of_another_keeps_every_copy_that_one_left_to_remove() {
    let phases = phases();
    let (settled, cleaning) = (phases[4].clone(), phases[5].clone());
    let earlier = Leftover {
        stage: Token::new(),
        removal: None,
    };
    let record = |phase: Phase| Record {
        target: target(),
        phase,
        earlier: vec![earlier.clone()],
    };

    let from_settled = record(settled.clone()).left();
    let from_cleaning = record(cleaning.clone()).left();

    assert_eq!(from_settled.len(), 2);
    assert_eq!(from_settled[0], earlier);
    assert_eq!(from_settled[1].stage, settled.issued()[0]);
    assert_eq!(from_settled[1].removal, None);
    let Phase::Cleaning { node, remove, .. } = &cleaning else {
        unreachable!()
    };
    assert_eq!(
        from_cleaning[1].removal,
        Some(Removal {
            node: node.clone(),
            remove: remove.clone(),
        })
    );
    assert!(record(phases[3].clone()).left() == vec![earlier.clone()]);
    let named = record(cleaning.clone()).issued();
    assert!(named.contains(&earlier.stage));
    assert!(named.contains(remove));
}

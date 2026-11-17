use super::*;
use crate::machine::{commitment::create, vault::Memory};
use serde_json::json;
use std::sync::Arc;

/// A guest's change records as pairing leaves them, in their own folder.
fn paired() -> (tempfile::TempDir, Arc<Memory>, Changes<Arc<Memory>>) {
    let folder = tempfile::tempdir().unwrap();
    let vault = Arc::new(Memory::default());
    create(&vault, STORE).unwrap();
    let changes = Changes::new(folder.path(), vault.clone());
    (folder, vault, changes)
}

fn run(argv: &str) -> Value {
    json!({"op": "run", "argv": [argv]})
}

#[test]
fn a_change_is_kept_before_it_runs_and_what_it_came_to_before_it_is_answered() {
    let (_folder, _vault, changes) = paired();
    let change = Token::new();

    assert_eq!(
        changes.ask(&change, "run", &run("true")).unwrap(),
        Asked::Begun
    );
    assert_eq!(changes.of(&change).unwrap(), Some(State::Intended));
    // Asked again before it is done: still unknown, never run twice.
    assert_eq!(
        changes.ask(&change, "run", &run("true")).unwrap(),
        Asked::Again(State::Intended)
    );

    let reply = json!({"ok": true, "exitCode": 0});
    changes.done(&change, &reply).unwrap();

    assert_eq!(
        changes.ask(&change, "run", &run("true")).unwrap(),
        Asked::Again(State::Done { reply })
    );
}

#[test]
fn a_change_asked_again_with_another_request_is_refused() {
    let (_folder, _vault, changes) = paired();
    let change = Token::new();
    changes.ask(&change, "run", &run("true")).unwrap();

    let refused = changes.ask(&change, "run", &run("false")).unwrap_err();

    assert_eq!(refused.code, Code::BadRequest);
}

#[test]
fn a_change_is_forgotten_only_once_the_station_settled_what_it_came_to() {
    let (_folder, _vault, changes) = paired();
    let (done, intended) = (Token::new(), Token::new());
    changes.ask(&done, "run", &run("true")).unwrap();
    changes.ask(&intended, "run", &run("true")).unwrap();
    changes.done(&done, &json!({"ok": true})).unwrap();
    assert_eq!(changes.unsettled().unwrap(), std::slice::from_ref(&done));

    // An unresolved change stays, settled or not.
    changes.settled(&[intended.clone(), done.clone()]).unwrap();
    changes.settled(std::slice::from_ref(&done)).unwrap();

    assert_eq!(changes.of(&done).unwrap(), None);
    assert_eq!(changes.of(&intended).unwrap(), Some(State::Intended));
    assert!(changes.unsettled().unwrap().is_empty());
}

#[test]
fn a_new_change_past_the_bound_is_refused_as_busy_and_none_is_forgotten() {
    let folder = tempfile::tempdir().unwrap();
    let vault = Arc::new(Memory::default());
    create(&vault, STORE).unwrap();
    let changes = Changes::bounded(folder.path(), vault, 3);
    let kept: Vec<Token> = (0..3).map(|_| Token::new()).collect();
    for change in &kept {
        changes.ask(change, "run", &run("true")).unwrap();
    }

    let refused = changes.ask(&Token::new(), "run", &run("true")).unwrap_err();

    assert_eq!(refused.code, Code::Busy);
    for change in &kept {
        assert_eq!(changes.of(change).unwrap(), Some(State::Intended));
    }
}

#[test]
fn a_store_deleted_rolled_back_or_forged_is_unknown_never_empty() {
    // Deleted whole.
    let (folder, _vault, changes) = paired();
    changes.ask(&Token::new(), "run", &run("true")).unwrap();
    for entry in std::fs::read_dir(folder.path()).unwrap() {
        std::fs::remove_file(entry.unwrap().path()).unwrap();
    }
    assert_eq!(changes.unsettled().unwrap_err().code, Code::Uncertain);
    assert!(changes.ask(&Token::new(), "run", &run("true")).is_err());

    // Rolled back to an earlier manifest.
    let (folder, _vault, changes) = paired();
    let change = Token::new();
    changes.ask(&change, "run", &run("true")).unwrap();
    let earlier = std::fs::read(folder.path().join("manifest.json")).unwrap();
    changes.done(&change, &json!({"ok": true})).unwrap();
    std::fs::write(folder.path().join("manifest.json"), earlier).unwrap();
    assert!(changes.of(&change).is_err());

    // A record forged, its manifest made to match.
    let (folder, _vault, changes) = paired();
    let change = Token::new();
    changes.ask(&change, "run", &run("true")).unwrap();
    let forged = Record {
        change: change.clone(),
        operation: "run".into(),
        request: bytes_sha256(&serde_json::to_vec(&run("true")).unwrap()),
        state: State::Done {
            reply: json!({"ok": true}),
        },
    };
    let bytes = serde_json::to_vec(&forged).unwrap();
    let digest = bytes_sha256(&bytes);
    std::fs::write(folder.path().join(format!("{change}.{digest}.json")), bytes).unwrap();
    let manifest = json!({"version": 1, "number": 2, "records": {change.as_str(): digest}});
    std::fs::write(folder.path().join("manifest.json"), manifest.to_string()).unwrap();
    assert!(changes.of(&change).is_err());
}

#[test]
fn a_write_cut_before_its_files_leaves_what_was_kept_before_it() {
    let (_folder, _vault, changes) = paired();
    let change = Token::new();
    changes.ask(&change, "run", &run("true")).unwrap();
    // Committed to a next revision whose files never came.
    changes
        .commitment
        .begin(Revision {
            number: 2,
            digest: "a".repeat(64),
        })
        .unwrap();

    assert_eq!(changes.of(&change).unwrap(), Some(State::Intended));
    changes.done(&change, &json!({"ok": true})).unwrap();
    assert!(matches!(
        changes.of(&change).unwrap(),
        Some(State::Done { .. })
    ));
}

#[test]
fn a_long_reply_keeps_what_it_came_to() {
    let (_folder, _vault, changes) = paired();
    let change = Token::new();
    changes.ask(&change, "run", &run("yes")).unwrap();
    let long = json!({"ok": true, "exitCode": 0, "output": "y".repeat(REPLY + 1)});

    changes.done(&change, &long).unwrap();

    let Some(State::Done { reply }) = changes.of(&change).unwrap() else {
        panic!("done");
    };
    assert_eq!(reply, json!({"ok": true, "exitCode": 0, "shortened": true}));
}

#[test]
fn a_record_cut_short_by_a_crash_is_written_again_never_taken_as_kept() {
    let (folder, _vault, changes) = paired();
    let change = Token::new();
    let record = Record {
        change: change.clone(),
        operation: "run".into(),
        request: bytes_sha256(&serde_json::to_vec(&run("true")).unwrap()),
        state: State::Intended,
    };
    let bytes = serde_json::to_vec(&record).unwrap();
    // Half of it, where the whole of it goes.
    let path = folder
        .path()
        .join(format!("{change}.{}.json", bytes_sha256(&bytes)));
    std::fs::write(&path, &bytes[..bytes.len() / 2]).unwrap();

    changes.ask(&change, "run", &run("true")).unwrap();

    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    assert_eq!(
        Changes::new(folder.path(), _vault).of(&change).unwrap(),
        Some(State::Intended)
    );
}

#[test]
fn files_no_revision_names_are_swept_at_the_next_write() {
    let (folder, _vault, changes) = paired();
    let stray = folder
        .path()
        .join(format!("{}.{}.json", Token::new(), "b".repeat(64)));
    std::fs::write(&stray, b"left by a write a crash cut short").unwrap();
    let change = Token::new();

    changes.ask(&change, "run", &run("true")).unwrap();
    changes.done(&change, &json!({"ok": true})).unwrap();
    changes.settled(std::slice::from_ref(&change)).unwrap();

    let left: Vec<_> = std::fs::read_dir(folder.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(left, ["manifest.json"]);
}

#[test]
fn a_shortened_reply_keeps_its_texts_short_too() {
    let (_folder, _vault, changes) = paired();
    let change = Token::new();
    changes.ask(&change, "run", &run("yes")).unwrap();
    let long = json!({"ok": false, "error": "e".repeat(REPLY + 1)});

    changes.done(&change, &long).unwrap();

    let Some(State::Done { reply }) = changes.of(&change).unwrap() else {
        panic!("done");
    };
    assert_eq!(reply["error"].as_str().unwrap().len(), SHORT);
    assert_eq!(reply["shortened"], true);
}

/// The one record file in `folder`.
fn only_record(folder: &std::path::Path) -> std::path::PathBuf {
    std::fs::read_dir(folder)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| path.file_name().is_some_and(|name| name != "manifest.json"))
        .unwrap()
}

#[test]
fn a_manifest_replaced_by_a_pipe_is_unknown_at_once() {
    let (folder, vault, changes) = paired();
    changes.ask(&Token::new(), "run", &run("true")).unwrap();
    let manifest = folder.path().join("manifest.json");
    std::fs::remove_file(&manifest).unwrap();
    nix::unistd::mkfifo(
        &manifest,
        nix::sys::stat::Mode::S_IRUSR | nix::sys::stat::Mode::S_IWUSR,
    )
    .unwrap();

    let started = std::time::Instant::now();
    let refused = Changes::new(folder.path(), vault).unsettled().unwrap_err();

    assert_eq!(refused.code, Code::Uncertain);
    assert!(started.elapsed() < std::time::Duration::from_secs(2));
}

#[test]
fn a_record_swapped_for_a_link_to_its_own_bytes_is_unknown() {
    let (folder, vault, changes) = paired();
    let change = Token::new();
    changes.ask(&change, "run", &run("true")).unwrap();
    let record = only_record(folder.path());
    let elsewhere = folder.path().join("elsewhere");
    std::fs::rename(&record, &elsewhere).unwrap();
    std::os::unix::fs::symlink(&elsewhere, &record).unwrap();

    assert!(Changes::new(folder.path(), vault).of(&change).is_err());
}

#[test]
fn a_record_changed_on_disk_never_changes_an_answer_and_is_caught_at_the_next_start() {
    let (folder, vault, changes) = paired();
    let change = Token::new();
    changes.ask(&change, "run", &run("true")).unwrap();
    std::fs::write(only_record(folder.path()), b"{}").unwrap();

    // This daemon answers from what it verified.
    assert_eq!(changes.of(&change).unwrap(), Some(State::Intended));
    // The next one checks again.
    assert!(Changes::new(folder.path(), vault).of(&change).is_err());
}

#[test]
fn a_change_too_large_to_keep_is_refused_before_anything_is_kept() {
    let (folder, vault, changes) = paired();
    let change = Token::new();
    let operation = "o".repeat(RECORD_LIMIT as usize);

    let refused = changes.ask(&change, &operation, &run("true")).unwrap_err();

    assert_eq!(refused.code, Code::BadRequest);
    assert_eq!(
        Changes::new(folder.path(), vault).of(&change).unwrap(),
        None
    );
}

#[test]
fn a_reply_too_deep_to_read_back_keeps_what_it_came_to_across_a_restart() {
    let (folder, vault, changes) = paired();
    let change = Token::new();
    changes.ask(&change, "run", &run("true")).unwrap();
    let mut deep = json!("bottom");
    for _ in 0..200 {
        deep = json!([deep]);
    }

    changes
        .done(&change, &json!({"ok": true, "exitCode": 0, "nested": deep}))
        .unwrap();

    let Some(State::Done { reply }) = Changes::new(folder.path(), vault).of(&change).unwrap()
    else {
        panic!("done, and readable after a restart");
    };
    assert_eq!(reply, json!({"ok": true, "exitCode": 0, "shortened": true}));
}

#[test]
fn only_changes_still_intended_are_listed_as_intended_with_their_operation() {
    let (_folder, _vault, changes) = paired();
    let (open, done) = (Token::new(), Token::new());
    changes
        .ask(&open, "stage", &json!({"op": "stage"}))
        .unwrap();
    changes.ask(&done, "run", &run("ls")).unwrap();
    changes.done(&done, &json!({"ok": true})).unwrap();

    let intended = changes.intended().unwrap();

    assert_eq!(
        intended,
        [Intended {
            change: open,
            operation: "stage".into(),
            request: request_digest(&json!({"op": "stage"})).unwrap(),
        }]
    );
}

#[test]
fn what_commands_printed_is_kept_within_one_bound_across_every_record_and_a_settled_one_frees_its_share()
 {
    let folder = tempfile::tempdir().unwrap();
    let vault = Arc::new(Memory::default());
    create(&vault, STORE).unwrap();
    // Room for two commands' output of 3 bytes each, as base64 says it.
    let changes = Changes::printing_at_most(folder.path(), vault, 6);
    let reply = json!({"ok": true, "exitCode": 0});
    let mut printed = reply.clone();
    printed["events"] = json!([{"event": "output", "data": "aGkK"}]);
    printed["truncated"] = json!(false);
    let commands: Vec<Token> = (0..4).map(|_| Token::new()).collect();
    let kept = |change: &Token| match changes.of(change).unwrap() {
        Some(State::Done { reply }) => reply,
        other => panic!("{other:?}"),
    };

    for change in &commands[..3] {
        changes.ask(change, "run", &run("echo")).unwrap();
        changes.done_printing(change, &reply, &printed).unwrap();
    }
    changes.settled(std::slice::from_ref(&commands[0])).unwrap();
    changes.ask(&commands[3], "run", &run("echo")).unwrap();
    changes
        .done_printing(&commands[3], &reply, &printed)
        .unwrap();

    assert_eq!(kept(&commands[1]), printed);
    assert_eq!(
        kept(&commands[2]),
        reply,
        "past the bound: none of its output"
    );
    assert_eq!(kept(&commands[3]), printed, "the settled one's share freed");
}

#[test]
fn only_an_intent_or_an_uncertain_reply_is_unresolved_and_kept_as_ended_once_its_work_is_proven_gone()
 {
    let (folder, vault, changes) = paired();
    let (intended, uncertain, certain, unkept) =
        (Token::new(), Token::new(), Token::new(), Token::new());
    for change in [&intended, &uncertain, &certain] {
        changes.ask(change, "run", &run("true")).unwrap();
    }
    let cut = json!({"ok": false, "code": "stopped", "error": "Stopped", "uncertain": true});
    changes.done(&uncertain, &cut).unwrap();
    changes
        .done(&certain, &json!({"ok": true, "exitCode": 0}))
        .unwrap();
    let unresolved = |changes: &Changes<Arc<Memory>>| -> Vec<Token> {
        changes
            .unresolved()
            .unwrap()
            .into_iter()
            .map(|intended| intended.change)
            .collect()
    };
    let mut expected = vec![intended.clone(), uncertain.clone()];
    expected.sort_by(|a, b| a.as_str().cmp(b.as_str()));
    assert_eq!(unresolved(&changes), expected);

    assert!(changes.ended(&intended).unwrap());
    assert!(changes.ended(&uncertain).unwrap());
    assert!(!changes.ended(&certain).unwrap(), "a certain reply stays");
    assert!(!changes.ended(&intended).unwrap(), "ended once");
    assert!(!changes.ended(&unkept).unwrap());

    let restarted = Changes::new(folder.path(), vault);
    for change in [&intended, &uncertain] {
        let Some(State::Done { reply }) = restarted.of(change).unwrap() else {
            panic!("kept as done");
        };
        assert_eq!(reply["ended"], true, "{reply}");
        assert_eq!(reply["uncertain"], true);
        assert_eq!(reply["ok"], false);
    }
    assert_eq!(
        restarted.of(&certain).unwrap(),
        Some(State::Done {
            reply: json!({"ok": true, "exitCode": 0})
        })
    );
    assert!(unresolved(&restarted).is_empty());
    assert_eq!(
        restarted.unsettled().unwrap().len(),
        3,
        "each named at a hello"
    );
}

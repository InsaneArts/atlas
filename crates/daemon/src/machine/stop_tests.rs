use super::*;
use crate::machine::{commitment::create, vault::Memory};
use std::sync::Arc;

/// A guest's Stop as pairing leaves it, in a folder of its own.
fn paired() -> (tempfile::TempDir, Arc<Memory>, Stop<Arc<Memory>>) {
    let folder = tempfile::tempdir().unwrap();
    let vault = Arc::new(Memory::default());
    create(&vault, STORE).unwrap();
    let stop = Stop::new(folder.path().join("stop.json"), vault.clone());
    (folder, vault, stop)
}

#[test]
fn a_guest_that_never_stopped_lets_agents_act() {
    let (_folder, _vault, stop) = paired();

    assert_eq!(stop.kept().unwrap(), Kept::Clear);
}

#[test]
fn a_stop_is_kept_across_restarts_with_its_first_reason_until_resumed() {
    let (folder, vault, stop) = paired();

    stop.stop(Why::Lapsed).unwrap();
    stop.stop(Why::Person).unwrap();
    let restarted = Stop::new(folder.path().join("stop.json"), vault);

    assert_eq!(
        restarted.kept().unwrap(),
        Kept::Stopped { why: Why::Lapsed }
    );
    restarted.resume().unwrap();
    assert_eq!(restarted.kept().unwrap(), Kept::Clear);
}

#[test]
fn a_stop_deleted_or_rolled_back_holds_as_a_stop() {
    let (folder, _vault, stop) = paired();
    let path = folder.path().join("stop.json");
    stop.stop(Why::Person).unwrap();
    let stopped = std::fs::read(&path).unwrap();
    stop.resume().unwrap();
    stop.stop(Why::Person).unwrap();

    // Deleted.
    std::fs::remove_file(&path).unwrap();
    assert_eq!(stop.kept().unwrap_err().code, Code::Stopped);
    // Rolled back to an earlier Stop, or to its clear before that.
    std::fs::write(&path, &stopped).unwrap();
    assert!(stop.kept().is_err());
    assert!(stop.resume().is_err(), "an unknown Stop is never lifted");
}

#[test]
fn a_change_cut_short_before_its_file_holds_as_a_stop_whichever_way_it_went() {
    let (_folder, _vault, stop) = paired();
    stop.stop(Why::Person).unwrap();
    // Committed to a next revision whose file never came.
    stop.commitment
        .begin(Revision {
            number: 2,
            digest: "a".repeat(64),
        })
        .unwrap();

    assert_eq!(stop.kept().unwrap_err().code, Code::Stopped);
}

#[test]
fn a_stop_committed_but_never_written_holds_after_a_restart_and_a_stop_writes_over_it() {
    let (folder, vault, stop) = paired();
    // Committed to a Stop whose file never came, from no Stop at all.
    stop.commitment
        .begin(Revision {
            number: 1,
            digest: "b".repeat(64),
        })
        .unwrap();

    let restarted = Stop::new(folder.path().join("stop.json"), vault);
    assert_eq!(restarted.kept().unwrap_err().code, Code::Stopped);
    assert!(restarted.resume().is_err());

    restarted.stop(Why::Lapsed).unwrap();
    assert_eq!(
        restarted.kept().unwrap(),
        Kept::Stopped { why: Why::Lapsed }
    );
    restarted.resume().unwrap();
    assert_eq!(restarted.kept().unwrap(), Kept::Clear);
}

#[test]
fn a_stop_written_over_an_unknown_one_never_trusts_what_came_before_on_the_way() {
    let (folder, vault, stop) = paired();
    // Unknown: a Stop committed from no Stop, its file never written.
    stop.commitment
        .begin(Revision {
            number: 1,
            digest: "b".repeat(64),
        })
        .unwrap();
    // Its overwrite committed in its place, and cut short before its file.
    stop.commitment
        .begin_over(Revision {
            number: 1,
            digest: "c".repeat(64),
        })
        .unwrap();

    let restarted = Stop::new(folder.path().join("stop.json"), vault);

    assert_eq!(restarted.kept().unwrap_err().code, Code::Stopped);
}

use super::*;
use std::{
    fs::File,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::Instant,
};

const GAP: Duration = Duration::from_secs(2);

/// The activity file in a new folder, last stamped `ago`.
fn stamped(ago: Duration) -> (tempfile::TempDir, PathBuf) {
    let folder = tempfile::tempdir().unwrap();
    let path = beside(&folder.path().join("input.sock"));
    drop(acting(&path));
    File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_modified(SystemTime::now() - ago)
        .unwrap();
    (folder, path)
}

#[test]
fn the_file_sits_beside_the_input_socket() {
    assert_eq!(
        beside(Path::new("/h/Atlas/input/input.sock")),
        Path::new("/h/Atlas/input/input.activity")
    );
}

#[test]
fn work_never_runs_while_input_is_under_way() {
    let (_folder, path) = stamped(GAP * 2);
    let input = acting(&path);
    assert_eq!(quiet(&path, GAP, || ()), None);
    drop(input);
}

#[test]
fn work_waits_for_the_quiet_gap_after_the_last_input() {
    let (_folder, path) = stamped(GAP * 2);
    drop(acting(&path));
    assert_eq!(quiet(&path, GAP, || ()), None, "input just ended");
    let (_folder, path) = stamped(GAP + Duration::from_millis(100));
    assert_eq!(quiet(&path, GAP, || 7), Some(7));
}

#[test]
fn a_clock_set_back_since_the_last_input_still_counts_the_time_between() {
    let (_folder, path) = stamped(Duration::ZERO);
    File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_modified(SystemTime::now() + GAP * 2)
        .unwrap();
    assert_eq!(quiet(&path, GAP, || 7), Some(7));
}

#[test]
fn input_that_comes_during_the_work_waits_for_it_and_then_goes_on() {
    let (_folder, path) = stamped(GAP * 2);
    let working = Arc::new(AtomicBool::new(false));
    let (started, began) = mpsc::channel();
    let work = {
        let (path, working) = (path.clone(), working.clone());
        std::thread::spawn(move || {
            quiet(&path, GAP, || {
                working.store(true, Ordering::SeqCst);
                started.send(()).unwrap();
                std::thread::sleep(Duration::from_millis(300));
                working.store(false, Ordering::SeqCst);
            })
        })
    };
    began.recv().unwrap();
    let asked = Instant::now();
    let input = acting(&path);
    assert!(
        !working.load(Ordering::SeqCst),
        "input began under the work"
    );
    assert!(asked.elapsed() >= Duration::from_millis(200), "it waited");
    assert!(input.is_some(), "input goes on, holding the file");
    assert_eq!(work.join().unwrap(), Some(()));
    drop(input);
    assert_eq!(quiet(&path, GAP, || ()), None, "the input was stamped");
}

#[test]
fn without_an_input_folder_work_runs_and_input_goes_on() {
    let folder = tempfile::tempdir().unwrap();
    let path = beside(&folder.path().join("missing/input.sock"));
    assert!(acting(&path).is_none());
    assert_eq!(quiet(&path, GAP, || 7), Some(7));
}

#[test]
fn input_past_its_patience_goes_on_holding_nothing() {
    let (_folder, path) = stamped(GAP * 2);
    let (started, working) = mpsc::channel();
    let (finish, finished) = mpsc::channel::<()>();
    let work = {
        let path = path.clone();
        std::thread::spawn(move || {
            quiet(&path, GAP, || {
                started.send(()).unwrap();
                finished.recv().unwrap();
            })
        })
    };
    working.recv().unwrap();
    let input = acting_within(&path, Duration::from_millis(50));
    assert!(input.is_none(), "it holds no lock it did not get");
    finish.send(()).unwrap();
    assert_eq!(work.join().unwrap(), Some(()));
    drop(input);
    assert_eq!(quiet(&path, GAP, || 7), Some(7), "and stamped nothing");
}

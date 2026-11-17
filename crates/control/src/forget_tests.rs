use super::*;
use atlas_core::files::token;

#[test]
fn a_desktop_forgotten_leaves_nothing_of_its_own_and_everything_of_anothers() {
    let directory = tempfile::tempdir().unwrap();
    let gone = Controller::at(505, directory.path());
    let other = Controller::at(506, directory.path());
    for controller in [&gone, &other] {
        drop(controller.acquire(false).unwrap());
        drop(controller.quarantine_command().unwrap());
        controller.note_stop(true).unwrap();
    }
    std::fs::create_dir_all(directory.path().join(format!("{}.waiting", gone.name))).unwrap();
    let before_other: Vec<_> = named(directory.path(), &other.name);

    gone.forget().unwrap();

    assert!(named(directory.path(), &gone.name).is_empty());
    assert_eq!(named(directory.path(), &other.name), before_other);
    assert!(!other.pending().unwrap().is_empty());
    assert!(other.stop_noted());
}

#[test]
fn a_desktop_whose_control_is_held_is_never_forgotten() {
    let directory = tempfile::tempdir().unwrap();
    let controller = Controller::at(505, directory.path());
    let held = Controller::at(505, directory.path())
        .acquire(false)
        .unwrap();
    drop(controller.quarantine_as(&token()).unwrap());

    let busy = controller.forget().unwrap_err();

    assert_eq!(busy.code, Code::Busy);
    assert!(!controller.pending().unwrap().is_empty());
    drop(held);
    controller.forget().unwrap();
    assert!(named(directory.path(), &controller.name).is_empty());
}

/// Every name in `directory` kept for the desktop `name`, sorted.
fn named(directory: &std::path::Path, name: &str) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(directory)
        .unwrap()
        .filter_map(|entry| entry.unwrap().file_name().into_string().ok())
        .filter(|entry| entry.starts_with(&format!("{name}.")))
        .collect();
    names.sort();
    names
}

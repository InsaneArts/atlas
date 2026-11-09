use super::*;
use std::{
    fs,
    os::unix::fs::{DirBuilderExt, PermissionsExt, symlink},
};

const DAY: Duration = Duration::from_secs(24 * 60 * 60);
const KEEP: Duration = Duration::from_secs(7 * 24 * 60 * 60);

fn kept(output: &Path, name: &str) -> bool {
    Listing::of(output).has(name)
}

fn nanos(at: SystemTime) -> u128 {
    at.duration_since(UNIX_EPOCH).unwrap().as_nanos()
}

fn key(text: &str) -> Key {
    Key::parse(text).unwrap()
}

/// A private output folder with its pictures' folder, in `directory`.
fn output(directory: &Path) -> std::path::PathBuf {
    let output = directory.join("runs");
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(output.join(FOLDER))
        .unwrap();
    output
}

/// A picture named `name`, of `size` bytes, in `output`'s folder.
fn picture(output: &Path, name: &str, size: usize) {
    fs::write(output.join(FOLDER).join(name), vec![0; size]).unwrap();
}

fn names(output: &Path) -> Vec<String> {
    let mut names: Vec<_> = fs::read_dir(output.join(FOLDER))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort();
    names
}

fn raw(at: u128) -> Raw {
    Raw {
        rgb: vec![200; 4 * 2 * 3],
        width: 4,
        height: 2,
        at,
    }
}

#[test]
fn a_pictures_name_says_when_it_was_taken_and_whose_action_it_came_before() {
    let (session, task) = (key("0123456789ab"), key("ba9876543210"));
    assert_eq!(name(5, None, None), "5-before.jpg");
    assert_eq!(name(5, Some(&session), None), "5-0123456789ab-before.jpg");
    assert_eq!(
        name(5, Some(&session), Some(&task)),
        "5-0123456789ab-ba9876543210-before.jpg"
    );
    assert_eq!(
        name(5, None, Some(&task)),
        "5-before.jpg",
        "never a task alone"
    );
    for named in [name(5, None, None), name(5, Some(&session), Some(&task))] {
        assert_eq!(time(&named), Some(5), "{named}");
    }
    for other in [
        "-before.jpg",
        "1730800000000000000-capture.png",
        "17308x0000-before.jpg",
        "../1-before.jpg",
        "before/1-before.jpg",
        "1-before.jpeg",
        "1--before.jpg",
        "1-0123456789AB-before.jpg",
        "1-0123456789ab-ba9876543210-0123456789ab-before.jpg",
    ] {
        assert_eq!(time(other), None, "{other}");
        assert!(Before::parse(other).is_none(), "{other}");
    }
}

#[test]
fn a_saved_picture_is_a_private_jpeg_in_its_own_folder() {
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("runs");
    let at = nanos(SystemTime::now());
    let name = name(at, None, None);
    save(&output, &name, &raw(at), KEEP).unwrap();

    let folder = output.join(FOLDER);
    let mode = |path: &Path| fs::metadata(path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode(&output), 0o700);
    assert_eq!(mode(&folder), 0o700);
    assert_eq!(mode(&folder.join(&name)), 0o600);
    let bytes = read(&output, &Before::parse(&name).unwrap()).unwrap();
    assert_eq!(bytes[..3], [0xff, 0xd8, 0xff], "a JPEG");
    assert!(kept(&output, &name));
    assert!(!output.join(&name).exists(), "never beside the screenshots");
    assert!(
        save(&output, &name, &raw(at), KEEP).is_err(),
        "never over another picture"
    );

    fs::set_permissions(&output, fs::Permissions::from_mode(0o755)).unwrap();
    let later = name.replace(&at.to_string(), &(at + 1).to_string());
    assert_eq!(
        save(&output, &later, &raw(at + 1), KEEP).unwrap_err().code,
        Code::Config,
        "never into a folder others can read"
    );
}

#[test]
fn a_picture_is_read_kept_and_removed_only_inside_its_own_folder() {
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("runs");
    fs::DirBuilder::new().mode(0o700).create(&output).unwrap();
    let elsewhere = output.join("elsewhere");
    fs::DirBuilder::new()
        .mode(0o700)
        .create(&elsewhere)
        .unwrap();
    let session = key("0123456789ab");
    let name = name(1, Some(&session), None);
    fs::write(elsewhere.join(&name), b"jpeg").unwrap();
    symlink(&elsewhere, output.join(FOLDER)).unwrap();

    let picture = Before::parse(&name).unwrap();
    assert_eq!(read(&output, &picture).unwrap_err().code, Code::BadRequest);
    assert!(!kept(&output, &name));
    forget(&output, &session, None);
    prune(&output, KEEP);
    assert!(
        elsewhere.join(&name).exists(),
        "a linked folder is never used"
    );
    assert!(save(&output, "2-before.jpg", &raw(2), KEEP).is_err());
    assert!(!elsewhere.join("2-before.jpg").exists());

    fs::remove_file(output.join(FOLDER)).unwrap();
    fs::DirBuilder::new()
        .mode(0o700)
        .create(output.join(FOLDER))
        .unwrap();
    symlink(elsewhere.join(&name), output.join(FOLDER).join(&name)).unwrap();
    assert_eq!(read(&output, &picture).unwrap_err().code, Code::BadRequest);
    assert!(!kept(&output, &name), "nor a linked picture");
}

#[test]
fn pruning_keeps_a_week_and_the_newest_within_their_room() {
    let directory = tempfile::tempdir().unwrap();
    let output = output(directory.path());
    let now = SystemTime::now();
    let at = |age: Duration| name(nanos(now - age), None, None);
    let (old, oldest_kept, middle, newest) = (at(KEEP + DAY), at(3 * DAY), at(2 * DAY), at(DAY));
    picture(&output, &old, 10);
    for name in [&oldest_kept, &middle, &newest] {
        picture(&output, name, 40);
    }
    picture(&output, "1730800000000000000-capture.png", 4);
    picture(&output, "notes.txt", 4);
    fs::create_dir(output.join(FOLDER).join("2-before.jpg")).unwrap();

    prune_in(&folder(&output).unwrap(), now, 100, KEEP);

    let left = names(&output);
    assert!(!left.contains(&old), "older than a week");
    assert!(!left.contains(&oldest_kept), "the oldest past the room");
    assert!(left.contains(&middle) && left.contains(&newest));
    for other in [
        "1730800000000000000-capture.png",
        "notes.txt",
        "2-before.jpg",
    ] {
        assert!(left.contains(&other.to_owned()), "{other} is not a picture");
    }
}

#[test]
fn old_pictures_go_when_atlas_starts_and_it_says_when_the_next_expires() {
    let directory = tempfile::tempdir().unwrap();
    let output = output(directory.path());
    let now = SystemTime::now();
    picture(&output, &name(nanos(now - KEEP - DAY), None, None), 1);
    assert_eq!(prune(&output, KEEP), KEEP, "a week once none is left");
    assert!(names(&output).is_empty());

    picture(&output, &name(nanos(now - 6 * DAY), None, None), 1);
    let next = prune(&output, KEEP);
    assert!(
        next <= DAY && next > DAY - Duration::from_secs(60),
        "{next:?}"
    );
}

#[test]
fn a_saved_picture_is_never_seen_half_written() {
    let directory = tempfile::tempdir().unwrap();
    let output = output(directory.path());
    let at = nanos(SystemTime::now());
    let taken = name(at, None, None);
    fs::create_dir(output.join(FOLDER).join(&taken)).unwrap();
    assert!(
        save(&output, &taken, &raw(at), KEEP).is_err(),
        "its place is taken"
    );
    assert_eq!(
        names(&output),
        std::slice::from_ref(&taken),
        "and its temporary file is gone"
    );

    let abandoned = name(nanos(SystemTime::now() - 2 * ABANDONED), None, None);
    let writing = name(at + 1, None, None);
    picture(&output, &format!("{abandoned}{PARTIAL}"), 1);
    picture(&output, &format!("{writing}{PARTIAL}"), 1);
    prune(&output, KEEP);
    assert_eq!(
        names(&output),
        [taken, format!("{writing}{PARTIAL}")],
        "only an abandoned writer's file goes"
    );
}

#[test]
fn a_picture_written_after_its_sessions_cleanup_is_never_kept() {
    let directory = tempfile::tempdir().unwrap();
    let output = output(directory.path());
    let (mine, probe) = (key("0123456789ab"), key("ba9876543210"));
    let taken = nanos(SystemTime::now());
    std::thread::sleep(Duration::from_millis(2));
    forget(&output, &mine, Some(&probe));
    for (session, task, stays) in [
        (Some(&mine), Some(&probe), false),
        (Some(&mine), None, true),
        (None, None, true),
    ] {
        let late = name(taken, session, task);
        save(&output, &late, &raw(taken), KEEP).unwrap();
        assert_eq!(kept(&output, &late), stays, "{late}");
    }

    std::thread::sleep(Duration::from_millis(2));
    let after = name(nanos(SystemTime::now()), Some(&mine), Some(&probe));
    save(&output, &after, &raw(time(&after).unwrap()), KEEP).unwrap();
    assert!(kept(&output, &after), "one taken after the cleanup stays");
}

#[test]
fn a_picture_saved_aside_is_in_place_once_the_process_settles() {
    crate::test_support::prepare_picture_encoder();
    let directory = tempfile::tempdir().unwrap();
    let output = output(directory.path());
    let at = nanos(SystemTime::now());
    let taken = name(at, None, None);
    save_aside(output.clone(), taken.clone(), raw(at), KEEP);
    settle(Duration::from_secs(10));
    assert!(kept(&output, &taken));
}

#[test]
fn a_cleanup_before_any_picture_is_kept_still_marks_its_session() {
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("runs");
    let mine = key("0123456789ab");
    let taken = nanos(SystemTime::now());
    std::thread::sleep(Duration::from_millis(2));
    forget(&output, &mine, None);
    let late = name(taken, Some(&mine), None);
    save(&output, &late, &raw(taken), KEEP).unwrap();
    assert!(
        !kept(&output, &late),
        "the first picture, saved after the cleanup"
    );
}

#[test]
fn a_cleanup_forgets_only_its_own_sessions_or_tasks_pictures() {
    let directory = tempfile::tempdir().unwrap();
    let output = output(directory.path());
    let (mine, theirs) = (key("0123456789ab"), key("aaaaaaaaaaaa"));
    let (probe, other) = (key("ba9876543210"), key("bbbbbbbbbbbb"));
    let shots = [
        name(1, Some(&mine), None),
        name(2, Some(&mine), Some(&probe)),
        name(3, Some(&mine), Some(&other)),
        name(4, Some(&theirs), None),
        name(5, None, None),
    ];
    for shot in &shots {
        picture(&output, shot, 1);
    }

    let pictures = || {
        names(&output)
            .into_iter()
            .filter(|name| time(name).is_some())
            .collect::<Vec<_>>()
    };
    forget(&output, &mine, Some(&probe));
    assert!(!pictures().contains(&shots[1]));
    assert_eq!(pictures().len(), 4, "only the task's");

    forget(&output, &mine, None);
    assert_eq!(pictures(), [shots[3].clone(), shots[4].clone()]);
    assert!(
        names(&output).contains(&format!("{mine}{FORGOTTEN}")),
        "it marks when it ran"
    );
}

#[test]
fn a_command_takes_its_picture_once_and_names_it_once_in_hand() {
    let slot = Slot::default();
    let other_client = slot.clone();
    assert!(slot.take());
    assert!(!other_client.take(), "one picture for each command");
    assert_eq!(slot.name(), None, "not while it is taken");
    other_client.named("1-before.jpg".into(), Map::new());
    assert_eq!(slot.name().as_deref(), Some("1-before.jpg"));
    assert_ne!(slot, Slot::default());
    assert_eq!(slot, other_client);
}

#[test]
fn an_actions_after_is_its_own_or_its_sessions_next_picture_within_a_second() {
    let directory = tempfile::tempdir().unwrap();
    let output = output(directory.path());
    let (mine, theirs) = (key("0123456789ab"), key("aaaaaaaaaaaa"));
    let second = 1_000_000_000;
    let start = 1_000 * second;
    let first = name(start, Some(&mine), None);
    let ended = 1_000_200; // milliseconds: 200 ms after the picture
    let theirs_next = name(start + second / 2, Some(&theirs), None);
    let mine_late = name(start + 2 * second, Some(&mine), None);
    for file in [&first, &theirs_next, &mine_late] {
        picture(&output, file, 1);
    }
    let kept = || Listing::of(&output);
    assert_eq!(kept().after(&first, ended), None, "nothing within a second");
    assert!(wants_after(&output, &first, ended));

    let mine_next = name(start + second, Some(&mine), None);
    picture(&output, &mine_next, 1);
    assert_eq!(
        kept().after(&first, ended),
        Some((mine_next.clone(), 1_001_000)),
        "taken when its name says"
    );
    assert!(
        !wants_after(&output, &first, ended),
        "the next step's serves"
    );

    let own = after_name(&first).unwrap();
    assert!(own.ends_with("-0123456789ab-after.jpg"));
    picture(&output, &own, 1);
    assert_eq!(
        kept().after(&first, ended).map(|(after, _)| after),
        Some(own.clone()),
        "its own first"
    );
    assert!(kept().has(&own) && time(&own) == Some(start));
}

#[test]
fn an_actions_own_picture_after_says_when_it_was_taken_not_when_its_action_began() {
    let directory = tempfile::tempdir().unwrap();
    let output = output(directory.path());
    let at = nanos(SystemTime::now()) - 2_000_000_000;
    let first = name(at, None, None);
    save(&output, &first, &raw(at), KEEP).unwrap();
    let taken = at + 1_300_000_000;
    let own = after_name(&first).unwrap();
    save(&output, &own, &raw(taken), KEEP).unwrap();

    let ended = u64::try_from(at / 1_000_000).unwrap() + 200;
    assert_eq!(
        Listing::of(&output).after(&first, ended),
        Some((own, u64::try_from(taken / 1_000_000).unwrap()))
    );
}

#[test]
fn an_actions_facts_are_kept_whole_beside_its_picture_and_replaced_as_it_ends() {
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("runs");
    let before = name(nanos(SystemTime::now()), None, None);
    assert_eq!(facts(&output, &before), None);
    keep_facts(&output, &before, &serde_json::json!({"outcome": "worked"})).unwrap();
    keep_facts(&output, &before, &serde_json::json!({"outcome": "failed"})).unwrap();
    assert_eq!(
        facts(&output, &before),
        Some(serde_json::json!({"outcome": "failed"}))
    );
    assert!(
        !kept(&output, &before.replace("-before.jpg", "-facts.json")),
        "no picture"
    );
    assert!(keep_facts(&output, "notes.txt", &serde_json::json!({})).is_err());
}

#[test]
fn settings_count_the_pictures_and_delete_all_of_them() {
    let directory = tempfile::tempdir().unwrap();
    let output = output(directory.path());
    let at = nanos(SystemTime::now());
    let taken = name(at, None, None);
    picture(&output, &taken, 10);
    picture(&output, &after_name(&taken).unwrap(), 20);
    keep_facts(&output, &taken, &serde_json::json!({})).unwrap();
    picture(&output, "notes.txt", 5);
    let (count, bytes) = summary(&output);
    assert_eq!(count, 2, "the pictures before and after");
    assert!(bytes >= 32, "{bytes}");

    delete_all(&output).unwrap();
    assert_eq!(summary(&output), (0, 0));
    assert_eq!(
        names(&output),
        ["all.forgotten", "notes.txt"],
        "only Atlas's own files go, and it marks when"
    );
}

#[test]
fn nothing_taken_before_delete_all_is_kept_again_after_it() {
    let directory = tempfile::tempdir().unwrap();
    let output = output(directory.path());
    let mine = key("0123456789ab");
    let taken = nanos(SystemTime::now());
    std::thread::sleep(Duration::from_millis(2));
    delete_all(&output).unwrap();
    for session in [Some(&mine), None] {
        let late = name(taken, session, None);
        save(&output, &late, &raw(taken), KEEP).unwrap();
        assert!(!kept(&output, &late), "a picture still being saved: {late}");
        let after = after_name(&late).unwrap();
        save(&output, &after, &raw(taken), KEEP).unwrap();
        assert!(!kept(&output, &after), "a picture after it taken later");
        keep_facts(&output, &late, &serde_json::json!({"outcome": "worked"})).unwrap();
        assert!(
            facts(&output, &late).is_none(),
            "its facts written as it ends"
        );
    }

    std::thread::sleep(Duration::from_millis(2));
    let next = name(nanos(SystemTime::now()), Some(&mine), None);
    save(&output, &next, &raw(time(&next).unwrap()), KEEP).unwrap();
    keep_facts(&output, &next, &serde_json::json!({})).unwrap();
    assert!(kept(&output, &next), "one taken after it stays");
    assert!(facts(&output, &next).is_some());
}

#[test]
fn delete_all_before_any_picture_is_kept_still_marks_when() {
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("runs");
    let taken = nanos(SystemTime::now());
    std::thread::sleep(Duration::from_millis(2));
    delete_all(&output).unwrap();
    let late = name(taken, None, None);
    save(&output, &late, &raw(taken), KEEP).unwrap();
    assert!(!kept(&output, &late));
}

#[test]
fn delete_all_that_cannot_mark_when_says_so_rather_than_done() {
    let directory = tempfile::tempdir().unwrap();
    let output = output(directory.path());
    let taken = name(nanos(SystemTime::now()), None, None);
    picture(&output, &taken, 10);
    // Something that is not a mark stands in its place.
    fs::create_dir(output.join(FOLDER).join("all.forgotten")).unwrap();

    assert!(delete_all(&output).is_err());
}

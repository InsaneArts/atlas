use super::*;
use crate::machine::{commitment::create, vault::Memory};
use std::{
    fs,
    os::unix::fs::{PermissionsExt, symlink},
    process::{Child, Command},
    sync::Arc,
};

/// The guest's copies folder, its stages and its change records.
struct Guest {
    _folders: (tempfile::TempDir, tempfile::TempDir),
    root: Root,
    stages: Stages,
    changes: Changes<Arc<Memory>>,
}

fn guest() -> Guest {
    let copies = tempfile::tempdir().unwrap();
    let records = tempfile::tempdir().unwrap();
    let vault = Arc::new(Memory::default());
    create(&vault, crate::machine::changes::STORE).unwrap();
    let path = copies.path().join("open");
    Guest {
        root: Root::open(&path).unwrap(),
        stages: Stages::new(path),
        changes: Changes::new(records.path(), vault),
        _folders: (copies, records),
    }
}

/// A placed copy holding a script that sleeps: its token, and its folder.
fn placed_copy(guest: &Guest) -> (Token, PathBuf) {
    let token = Token::new();
    let folder = guest.root.path.join(token.as_str());
    let tool = folder.join("Tool.app/Contents");
    fs::create_dir_all(&tool).unwrap();
    let script = tool.join("run.sh");
    fs::write(&script, b"#!/bin/sh\nsleep 30\n").unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
    (token, folder)
}

fn node(path: &Path) -> String {
    let stat = nix::sys::stat::lstat(path).unwrap();
    format!("{}:{}", stat.st_dev, stat.st_ino)
}

/// A process running the copy's script, ended when dropped.
struct Runs(Child);

impl Runs {
    fn script(folder: &Path, relative: bool) -> Self {
        let tool = folder.join("Tool.app/Contents");
        let mut command = Command::new("/bin/sh");
        if relative {
            command.arg("run.sh").current_dir(&tool);
        } else {
            command.arg(tool.join("run.sh"));
        }
        std::os::unix::process::CommandExt::process_group(&mut command, 0);
        let child = atlas_core::spawn::spawn(&mut command).unwrap();
        // Long enough for the kernel to show its arguments.
        std::thread::sleep(std::time::Duration::from_millis(200));
        Self(child)
    }

    fn pid(&self) -> i32 {
        i32::try_from(self.0.id()).unwrap()
    }
}

impl Drop for Runs {
    fn drop(&mut self) {
        // The whole group: the script's own children go with it.
        let _ = nix::sys::signal::killpg(
            nix::unistd::Pid::from_raw(self.pid()),
            nix::sys::signal::Signal::SIGKILL,
        );
        let _ = self.0.wait();
    }
}

fn removing(guest: &Guest, change: &Token, token: &Token, node: &str) -> Removal {
    guest
        .changes
        .ask(change, OPERATION, &json!({"op": "remove-copy"}))
        .unwrap();
    remove(&guest.root, change, token, node, || None)
}

fn came(removal: Removal) -> Value {
    match removal {
        Removal::Came(reply) => reply,
        Removal::Pending(reply) => panic!("pending: {reply}"),
    }
}

#[test]
fn a_copy_is_listed_with_its_identity_and_the_process_running_from_it() {
    let guest = guest();
    let (token, folder) = placed_copy(&guest);
    let runs = Runs::script(&folder, false);

    let listed = inventory(&guest.root, &guest.stages, Some(&token), None);

    let copy = &listed["copies"][0];
    assert_eq!(copy["copy"], token.as_str(), "{listed}");
    assert_eq!(copy["state"], "placed");
    assert_eq!(copy["node"], node(&folder));
    assert!(
        copy["processes"]
            .as_array()
            .is_some_and(|pids| pids.contains(&json!(runs.pid()))),
        "{copy}"
    );
}

#[test]
fn a_script_run_by_a_relative_path_from_its_folder_counts_as_in_use() {
    let guest = guest();
    let (token, folder) = placed_copy(&guest);
    let runs = Runs::script(&folder, true);

    let refused = came(removing(&guest, &Token::new(), &token, &node(&folder)));

    assert_eq!(refused["outcome"], "inUse", "{refused}");
    assert!(
        refused["pids"]
            .as_array()
            .unwrap()
            .contains(&json!(runs.pid()))
    );
    assert!(folder.exists());
}

#[test]
fn a_copy_in_use_is_kept_and_removed_once_it_ends() {
    let guest = guest();
    let (token, folder) = placed_copy(&guest);
    let runs = Runs::script(&folder, false);
    let refused = came(removing(&guest, &Token::new(), &token, &node(&folder)));
    assert_eq!(refused["outcome"], "inUse", "{refused}");
    drop(runs);

    let removed = came(removing(&guest, &Token::new(), &token, &node(&folder)));

    assert_eq!(removed["outcome"], "removed", "{removed}");
    assert!(!folder.exists());
}

#[test]
fn a_copy_whose_folder_was_swapped_is_kept() {
    let guest = guest();
    let (token, folder) = placed_copy(&guest);
    let looked_at = node(&folder);
    fs::rename(&folder, guest.root.path.join("elsewhere")).unwrap();
    fs::create_dir(&folder).unwrap();

    let kept = came(removing(&guest, &Token::new(), &token, &looked_at));

    assert_eq!(kept["outcome"], "changed", "{kept}");
    assert!(folder.exists());
}

#[test]
fn a_link_inside_a_removed_copy_leaves_what_it_points_at() {
    let guest = guest();
    let (token, folder) = placed_copy(&guest);
    let outside = tempfile::tempdir().unwrap();
    fs::write(outside.path().join("kept"), b"keep me").unwrap();
    symlink(outside.path(), folder.join("out")).unwrap();

    let removed = came(removing(&guest, &Token::new(), &token, &node(&folder)));

    assert_eq!(removed["outcome"], "removed", "{removed}");
    assert_eq!(fs::read(outside.path().join("kept")).unwrap(), b"keep me");
}

#[test]
fn a_removal_refused_right_before_it_moves_leaves_the_copy_and_no_record() {
    let guest = guest();
    let (token, folder) = placed_copy(&guest);
    let change = Token::new();
    guest
        .changes
        .ask(&change, OPERATION, &json!({"op": "remove-copy"}))
        .unwrap();

    let refused = came(remove(&guest.root, &change, &token, &node(&folder), || {
        Some(failure(Code::Stopped, "Agents are stopped on this desktop"))
    }));

    assert_eq!(refused["code"], "stopped", "{refused}");
    assert!(folder.exists());
    assert!(record_of(&guest.root, &token).is_none());
}

/// A removal cut short after its copy was moved, as its record says.
fn cut_short(guest: &Guest, phase: Phase) -> (Token, Token, PathBuf) {
    let (token, folder) = placed_copy(guest);
    let change = Token::new();
    guest
        .changes
        .ask(&change, OPERATION, &json!({"op": "remove-copy"}))
        .unwrap();
    let record = Record {
        change: change.clone(),
        token: token.clone(),
        node: node(&folder),
        phase,
        why: Some("inUse".into()),
    };
    keep(&guest.root, &record).unwrap();
    let moved = guest
        .root
        .path
        .join(format!(".removing-{}", token.as_str()));
    fs::rename(&folder, &moved).unwrap();
    (change, token, moved)
}

fn kept_as(guest: &Guest, change: &Token) -> Value {
    match guest.changes.of(change).unwrap() {
        Some(State::Done { reply }) => reply,
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_removal_cut_short_once_it_was_deleting_is_finished_at_start() {
    let guest = guest();
    let (change, token, moved) = cut_short(&guest, Phase::Deleting);

    recover(&guest.root, &guest.changes).unwrap();

    assert_eq!(kept_as(&guest, &change)["outcome"], "removed");
    assert!(!moved.exists());
    assert!(record_of(&guest.root, &token).is_none());
}

#[test]
fn a_removal_cut_short_while_putting_back_is_put_back_never_deleted() {
    let guest = guest();
    let (change, token, moved) = cut_short(&guest, Phase::PuttingBack);

    recover(&guest.root, &guest.changes).unwrap();

    assert_eq!(kept_as(&guest, &change)["outcome"], "inUse");
    assert!(!moved.exists());
    assert!(guest.root.path.join(token.as_str()).exists());
}

#[test]
fn a_removal_cut_short_before_it_was_checked_again_is_put_back_never_deleted() {
    let guest = guest();
    let (change, token, moved) = cut_short(&guest, Phase::Removing);

    recover(&guest.root, &guest.changes).unwrap();

    assert_ne!(kept_as(&guest, &change)["outcome"], "removed");
    assert!(!moved.exists());
    assert!(guest.root.path.join(token.as_str()).exists());
}

#[test]
fn a_removal_cut_short_while_deleting_with_a_process_running_from_it_is_put_back_at_start() {
    let guest = guest();
    let (change, token, moved) = cut_short(&guest, Phase::Deleting);
    let _runs = Runs::script(&moved, false);

    recover(&guest.root, &guest.changes).unwrap();

    assert_ne!(kept_as(&guest, &change)["outcome"], "removed");
    assert!(guest.root.path.join(token.as_str()).exists());
}

#[test]
fn a_removal_folder_with_no_record_is_never_deleted() {
    let guest = guest();
    let (_change, token, moved) = cut_short(&guest, Phase::Removing);
    forget(&guest.root, &token).unwrap();

    recover(&guest.root, &guest.changes).unwrap();

    assert!(moved.exists());
}

#[test]
fn an_intended_removal_with_no_record_removed_nothing() {
    let guest = guest();
    let (token, folder) = placed_copy(&guest);
    let change = Token::new();
    guest
        .changes
        .ask(&change, OPERATION, &json!({"op": "remove-copy"}))
        .unwrap();

    recover(&guest.root, &guest.changes).unwrap();

    assert_eq!(kept_as(&guest, &change)["ok"], false);
    assert!(folder.exists(), "{token:?}");
}

#[test]
fn a_new_removal_puts_right_what_an_earlier_one_left_first() {
    let guest = guest();
    let (earlier, token, moved) = cut_short(&guest, Phase::Deleting);

    let righted = put_right(&guest.root, &guest.changes, &token);
    let again = came(removing(&guest, &Token::new(), &token, "1:1"));

    assert!(righted);
    assert_eq!(kept_as(&guest, &earlier)["outcome"], "removed");
    assert!(!moved.exists());
    assert_eq!(again["outcome"], "alreadyGone", "{again}");
}

#[test]
fn a_script_named_by_a_path_its_copy_has_since_left_still_counts_as_running_from_it() {
    let guest = guest();
    let (_token, folder) = placed_copy(&guest);
    let runs = Runs::script(&folder, false);
    let named = std::fs::canonicalize(&folder).unwrap();
    fs::rename(&folder, guest.root.path.join("moved")).unwrap();

    let from = Running::now().within(&named);

    assert!(from.is_some_and(|pids| pids.contains(&runs.pid())));
}

#[test]
fn work_registered_on_a_token_with_no_folder_yet_is_listed_as_pending() {
    let guest = guest();
    let token = Token::new();
    let _claimed = guest.stages.claim(&token).unwrap();

    let listed = inventory(&guest.root, &guest.stages, Some(&token), None);

    assert_eq!(listed["copies"][0]["state"], "pending", "{listed}");
}

#[test]
fn a_token_staged_is_never_claimed_for_removal_and_one_claimed_is_never_claimed_again() {
    let guest = Arc::new(guest());
    let (staged, removed) = (Token::new(), Token::new());
    let stages = Arc::new(Stages::new(guest.root.path.clone()));
    let _running = stages.register(&staged);
    let _claimed = stages.claim(&removed).unwrap();

    assert!(stages.claim(&staged).is_none());
    assert!(stages.claim(&removed).is_none());
    assert!(stages.removing(&removed));
}

#[test]
fn copies_in_every_state_are_each_listed_once_across_pages_and_other_names_apart() {
    let guest = guest();
    let mut tokens: Vec<_> = (0..PAGE + 3)
        .map(|_| {
            let token = Token::new();
            fs::create_dir(guest.root.path.join(token.as_str())).unwrap();
            token
        })
        .collect();
    let moving = Token::new();
    fs::create_dir(
        guest
            .root
            .path
            .join(format!(".removing-{}", moving.as_str())),
    )
    .unwrap();
    tokens.push(moving);
    fs::create_dir(guest.root.path.join("planted")).unwrap();

    let mut seen = Vec::new();
    let first = inventory(&guest.root, &guest.stages, None, None);
    let mut page = first.clone();
    loop {
        for copy in page["copies"].as_array().unwrap() {
            seen.push(copy["copy"].as_str().unwrap().to_owned());
        }
        let Some(next) = page["next"].as_str().and_then(Token::parse) else {
            break;
        };
        page = inventory(&guest.root, &guest.stages, None, Some(&next));
    }

    let mut expected: Vec<_> = tokens
        .iter()
        .map(|token| token.as_str().to_owned())
        .collect();
    expected.sort();
    assert_eq!(seen, expected);
    assert_eq!(first["unknown"], json!(["planted"]));
}

#[test]
fn every_copy_is_listed_page_by_page_and_one_is_found_by_its_token() {
    let guest = guest();
    let mut tokens: Vec<_> = (0..PAGE + 10)
        .map(|_| {
            let token = Token::new();
            fs::create_dir(guest.root.path.join(token.as_str())).unwrap();
            token
        })
        .collect();
    tokens.sort_by(|a, b| a.as_str().cmp(b.as_str()));

    let first = inventory(&guest.root, &guest.stages, None, None);
    let after = Token::parse(first["next"].as_str().unwrap()).unwrap();
    let second = inventory(&guest.root, &guest.stages, None, Some(&after));
    let one = inventory(&guest.root, &guest.stages, Some(&tokens[PAGE + 5]), None);

    assert_eq!(first["copies"].as_array().unwrap().len(), PAGE);
    assert_eq!(second["copies"].as_array().unwrap().len(), 10);
    assert!(second["next"].is_null());
    assert_eq!(one["copies"][0]["copy"], tokens[PAGE + 5].as_str());
}

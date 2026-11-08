use super::*;

const HOME: &str = "drwxr-x---+ 100 theiskaa  staff  3200 Nov  8 18:28 /Users/theiskaa\n";

fn listing(entries: &[&str]) -> String {
    let mut text = HOME.to_owned();
    for (index, entry) in entries.iter().enumerate() {
        text.push_str(&format!(" {index}: {entry}\n"));
    }
    text
}

#[test]
fn a_home_with_only_the_everyone_entry_is_open() {
    assert!(!private_in(
        &listing(&["group:everyone deny delete"]),
        "helper"
    ));
}

#[test]
fn the_helpers_deny_first_or_after_a_deny_makes_it_private() {
    let first = listing(&["user:helper deny list,search", "group:everyone deny delete"]);
    let second = listing(&["group:everyone deny delete", "user:helper deny search,list"]);
    assert!(private_in(&first, "helper"));
    assert!(private_in(&second, "helper"));
    assert_eq!(ours(&second, "helper"), Some(1));
}

#[test]
fn a_subject_with_spaces_is_read_whole() {
    let spaced = listing(&[
        "group:Domain Users allow list,search",
        "user:helper deny list,search",
    ]);
    assert!(!private_in(&spaced, "helper"));
    assert_eq!(ours(&spaced, "helper"), Some(1));
}

#[test]
fn atlas_finds_its_entry_even_where_it_decides_nothing() {
    let overridden = listing(&["user:helper allow list", "user:helper deny list,search"]);
    assert!(!private_in(&overridden, "helper"));
    assert_eq!(ours(&overridden, "helper"), Some(1));
    assert_eq!(
        ours(&listing(&["group:everyone deny delete"]), "helper"),
        None
    );
}

#[test]
fn another_accounts_deny_or_a_former_helpers_id_does_not() {
    assert!(!private_in(
        &listing(&["user:someone deny list,search"]),
        "helper"
    ));
    let former = listing(&["2C2A6E12-0C0E-4F7A-9A44-6C11B0B1C9F2 deny list,search"]);
    assert!(!private_in(&former, "helper"));
}

#[test]
fn a_deny_that_only_reaches_new_items_or_comes_after_an_allow_does_not() {
    let inherit_only = listing(&["user:helper deny list,search,file_inherit,only_inherit"]);
    let after_allow = listing(&["group:staff allow list", "user:helper deny list,search"]);
    let one_right = listing(&["user:helper deny list"]);
    assert!(!private_in(&inherit_only, "helper"));
    assert!(!private_in(&after_allow, "helper"));
    assert!(!private_in(&one_right, "helper"));
}

#[test]
fn an_inherited_entry_is_read_past_its_word() {
    let inherited = listing(&["user:helper inherited deny list,search"]);
    assert!(private_in(&inherited, "helper"));
}

const HELPER: u32 = 503;
const STAFF: u32 = 20;

fn home_listing(home_mode: &str, items: &[&str]) -> String {
    let mut text = format!(
        "total 8\n{home_mode}  100 501  20  3200 Nov  8 18:28 .\n 0: group:everyone deny delete\n\
         drwxr-xr-x    6 0  80   192 Nov  3 12:42 ..\n"
    );
    for item in items {
        text.push_str(item);
        text.push('\n');
    }
    text
}

#[test]
fn folders_the_helper_can_enter_are_counted_and_named_in_order() {
    let text = home_listing(
        "drwxr-x---+",
        &[
            "drwxr-xr-x@  13 501  20   416 Oct 21 14:46 dev",
            "drwxr-xr-x@  18 501  20   576 Oct 22 02:50 .config",
            "drwx------@   7 501  20   224 Nov  8 18:28 Library",
            "-rw-r--r--@   1 501  20    53 Oct  7 07:17 notes.txt",
            "drwxr-xr-x    3 501  20    96 Jul  4 17:42 My Projects",
        ],
    );
    let (count, examples) = readable_in(&text, HELPER, &[STAFF]);
    assert_eq!(count, 3);
    assert_eq!(examples, ["dev", "My Projects"]);
}

#[test]
fn a_home_the_helper_cannot_enter_counts_nothing() {
    let text = home_listing(
        "drwx------+",
        &["drwxr-xr-x  13 501  20   416 Oct 21 14:46 dev"],
    );
    assert_eq!(readable_in(&text, HELPER, &[STAFF]), (0, Vec::new()));
}

#[test]
fn each_folder_is_judged_by_the_class_the_helper_falls_in() {
    let text = home_listing(
        "drwxr-x---+",
        &[
            // The group's bits decide for a member, even when everyone else's are wider.
            "drwx---r-x   2 501  20    64 Oct 21 14:46 grouped",
            // Everyone else's bits decide for a group the helper isn't in.
            "drwx---r-x   2 501  80    64 Oct 21 14:46 others",
            // The owner's bits decide for a folder the helper owns.
            "d---r-xr-x   2 503  20    64 Oct 21 14:46 owned",
            // A folder with its own access entries isn't counted.
            "drwxr-xr-x+  2 501  20    64 Oct 21 14:46 listed",
            " 0: user:helper deny list",
        ],
    );
    let (count, examples) = readable_in(&text, HELPER, &[STAFF]);
    assert_eq!(count, 1);
    assert_eq!(examples, ["others"]);
}

#[test]
fn the_offer_is_remembered_for_one_helper_only() {
    let home = tempfile::tempdir().unwrap();
    assert!(!offered_in(home.path(), "A-GUID"));
    mark_offered_in(home.path(), "A-GUID");
    assert!(offered_in(home.path(), "A-GUID"));
    assert!(!offered_in(home.path(), "ANOTHER-GUID"));
}

#[test]
fn only_the_person_opens_the_home_folder() {
    let refusal = open(HELPER, false).unwrap_err();
    assert_eq!(refusal.code, Code::Stopped);
}

#[test]
fn the_undo_line_names_the_helpers_entry() {
    assert_eq!(
        undo_line("helper"),
        "chmod -a \"user:helper deny list,search\" ~"
    );
}

#[test]
fn a_home_that_may_keep_the_helper_out_hides_its_folders() {
    assert!(denies_search(&listing(&["user:helper deny search"])));
    assert!(denies_search(&listing(&["group:everyone deny search"])));
    assert!(denies_search(&listing(&[
        "group:staff allow search,only_inherit",
        "group:everyone deny search"
    ])));
    assert!(denies_search(&listing(&[
        "user:someone allow search",
        "group:everyone deny search"
    ])));
    assert!(!denies_search(&listing(&[
        "group:staff deny search,only_inherit"
    ])));
    assert!(!denies_search(&listing(&[
        "user:helper deny list",
        "group:everyone deny delete"
    ])));
}

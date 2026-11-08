use super::*;

fn github(now: u64) -> Entry {
    account_of(
        &Seen {
            app: "com.google.Chrome".to_owned(),
            name: Some("Google Chrome".to_owned()),
            host: Some("github.com".to_owned()),
        },
        now,
    )
}

#[test]
fn a_missing_record_is_an_empty_list() {
    let home = tempfile::tempdir().unwrap();
    assert_eq!(listed_in(home.path()), Listed::Accounts(Vec::new()));
}

#[test]
fn accounts_round_trip_and_the_same_one_is_listed_once() {
    let home = tempfile::tempdir().unwrap();
    assert!(add_in(home.path(), github(1), true).unwrap());
    assert!(!add_in(home.path(), github(2), true).unwrap());
    let Listed::Accounts(accounts) = listed_in(home.path()) else {
        panic!("unread");
    };
    assert_eq!(accounts, [github(1)]);
    assert_eq!(accounts[0].level, Level::Ask);

    assert!(
        remove_in(
            home.path(),
            Kind::Site,
            "github.com",
            Some("com.google.Chrome"),
            true
        )
        .unwrap()
    );
    assert_eq!(listed_in(home.path()), Listed::Accounts(Vec::new()));
}

#[test]
fn the_record_is_private_to_the_person() {
    use std::os::unix::fs::PermissionsExt;
    let home = tempfile::tempdir().unwrap();
    add_in(home.path(), github(1), true).unwrap();
    let mode = std::fs::metadata(home.path().join(RECORD))
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o600);
}

#[test]
fn a_corrupt_or_newer_record_is_unknown_and_never_written_over() {
    let home = tempfile::tempdir().unwrap();
    let record = home.path().join(RECORD);
    std::fs::create_dir_all(record.parent().unwrap()).unwrap();
    for text in ["{ not json", r#"{"version": 2, "accounts": []}"#] {
        std::fs::write(&record, text).unwrap();
        assert_eq!(listed_in(home.path()), Listed::Unknown);
        assert_eq!(
            add_in(home.path(), github(1), true).unwrap_err().code,
            Code::State
        );
        assert_eq!(std::fs::read_to_string(&record).unwrap(), text);
    }
}

#[test]
fn only_the_person_changes_the_list() {
    let home = tempfile::tempdir().unwrap();
    assert_eq!(
        add_in(home.path(), github(1), false).unwrap_err().code,
        Code::Stopped
    );
    add_in(home.path(), github(1), true).unwrap();
    let refused = remove_in(
        home.path(),
        Kind::Site,
        "github.com",
        Some("com.google.Chrome"),
        false,
    );
    assert_eq!(refused.unwrap_err().code, Code::Stopped);
    assert!(matches!(listed_in(home.path()), Listed::Accounts(accounts) if accounts.len() == 1));
}

#[test]
fn sites_are_listed_by_their_registrable_domain() {
    assert_eq!(site_key("accounts.google.com"), "google.com");
    assert_eq!(site_key("GitHub.com."), "github.com");
    assert_eq!(site_key("a.b.co.uk"), "b.co.uk");
    assert_eq!(site_key("alice.github.io"), "alice.github.io");
    assert_ne!(site_key("alice.github.io"), site_key("bob.github.io"));
    assert_eq!(site_key("localhost"), "localhost");
    assert_eq!(site_key("127.0.0.1"), "127.0.0.1");
    assert_eq!(site_key("::1"), "::1");
}

#[test]
fn a_password_outside_a_page_lists_the_app() {
    let entry = account_of(
        &Seen {
            app: "com.tinyspeck.slackmacgap".to_owned(),
            name: Some("Slack".to_owned()),
            host: None,
        },
        5,
    );
    assert_eq!(entry.kind, Kind::App);
    assert_eq!(entry.name, "Slack");
    assert_eq!(entry.browser, None);
    assert_eq!(entry.how, How::TakeControl);
}

#[test]
fn the_same_site_in_two_browsers_is_two_accounts() {
    let home = tempfile::tempdir().unwrap();
    add_in(home.path(), github(1), true).unwrap();
    let mut firefox = github(2);
    firefox.browser = Some("org.mozilla.firefox".to_owned());
    assert!(add_in(home.path(), firefox, true).unwrap());
}

#[test]
fn what_the_helper_saw_carries_no_typed_text() {
    let canary = "hunter2-canary";
    let with_text =
        serde_json::json!({"app": "com.google.Chrome", "host": "github.com", "typed": canary});
    assert!(serde_json::from_value::<Seen>(with_text).is_err());

    let home = tempfile::tempdir().unwrap();
    add_in(home.path(), github(1), true).unwrap();
    let record = std::fs::read_to_string(home.path().join(RECORD)).unwrap();
    assert!(!record.contains(canary));
    assert!(!record.contains("login"));
}

#[test]
fn changes_made_at_once_are_all_kept() {
    let home = tempfile::tempdir().unwrap();
    let path = home.path().to_owned();
    let adders: Vec<_> = (0..16)
        .map(|index| {
            let home = path.clone();
            std::thread::spawn(move || {
                let mut entry = github(index);
                entry.id = format!("site{index}.example");
                add_in(&home, entry, true).unwrap()
            })
        })
        .collect();
    for adder in adders {
        assert!(adder.join().unwrap());
    }
    assert!(matches!(listed_in(&path), Listed::Accounts(accounts) if accounts.len() == 16));
}

#[test]
fn the_person_sets_an_accounts_level_and_an_agent_cannot() {
    let home = tempfile::tempdir().unwrap();
    add_in(home.path(), github(1), true).unwrap();
    let browser = Some("com.google.Chrome");
    let refused = set_level_in(
        home.path(),
        Kind::Site,
        "github.com",
        browser,
        Level::Trusted,
        false,
    );
    assert_eq!(refused.unwrap_err().code, Code::Stopped);
    assert!(
        set_level_in(
            home.path(),
            Kind::Site,
            "github.com",
            browser,
            Level::LookOnly,
            true
        )
        .unwrap()
    );
    assert!(
        !set_level_in(
            home.path(),
            Kind::Site,
            "gitlab.com",
            browser,
            Level::LookOnly,
            true
        )
        .unwrap()
    );
    let Listed::Accounts(accounts) = listed_in(home.path()) else {
        panic!("unread");
    };
    assert_eq!(accounts[0].level, Level::LookOnly);
}

use super::*;
use std::{os::unix::fs::PermissionsExt, path::PathBuf};

/// A canary folder made as the fixture's launcher makes one, in
/// `/private/tmp`, gone with its files when dropped.
struct Canary(PathBuf);

impl Canary {
    fn new() -> Self {
        let path = PathBuf::from(format!(
            "/private/tmp/atlas-canary.{}",
            &crate::files::token()[..12]
        ));
        std::fs::create_dir(&path).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        Self(path)
    }

    fn path(&self) -> &str {
        self.0.to_str().unwrap()
    }

    fn write(&self, name: &str, bytes: &[u8]) {
        std::fs::write(self.0.join(name), bytes).unwrap();
    }
}

impl Drop for Canary {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn pin() -> SessionPin {
    SessionPin {
        security_session_id: 42,
        display_id: 7,
    }
}

fn scan_of(attempt: &str, challenge: &str) -> Action {
    Action::Scan {
        attempt: attempt.to_owned(),
        challenge: challenge.to_owned(),
    }
}

/// What the fixture's panel observer writes: a scan of `attempt` that
/// carries `challenge` and `epoch` back, when it read them.
fn observed(attempt: &str, challenge: &str, epoch: &str) -> Vec<u8> {
    json!({"attemptID": attempt, "challenge": challenge, "epoch": epoch, "trusted": true,
        "scanComplete": true})
    .to_string()
    .into_bytes()
}

#[test]
fn a_native_scan_answers_only_with_the_fixtures_scan_of_this_challenge_in_this_epoch() {
    let canary = Canary::new();
    let challenge = crate::files::token();
    let folder = canary.0.clone();
    let asked = challenge.clone();
    // The fixture: once handed the challenge, a scan of another session
    // and one of another attempt, then the one asked for.
    let fixture = std::thread::spawn(move || {
        let handed = loop {
            if let Ok(bytes) = std::fs::read(folder.join(CHALLENGE)) {
                break serde_json::from_slice::<Value>(&bytes).unwrap();
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        let write = |bytes: Vec<u8>| {
            std::fs::write(folder.join(OBSERVED), bytes).unwrap();
            std::thread::sleep(Duration::from_millis(150));
        };
        write(observed(
            "8E3F2A10-4B7C-4D2E-9A61-3C5B7D9E0F12",
            &asked,
            "99-7",
        ));
        write(observed(
            "1D4C7B2A-6E5F-4A3B-8C9D-0E1F2A3B4C5D",
            &asked,
            handed["epoch"].as_str().unwrap(),
        ));
        write(observed(
            "8E3F2A10-4B7C-4D2E-9A61-3C5B7D9E0F12",
            &asked,
            handed["epoch"].as_str().unwrap(),
        ));
        handed
    });

    let answered = answer(
        &scan_of("8E3F2A10-4B7C-4D2E-9A61-3C5B7D9E0F12", &challenge),
        canary.path(),
        pin(),
    );
    let handed = fixture.join().unwrap();

    assert_eq!(handed, json!({"challenge": challenge, "epoch": "42-7"}));
    assert_eq!(answered["ok"], true, "{answered}");
    assert_eq!(
        answered["scan"]["attemptID"],
        "8E3F2A10-4B7C-4D2E-9A61-3C5B7D9E0F12"
    );
    assert_eq!(answered["scan"]["challenge"], challenge.as_str());
    assert_eq!(answered["scan"]["epoch"], "42-7");
}

#[test]
fn a_scan_that_does_not_come_within_its_bound_is_answered_late_never_with_an_older_scan() {
    let canary = Canary::new();
    let older = crate::files::token();
    canary.write(
        OBSERVED,
        &observed("8E3F2A10-4B7C-4D2E-9A61-3C5B7D9E0F12", &older, "42-7"),
    );
    let started = Instant::now();

    let late = answer(
        &scan_of(
            "8E3F2A10-4B7C-4D2E-9A61-3C5B7D9E0F12",
            &crate::files::token(),
        ),
        canary.path(),
        pin(),
    );

    assert!(started.elapsed() >= SCAN_WAIT, "{:?}", started.elapsed());
    assert!(started.elapsed() < SCAN_WAIT * 2, "{:?}", started.elapsed());
    assert_eq!(late["ok"], false, "{late}");
    assert_eq!(late["late"], true);
    assert_eq!(late["code"], "native");
    assert_eq!(
        late["error"],
        "The fixture's scan did not answer this check's challenge within 2 seconds."
    );
    assert!(late.get("scan").is_none(), "{late}");
}

#[test]
fn the_same_scan_read_again_never_passes_for_a_new_challenge() {
    let canary = Canary::new();
    let first = crate::files::token();
    canary.write(
        OBSERVED,
        &observed("8E3F2A10-4B7C-4D2E-9A61-3C5B7D9E0F12", &first, "42-7"),
    );

    let answered = answer(
        &scan_of("8E3F2A10-4B7C-4D2E-9A61-3C5B7D9E0F12", &first),
        canary.path(),
        pin(),
    );
    let again = answer(
        &scan_of(
            "8E3F2A10-4B7C-4D2E-9A61-3C5B7D9E0F12",
            &crate::files::token(),
        ),
        canary.path(),
        pin(),
    );

    assert_eq!(answered["scan"]["challenge"], first.as_str(), "{answered}");
    assert_eq!(again["late"], true, "{again}");
}

#[test]
fn a_native_read_refuses_a_folder_that_is_not_a_canary_folder_of_the_guests_own() {
    let shared = Canary::new();
    std::fs::set_permissions(&shared.0, std::fs::Permissions::from_mode(0o775)).unwrap();
    let target = Canary::new();
    target.write("state.json", b"{}");
    let linked = PathBuf::from(format!(
        "/private/tmp/atlas-canary.{}",
        &crate::files::token()[..12]
    ));
    std::os::unix::fs::symlink(&target.0, &linked).unwrap();

    let refused = [
        answer(&Action::Files, shared.path(), pin()),
        answer(&Action::Files, linked.to_str().unwrap(), pin()),
    ];
    let elsewhere = answer(&Action::Files, "/private/tmp/other.Ab12Cd", pin());
    let _ = std::fs::remove_file(&linked);

    for refused in refused {
        assert_eq!(refused["ok"], false, "{refused}");
        assert_eq!(refused["code"], "native", "{refused}");
        assert_eq!(
            refused["error"],
            "The canary folder is not one of this account's own that no one else may change."
        );
    }
    assert_eq!(elsewhere["code"], "bad-request", "{elsewhere}");
}

#[test]
fn a_canary_folders_fixed_files_are_read_whole_and_nothing_else() {
    let canary = Canary::new();
    canary.write("state.json", b"{\"pid\":7}");
    canary.write("atlas-fixture.txt", "e\u{301}".as_bytes());
    canary.write("challenge.json", b"{}");
    canary.write("secret.txt", b"SECRET");

    let read = answer(&Action::Files, canary.path(), pin());

    assert_eq!(read["ok"], true, "{read}");
    let files = read["files"].as_object().unwrap();
    assert_eq!(
        files.keys().collect::<Vec<_>>(),
        ["atlas-fixture.txt", "state.json"]
    );
    assert_eq!(
        STANDARD
            .decode(files["atlas-fixture.txt"].as_str().unwrap())
            .unwrap(),
        "e\u{301}".as_bytes()
    );
}

#[test]
fn a_canary_folders_file_that_is_a_link_a_fifo_or_too_large_is_refused() {
    let linked = Canary::new();
    std::fs::write("/private/tmp/atlas-native-test-outside", b"outside").unwrap();
    std::os::unix::fs::symlink(
        "/private/tmp/atlas-native-test-outside",
        linked.0.join("state.json"),
    )
    .unwrap();
    let fifo = Canary::new();
    nix::unistd::mkfifo(&fifo.0.join("events.jsonl"), nix::sys::stat::Mode::S_IRWXU).unwrap();
    let large = Canary::new();
    large.write("state.json", &vec![b' '; 600 * 1024]);
    large.write("fixture.log", &vec![b'x'; 600 * 1024]);

    let refused = [
        answer(&Action::Files, linked.path(), pin()),
        answer(&Action::Files, fifo.path(), pin()),
    ];
    let over = answer(&Action::Files, large.path(), pin());
    let _ = std::fs::remove_file("/private/tmp/atlas-native-test-outside");

    for refused in refused {
        assert_eq!(refused["code"], "native", "{refused}");
        assert!(
            refused["error"]
                .as_str()
                .unwrap()
                .ends_with("in the canary folder is not a plain file of this account's own."),
            "{refused}"
        );
    }
    assert_eq!(over["code"], "native", "{over}");
    assert_eq!(
        over["error"],
        "fixture.log in the canary folder could not be read within the 1 MiB a check reads."
    );
}

#[test]
fn only_a_folder_directly_in_private_tmp_named_as_a_canary_folder_is_one() {
    assert!(canary("/private/tmp/atlas-canary.Ab12Cd"));
    for directory in [
        "/tmp/atlas-canary.Ab12Cd",
        "/private/tmp/atlas-canary.",
        "/private/tmp/atlas-canary.x/nested",
        "/private/tmp/atlas-canary.a-b",
        "/private/tmp/atlas-canary.Ab/..",
        "/private/tmp//atlas-canary.Ab",
    ] {
        assert!(!canary(directory), "{directory}");
    }
    assert!(!canary(&format!(
        "/private/tmp/atlas-canary.{}",
        "a".repeat(65)
    )));
}

#[test]
fn a_matching_scan_that_comes_past_the_bound_is_answered_late() {
    let canary = Canary::new();
    let challenge = crate::files::token();
    let folder = canary.0.clone();
    let asked = challenge.clone();
    let fixture = std::thread::spawn(move || {
        std::thread::sleep(SCAN_WAIT + Duration::from_millis(200));
        std::fs::write(
            folder.join(OBSERVED),
            observed("8E3F2A10-4B7C-4D2E-9A61-3C5B7D9E0F12", &asked, "42-7"),
        )
        .unwrap();
    });

    let late = answer(
        &scan_of("8E3F2A10-4B7C-4D2E-9A61-3C5B7D9E0F12", &challenge),
        canary.path(),
        pin(),
    );
    fixture.join().unwrap();

    assert_eq!(late["late"], true, "{late}");
    assert!(late.get("scan").is_none(), "{late}");
}

#[test]
fn only_the_folder_itself_and_its_fixed_plain_files_resolve() {
    let canary = Canary::new();
    canary.write("atlas-fixture.txt", b"saved");
    std::fs::write("/private/tmp/atlas-native-resolve-outside", b"outside").unwrap();
    std::os::unix::fs::symlink(
        "/private/tmp/atlas-native-resolve-outside",
        canary.0.join("atlas-open-fixture.txt"),
    )
    .unwrap();
    let folder = canary.path().to_owned();
    let tmp = folder.replacen("/private/tmp/", "/tmp/", 1);
    let paths = vec![
        format!("{folder}/atlas-fixture.txt"),
        format!("{tmp}/atlas-fixture.txt"),
        folder.clone(),
        format!("{folder}/atlas-fixture.txt/"),
        format!("{folder}/atlas-open-fixture.txt"),
        "/private/tmp/atlas-native-resolve-outside".to_owned(),
        format!("{folder}/../atlas-native-resolve-outside"),
        format!("{folder}x/atlas-fixture.txt"),
        format!("{folder}/state.json"),
    ];

    let resolved: Vec<Value> = paths
        .chunks(MOST_PATHS)
        .flat_map(|paths| {
            let answered = answer(
                &Action::Resolve {
                    paths: paths.to_vec(),
                },
                canary.path(),
                pin(),
            );
            answered["resolved"].as_array().unwrap().clone()
        })
        .collect();
    let _ = std::fs::remove_file("/private/tmp/atlas-native-resolve-outside");

    assert_eq!(
        resolved,
        [
            json!("atlas-fixture.txt"),
            json!("atlas-fixture.txt"),
            json!(""),
            Value::Null,
            Value::Null,
            Value::Null,
            Value::Null,
            Value::Null,
            Value::Null,
        ],
        "{paths:#?}"
    );
}

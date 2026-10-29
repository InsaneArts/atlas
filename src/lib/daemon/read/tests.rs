use super::*;
use std::time::Duration;

fn uid() -> u32 {
    nix::unistd::getuid().as_raw()
}

/// A home of its own, as it really is.
fn home() -> (tempfile::TempDir, Places) {
    let dir = tempfile::tempdir().unwrap();
    let home = std::fs::canonicalize(dir.path()).unwrap();
    (
        dir,
        Places {
            home,
            temporary: None,
        },
    )
}

/// Write `bytes` at `name` in `home`, made long enough ago to be still.
fn still(places: &Places, name: &str, bytes: &[u8]) -> PathBuf {
    let path = places.home.join(name);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(&path, bytes).unwrap();
    let then = std::time::SystemTime::now() - Duration::from_secs(10);
    let file = File::options().write(true).open(&path).unwrap();
    file.set_modified(then).unwrap();
    // The change time follows any change; it is still once two seconds pass.
    path
}

fn request(path: &str, offset: u64, length: u64) -> ReadRequest {
    ReadRequest {
        path: path.into(),
        offset,
        length,
        expect: None,
        digest: false,
        max_size: 1 << 30,
        pin: None,
    }
}

/// Read `request` with no stillness wait: its reply and its bytes.
fn served(request: &ReadRequest, places: &Places) -> (Value, Vec<u8>) {
    let mut bytes = Vec::new();
    let reply = read_now(request, places, &|| Ok(()), &mut |event| {
        bytes.extend(STANDARD.decode(event["data"].as_str().unwrap()).unwrap());
        true
    });
    (reply, bytes)
}

thread_local! {
    /// How far this test's clock runs ahead of the real one.
    static AHEAD: std::cell::Cell<Duration> = const { std::cell::Cell::new(Duration::ZERO) };
}

/// Read `request` as of this test's clock.
fn read_now(
    request: &ReadRequest,
    places: &Places,
    guard: &dyn Fn() -> Result<(), Value>,
    events: &mut dyn FnMut(Value) -> bool,
) -> Value {
    let now = SystemTime::now() + AHEAD.with(std::cell::Cell::get);
    read_at(request, places, uid(), guard, events, now)
}

/// Let this test's clock run on until a file just written counts as still.
fn settle() {
    AHEAD.with(|ahead| ahead.set(ahead.get() + STILL + Duration::from_millis(100)));
}

/// The places of a helper named atlas, its own temporary folder `T`.
fn atlas() -> Places {
    Places {
        home: PathBuf::from("/Users/atlas"),
        temporary: Some(PathBuf::from("/private/var/folders/ab/cd/T")),
    }
}

#[test]
fn a_relative_path_starts_at_the_helpers_home() {
    let places = atlas();
    assert_eq!(
        locate("atlas-work/report.pdf", &places).unwrap(),
        Path::new("/Users/atlas/atlas-work/report.pdf")
    );
    assert_eq!(
        locate("~/atlas-work/a.bin", &places).unwrap(),
        Path::new("/Users/atlas/atlas-work/a.bin")
    );
    assert_eq!(
        locate("/Users/atlas/x.txt", &places).unwrap(),
        Path::new("/Users/atlas/x.txt")
    );
}

#[test]
fn the_two_system_links_are_read_as_their_private_paths() {
    let places = atlas();
    assert_eq!(
        locate("/tmp/out.png", &places).unwrap(),
        Path::new("/private/tmp/out.png")
    );
    assert_eq!(
        locate("/var/folders/ab/cd/T/x", &places).unwrap(),
        Path::new("/private/var/folders/ab/cd/T/x")
    );
}

#[test]
fn only_the_helpers_own_temporary_folder_is_read() {
    let places = atlas();
    for path in [
        "/var/folders/zz/yy/T/theirs",
        "/private/var/folders/ab/cd/C/cache",
        "/private/var/folders/ab/cd/T",
    ] {
        assert_eq!(locate(path, &places), Err(Refused::Outside), "{path}");
    }
    let unknown = Places {
        temporary: None,
        ..atlas()
    };
    assert_eq!(
        locate("/var/folders/ab/cd/T/x", &unknown),
        Err(Refused::Outside),
        "no account's when its own is not known"
    );
}

#[test]
fn the_helpers_own_temporary_folder_is_the_one_macos_names() {
    let own = own_temporary().unwrap();
    assert!(own.starts_with("/var/folders") || own.starts_with("/private/var/folders"));
    assert!(
        own.ends_with("T") || own.ends_with("T/"),
        "{}",
        own.display()
    );
}

#[test]
fn dots_and_paths_outside_the_allowed_folders_are_refused() {
    let places = atlas();
    for path in [
        "../p/secret",
        "atlas-work/../../p/x",
        "./x",
        "/etc/hosts",
        "/Users/p/x",
        "/private/tmp",
        ".atlas/helper/jobs/x.log",
        ".ATLAS/x",
    ] {
        assert_eq!(locate(path, &places), Err(Refused::Outside), "{path}");
    }
}

#[test]
fn protected_folders_are_refused_before_they_are_opened() {
    let places = atlas();
    for path in [
        "Documents/x.txt",
        "documents/x.txt",
        "Desktop/a.png",
        "Downloads/b.zip",
        "Library/Containers/com.apple.TextEdit/x",
        "Library/Preferences/x.plist",
        "Pictures/Photos Library.photoslibrary/database/Photos.sqlite",
        "Music/Music/Music Library.musiclibrary",
        "movies/TV/x",
        ".Trash/deleted.pdf",
        "/Users/atlas/LIBRARY/Mail/x",
        "De\u{17F}ktop/x.png",
        "Des\u{212A}top/x.png",
        "Document\u{17F}/x.txt",
        "Library\u{17F}/x",
    ] {
        let refused = locate(path, &places);
        let expected = if path.starts_with("Library\u{17F}") {
            Ok(PathBuf::from("/Users/atlas").join(path))
        } else {
            Err(Refused::Protected)
        };
        assert_eq!(refused, expected, "{path}");
    }
}

#[test]
fn a_spelling_the_disk_takes_for_a_guarded_folder_is_refused() {
    let (_dir, places) = home();
    std::fs::create_dir(places.home.join("Desktop")).unwrap();
    std::fs::create_dir(places.home.join("Kits")).unwrap();
    still(&places, "Desktop/secret.txt", b"s");
    // The disk folds the long s and the Kelvin sign as APFS does when it
    // compares names in any case; either way the guarded folder is refused.
    for path in [
        "De\u{17F}ktop/secret.txt",
        "Des\u{212A}top/secret.txt",
        "DESKTOP/secret.txt",
    ] {
        let (reply, bytes) = served(&request(path, 0, 1), &places);
        assert_eq!(reply["reason"], "protected", "{path}: {reply}");
        assert!(bytes.is_empty());
    }
    // A folder that is not guarded, whatever its spelling, is looked up as
    // what it is.
    assert!(locate("\u{212A}its/x", &places).is_ok());
}

#[test]
fn a_link_anywhere_in_the_path_is_refused() {
    let (_dir, places) = home();
    still(&places, "work/blob.bin", b"blob");
    std::os::unix::fs::symlink("blob.bin", places.home.join("work/link.bin")).unwrap();
    std::os::unix::fs::symlink("work", places.home.join("linked")).unwrap();
    settle();
    for path in ["work/link.bin", "linked/blob.bin"] {
        let (reply, _) = served(&request(path, 0, 16), &places);
        assert_eq!(reply["reason"], "symlink", "{path}: {reply}");
    }
}

#[test]
fn only_regular_files_are_read() {
    let (_dir, places) = home();
    std::fs::create_dir(places.home.join("folder")).unwrap();
    nix::unistd::mkfifo(&places.home.join("pipe"), nix::sys::stat::Mode::S_IRWXU).unwrap();
    settle();
    let started = std::time::Instant::now();
    for path in ["folder", "pipe"] {
        let (reply, _) = served(&request(path, 0, 16), &places);
        assert_eq!(reply["reason"], "not-regular", "{path}: {reply}");
    }
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "a pipe never waits"
    );
}

#[test]
fn another_owners_file_is_refused() {
    let (_dir, places) = home();
    let path = still(&places, "theirs.bin", b"x");
    settle();
    let metadata = std::fs::metadata(&path).unwrap();
    assert_eq!(
        readable(
            &metadata,
            &path,
            &places.roots(),
            uid() + 1,
            None,
            1 << 20,
            SystemTime::now()
        ),
        Err(Refused::Owner)
    );
}

#[test]
fn a_file_over_its_limit_is_refused_before_it_is_read() {
    let (_dir, places) = home();
    still(&places, "big.bin", &[7_u8; 4096]);
    settle();
    let mut asked = request("big.bin", 0, 4096);
    asked.max_size = 4095;
    let (reply, bytes) = served(&asked, &places);
    assert_eq!(reply["reason"], "too-large", "{reply}");
    assert!(bytes.is_empty());
}

#[test]
fn a_file_written_in_the_last_two_seconds_is_refused() {
    let (_dir, places) = home();
    std::fs::write(places.home.join("fresh.log"), b"new").unwrap();
    let (reply, bytes) = served(&request("fresh.log", 0, 16), &places);
    assert_eq!(reply["reason"], "changing", "{reply}");
    assert!(bytes.is_empty());
}

#[test]
fn a_file_replaced_between_parts_is_refused() {
    let (_dir, places) = home();
    still(&places, "out.bin", &[1_u8; 100]);
    settle();
    let (first, _) = served(&request("out.bin", 0, 10), &places);
    assert_eq!(first["ok"], true, "{first}");
    let identity: FileIdentity = serde_json::from_value(json!({
        "dev": first["file"]["dev"], "ino": first["file"]["ino"], "size": first["file"]["size"],
        "mtime": first["file"]["mtime"], "ctime": first["file"]["ctime"],
    }))
    .unwrap();

    std::fs::remove_file(places.home.join("out.bin")).unwrap();
    still(&places, "out.bin", &[1_u8; 100]);
    settle();
    let mut next = request("out.bin", 10, 10);
    next.expect = Some(identity);
    let (reply, _) = served(&next, &places);
    assert_eq!(reply["reason"], "changing", "{reply}");
}

#[test]
fn a_file_that_grows_between_parts_is_refused() {
    let (_dir, places) = home();
    let path = still(&places, "grow.bin", &[1_u8; 100]);
    settle();
    let (first, _) = served(&request("grow.bin", 0, 10), &places);
    let identity: FileIdentity = serde_json::from_value(json!({
        "dev": first["file"]["dev"], "ino": first["file"]["ino"], "size": first["file"]["size"],
        "mtime": first["file"]["mtime"], "ctime": first["file"]["ctime"],
    }))
    .unwrap();
    use std::io::Write;
    File::options()
        .append(true)
        .open(&path)
        .unwrap()
        .write_all(b"more")
        .unwrap();
    let mut next = request("grow.bin", 10, 10);
    next.expect = Some(identity);
    let (reply, _) = served(&next, &places);
    assert_eq!(reply["reason"], "changing", "{reply}");
}

#[test]
fn parts_arrive_as_exact_bytes() {
    let (_dir, places) = home();
    let bytes: Vec<u8> = (0..(CHUNK * 2 + 777))
        .map(|n| (n * 31 % 256) as u8)
        .collect();
    still(&places, "blob.bin", &bytes);
    settle();
    let (reply, got) = served(&request("blob.bin", 5, bytes.len() as u64), &places);
    assert_eq!(reply["ok"], true, "{reply}");
    assert_eq!(reply["length"], bytes.len() - 5);
    assert_eq!(got, bytes[5..]);
    assert_eq!(
        reply["file"]["path"],
        json!(places.home.join("blob.bin")),
        "the path it really has"
    );
}

#[test]
fn the_digest_is_of_the_whole_file_as_it_stands() {
    let (_dir, places) = home();
    let bytes = vec![0_u8, 255, 10, 13, 0];
    still(&places, "zero.bin", &bytes);
    settle();
    let mut asked = request("zero.bin", 0, 0);
    asked.digest = true;
    let (reply, got) = served(&asked, &places);
    assert!(got.is_empty());
    let expected = crate::name::hex(&Sha256::digest(&bytes));
    assert_eq!(reply["sha256"], expected, "{reply}");
}

#[test]
fn a_missing_file_says_so() {
    let (_dir, places) = home();
    let (reply, _) = served(&request("nothing-here", 0, 1), &places);
    assert_eq!(reply["reason"], "missing");
    assert_eq!(reply["code"], "collect");
    assert_eq!(reply["uncertain"], false);
}

#[test]
fn nothing_is_opened_or_sent_once_the_guard_refuses() {
    let (_dir, places) = home();
    let bytes: Vec<u8> = vec![7; CHUNK * 3];
    still(&places, "big.bin", &bytes);
    settle();
    let refusal =
        || json!({"ok": false, "code": "on-console", "error": "on screen", "uncertain": false});

    let mut sent = 0;
    let reply = read_now(
        &request("big.bin", 0, bytes.len() as u64),
        &places,
        &|| Err(refusal()),
        &mut |_| {
            sent += 1;
            true
        },
    );
    assert_eq!(reply, refusal());
    assert_eq!(sent, 0, "refused before it was opened");

    // The helper comes on screen after the first part: no more are read.
    let asked = std::cell::Cell::new(0);
    let guard = || {
        asked.set(asked.get() + 1);
        if asked.get() > 2 {
            Err(refusal())
        } else {
            Ok(())
        }
    };
    let mut sent = 0;
    let reply = read_now(
        &request("big.bin", 0, bytes.len() as u64),
        &places,
        &guard,
        &mut |_| {
            sent += 1;
            true
        },
    );
    assert_eq!(reply, refusal());
    assert_eq!(sent, 1);

    let mut digest = request("big.bin", 0, 0);
    digest.digest = true;
    let asked = std::cell::Cell::new(0);
    let guard = || {
        asked.set(asked.get() + 1);
        if asked.get() > 1 {
            Err(refusal())
        } else {
            Ok(())
        }
    };
    let reply = read_now(&digest, &places, &guard, &mut |_| true);
    assert_eq!(reply, refusal(), "nor is it hashed");
}

#[test]
fn a_digest_stops_once_the_helper_may_no_longer_be_read() {
    let (_dir, places) = home();
    still(&places, "big.bin", &vec![9_u8; 3 * 1024 * 1024 + 5]);
    settle();
    let mut asked = request("big.bin", 0, 0);
    asked.digest = true;
    let checks = std::cell::Cell::new(0);
    let guard = || {
        checks.set(checks.get() + 1);
        if checks.get() <= 3 {
            Ok(())
        } else {
            Err(failure("on-console", "The helper is on the screen"))
        }
    };
    let reply = read_now(&asked, &places, &guard, &mut |_| true);
    assert_eq!(reply["code"], "on-console", "{reply}");
    assert!(reply.get("sha256").is_none());
    assert!(
        checks.get() < 8,
        "it stopped partway through: {}",
        checks.get()
    );
}

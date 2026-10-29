use super::*;
use crate::{
    daemon::{
        protocol::{self, Request},
        read::{self, Places},
    },
    doing::Doing,
    host_session::{Host, HostSession},
    task::Task,
};
use std::{
    os::unix::fs::PermissionsExt,
    sync::{Arc, Mutex},
    time::SystemTime,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::UnixListener,
};

/// The helper's home and the person's output folder, both private.
struct Place {
    _dir: tempfile::TempDir,
    home: PathBuf,
    client: Client,
    /// Whether the stand-in gives every digest wrong.
    lie: Arc<Mutex<bool>>,
    /// How far the stand-in's clock runs ahead of the real one.
    ahead: Arc<Mutex<Duration>>,
}

impl Place {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let base = fs::canonicalize(dir.path()).unwrap();
        let home = base.join("helper");
        fs::create_dir(&home).unwrap();
        let mut client = crate::test_support::client_in(&base).doing(Doing::new("collect"));
        client.transport.peer_uid = nix::unistd::getuid().as_raw();
        let host = HostSession::new(Host::Codex, "collect-test").unwrap();
        client.task = Some(Task::of(host.key(), None).unwrap());
        client.host_session = Some(host);
        Self {
            _dir: dir,
            home,
            client,
            lie: Arc::default(),
            ahead: Arc::default(),
        }
    }

    /// Serve reads of the helper's home at the client's socket, as the
    /// daemon does; `asked` counts the requests.
    fn serve(&self) -> Arc<Mutex<usize>> {
        let listener = UnixListener::bind(&self.client.config.socket_path).unwrap();
        let home = self.home.clone();
        let lie = *self.lie.lock().unwrap();
        let ahead = self.ahead.clone();
        let asked = Arc::new(Mutex::new(0));
        let counted = asked.clone();
        tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                *counted.lock().unwrap() += 1;
                let home = home.clone();
                let ahead = ahead.clone();
                tokio::spawn(async move {
                    let (reader, mut writer) = stream.into_split();
                    let mut line = String::new();
                    BufReader::new(reader).read_line(&mut line).await.unwrap();
                    let Ok(Request::Read(request)) = protocol::parse(line.trim_end().as_bytes())
                    else {
                        panic!("only reads are asked for: {line}");
                    };
                    let mut lines = Vec::new();
                    let places = Places {
                        home,
                        temporary: None,
                    };
                    let now = SystemTime::now() + *ahead.lock().unwrap();
                    let reply = read::read_at(
                        &request,
                        &places,
                        nix::unistd::getuid().as_raw(),
                        &|| Ok(()),
                        &mut |event| {
                            lines.push(event);
                            true
                        },
                        now,
                    );
                    let mut reply = reply;
                    if lie && reply["sha256"].is_string() {
                        reply["sha256"] = json!("0".repeat(64));
                    }
                    lines.push(reply);
                    for line in lines {
                        writer
                            .write_all(format!("{line}\n").as_bytes())
                            .await
                            .unwrap();
                    }
                });
            }
        });
        asked
    }

    /// Write `bytes` at `name` in the helper's home, still for a while.
    fn file(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let path = self.home.join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&path, bytes).unwrap();
        File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(SystemTime::now() - Duration::from_secs(60))
            .unwrap();
        path
    }

    /// Write `bytes` at `name` in the helper's home, just now as the
    /// stand-in's clock says.
    fn fresh(&self, name: &str, bytes: &[u8]) {
        let path = self.home.join(name);
        fs::write(&path, bytes).unwrap();
        File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(SystemTime::now() + *self.ahead.lock().unwrap())
            .unwrap();
    }

    /// Let the stand-in's clock run on until files just written count as
    /// still.
    fn settle(&self) {
        *self.ahead.lock().unwrap() += Duration::from_millis(2100);
    }

    async fn collect(&self, files: &[&str], max_size: u64) -> Value {
        let files: Vec<String> = files.iter().map(|file| (*file).to_owned()).collect();
        collected(&self.client, None, &files, max_size, Instant::now())
            .await
            .unwrap()
    }
}

fn random(bytes: usize) -> Vec<u8> {
    let mut state = 0x2545_f491_4f6c_dd1d_u64;
    (0..bytes)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (state >> 24) as u8
        })
        .collect()
}

fn sha256(bytes: &[u8]) -> String {
    crate::name::hex(&Sha256::digest(bytes))
}

#[tokio::test]
async fn a_binary_file_comes_back_byte_for_byte() {
    let place = Place::new();
    place.serve();
    let bytes = random(20 * 1024 * 1024 + 123);
    place.file("atlas-work/blob.bin", &bytes);
    place.settle();
    let reply = place.collect(&["atlas-work/blob.bin"], 64 << 20).await;
    assert_eq!(reply["ok"], true, "{reply}");
    let kept = &reply["collected"][0];
    assert_eq!(kept["size"], bytes.len());
    assert_eq!(kept["sha256"], sha256(&bytes));
    let copy = PathBuf::from(kept["path"].as_str().unwrap());
    assert_eq!(fs::read(&copy).unwrap(), bytes);
    assert_eq!(
        kept["source"],
        json!(place.home.join("atlas-work/blob.bin"))
    );
    assert_eq!(kept["inHome"], "atlas-work/blob.bin");
}

#[tokio::test]
async fn results_name_paths_sizes_and_digests_never_contents() {
    let place = Place::new();
    place.serve();
    place.file("a.txt", b"secret words");
    place.settle();
    let reply = place.collect(&["a.txt"], 1024).await;
    assert!(!reply.to_string().contains("secret words"));
    let mut keys: Vec<_> = reply["collected"][0]
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect();
    keys.sort();
    assert_eq!(keys, ["inHome", "path", "read", "sha256", "size", "source"]);
    let read = &reply["collected"][0]["read"];
    assert!(read["started"].as_u64().unwrap() <= read["finished"].as_u64().unwrap());
}

#[tokio::test]
async fn a_copy_is_private_and_never_executable() {
    let place = Place::new();
    place.serve();
    let source = place.file("tool.sh", b"#!/bin/sh\n");
    fs::set_permissions(&source, fs::Permissions::from_mode(0o755)).unwrap();
    File::options()
        .write(true)
        .open(&source)
        .unwrap()
        .set_modified(SystemTime::now() - Duration::from_secs(60))
        .unwrap();
    place.settle();
    let reply = place.collect(&["tool.sh"], 1024).await;
    let copy = PathBuf::from(reply["collected"][0]["path"].as_str().unwrap());
    assert_eq!(fs::metadata(&copy).unwrap().mode() & 0o777, 0o600);
    for folder in copy.ancestors().skip(1).take(4) {
        assert_eq!(
            fs::metadata(folder).unwrap().mode() & 0o777,
            0o700,
            "{}",
            folder.display()
        );
    }
}

#[tokio::test]
async fn two_files_of_one_name_in_one_call_never_overwrite() {
    let place = Place::new();
    place.serve();
    place.file("a/report.bin", b"first");
    place.file("b/report.bin", b"second");
    place.file("c/Report.bin", b"third");
    place.settle();
    let reply = place
        .collect(&["a/report.bin", "b/report.bin", "c/Report.bin"], 1024)
        .await;
    assert_eq!(reply["ok"], false);
    assert_eq!(reply["collected"].as_array().unwrap().len(), 1);
    let copy = PathBuf::from(reply["collected"][0]["path"].as_str().unwrap());
    assert_eq!(fs::read(copy).unwrap(), b"first");
    assert_eq!(reply["failed"][0]["reason"], "exists");
    assert_eq!(reply["failed"][1]["reason"], "exists");
    assert_eq!(reply["error"], "2 of 3 files were not collected");
}

#[tokio::test]
async fn a_failed_file_leaves_the_others_collected() {
    let place = Place::new();
    place.serve();
    place.file("kept.bin", b"kept");
    place.settle();
    place.fresh("fresh.log", b"new");
    let reply = place
        .collect(&["kept.bin", "fresh.log", "gone.bin"], 1024)
        .await;
    assert_eq!(reply["collected"].as_array().unwrap().len(), 1);
    assert_eq!(reply["failed"][0]["reason"], "changing");
    assert!(
        reply["failed"][0]["error"]
            .as_str()
            .unwrap()
            .contains("wait until whatever writes it is done")
    );
    assert_eq!(reply["failed"][1]["reason"], "missing");
    assert_eq!(reply["uncertain"], false);
    let folder = PathBuf::from(reply["collected"][0]["path"].as_str().unwrap());
    let receipt = folder.parent().unwrap().parent().unwrap();
    let names: Vec<String> = fs::read_dir(receipt)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert!(
        names.iter().all(|name| !name.ends_with(".partial")),
        "{names:?}"
    );
}

#[tokio::test]
async fn a_file_over_its_limit_is_refused_and_keeps_nothing() {
    let place = Place::new();
    place.serve();
    place.file("big.bin", &[1_u8; 5000]);
    place.settle();
    let reply = place.collect(&["big.bin"], 4096).await;
    assert_eq!(reply["failed"][0]["reason"], "too-large");
    assert_eq!(reply["receipt"], Value::Null);
    let collected = place.client.output_dir().unwrap().join(COLLECTED);
    let left: Vec<_> = walk(&collected);
    assert!(left.is_empty(), "{left:?}");
}

/// Every file under `folder`.
fn walk(folder: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for entry in fs::read_dir(folder).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir() {
            files.extend(walk(&path));
        } else {
            files.push(path);
        }
    }
    files
}

#[tokio::test]
async fn a_digest_that_does_not_match_keeps_nothing() {
    let place = Place::new();
    *place.lie.lock().unwrap() = true;
    place.serve();
    place.file("a.bin", b"abc");
    place.settle();
    let reply = place.collect(&["a.bin"], 1024).await;
    assert_eq!(reply["failed"][0]["reason"], "changing", "{reply}");
    assert_eq!(reply["receipt"], Value::Null);
    let collected = place.client.output_dir().unwrap().join(COLLECTED);
    assert!(walk(&collected).is_empty(), "no partial and no file");
}

#[tokio::test]
async fn a_path_in_your_own_home_is_refused_before_it_is_sent() {
    let place = Place::new();
    let asked = place.serve();
    let home = crate::config::home().unwrap();
    let mine = home.join("Desktop/x.txt").to_string_lossy().into_owned();
    for path in [mine.as_str(), "../x", "a/../../b"] {
        let error = collect(&place.client, &[path.to_owned()], 1024)
            .await
            .unwrap_err();
        assert_eq!(error.code, "collect", "{path}");
        assert_eq!(error.details["reason"], "outside");
    }
    assert!(
        collect(&place.client, std::slice::from_ref(&mine), 1024)
            .await
            .unwrap_err()
            .error
            .contains("your shell turns ~ into your own home")
    );
    assert_eq!(*asked.lock().unwrap(), 0, "nothing was sent");
}

#[tokio::test]
async fn a_linked_receipt_folder_is_refused() {
    let place = Place::new();
    place.serve();
    place.file("a.bin", b"a");
    place.settle();
    let output = place.client.output_dir().unwrap();
    let elsewhere = output.parent().unwrap().join("elsewhere");
    fs::create_dir(&elsewhere).unwrap();
    std::os::unix::fs::symlink(&elsewhere, output.join(COLLECTED)).unwrap();
    let files = ["a.bin".to_owned()];
    let error = collected(&place.client, None, &files, 1024, Instant::now())
        .await
        .unwrap_err();
    assert_eq!(error.code, "state");
    assert!(fs::read_dir(&elsewhere).unwrap().next().is_none());
}

/// Make `path` look an hour and a half old, not following a link.
fn aged(path: &Path) {
    let then = nix::sys::time::TimeVal::new(
        (SystemTime::now() - Duration::from_secs(90 * 60))
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64,
        0,
    );
    nix::sys::stat::lutimes(path, &then, &then).unwrap();
}

/// A stale partial file at `folder`/`.0.partial`, and the folder, aged.
fn stale_partial(folder: &Path) -> PathBuf {
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(folder)
        .unwrap();
    let partial = folder.join(".0.partial");
    fs::write(&partial, b"half").unwrap();
    aged(&partial);
    aged(folder);
    partial
}

#[tokio::test]
async fn nothing_is_scavenged_through_a_linked_collected_folder() {
    let place = Place::new();
    place.serve();
    place.file("a.bin", b"a");
    place.settle();
    let output = place.client.output_dir().unwrap();
    let elsewhere = output.parent().unwrap().join("elsewhere");
    let partial = stale_partial(&elsewhere.join("unlabelled/1-a1b2c3"));
    let other = elsewhere.join("unlabelled/1-a1b2c3/keep.txt");
    fs::write(&other, b"keep").unwrap();
    aged(&other);
    fs::set_permissions(&elsewhere, fs::Permissions::from_mode(0o700)).unwrap();
    std::os::unix::fs::symlink(&elsewhere, output.join(COLLECTED)).unwrap();

    let files = ["a.bin".to_owned()];
    let error = collected(&place.client, None, &files, 1024, Instant::now())
        .await
        .unwrap_err();
    assert_eq!(error.code, "state");
    assert!(
        partial.exists() && other.exists(),
        "nothing outside was touched"
    );
}

#[tokio::test]
async fn abandoned_parts_are_removed_and_nothing_through_a_link_or_misnamed_is() {
    let place = Place::new();
    place.serve();
    place.file("a.bin", b"a");
    place.settle();
    let output = place.client.output_dir().unwrap().to_owned();
    let root = output.join(COLLECTED);
    fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
    let key = place.client.task.as_ref().unwrap().key().to_string();

    // A receipt's folder with its receipt: only its stale partial goes.
    let kept = root.join(&key).join("1-aaaaaa");
    let stale = stale_partial(&kept);
    fs::write(kept.join("receipt.json"), b"{}").unwrap();
    let fresh = kept.join(".1.partial");
    fs::write(&fresh, b"fresh").unwrap();
    aged(&kept);

    // A folder a killed collection left without its receipt goes whole.
    let left = root.join(&key).join("2-bbbbbb");
    stale_partial(&left);
    fs::DirBuilder::new()
        .mode(0o700)
        .create(left.join("files"))
        .unwrap();
    let copy = left.join("files/report.pdf");
    fs::write(&copy, b"pdf").unwrap();
    aged(&copy);
    aged(&left.join("files"));
    aged(&left);

    // Neither a folder Atlas does not name so, nor one reached by a link.
    let misnamed = stale_partial(&root.join(&key).join("not-a-receipt"));
    let elsewhere = output.parent().unwrap().join("elsewhere");
    let outside = stale_partial(&elsewhere.join("3-cccccc"));
    fs::set_permissions(&elsewhere, fs::Permissions::from_mode(0o700)).unwrap();
    std::os::unix::fs::symlink(&elsewhere, root.join("000000000000")).unwrap();
    aged(&root.join(&key));

    let reply = place.collect(&["a.bin"], 1024).await;
    assert_eq!(reply["ok"], true, "{reply}");
    assert!(!stale.exists(), "a stale part goes");
    assert!(fresh.exists(), "a fresh one stays");
    assert!(kept.join("receipt.json").exists());
    assert!(!left.exists(), "an unreported collection goes");
    assert!(misnamed.exists(), "a folder Atlas does not name is left");
    assert!(outside.exists(), "a link is never followed");
}

#[tokio::test]
async fn an_older_daemon_is_refused_before_anything_is_read() {
    use base64::{Engine, engine::general_purpose::STANDARD};

    let place = Place::new();
    let listener = UnixListener::bind(&place.client.config.socket_path).unwrap();
    let asked: Arc<Mutex<Vec<Value>>> = Arc::default();
    let heard = asked.clone();
    tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            let (reader, mut writer) = stream.into_split();
            let mut line = String::new();
            BufReader::new(reader).read_line(&mut line).await.unwrap();
            let request: Value = serde_json::from_str(&line).unwrap();
            heard.lock().unwrap().push(request.clone());
            let proof = json!({"ok": true, "uid": 60001, "hasGraphicAccess": true,
                "parentPID": 123, "onConsole": false, "securitySessionID": 42,
                "displayID": 7, "width": 100, "height": 100});
            let lines = match request["op"].as_str() {
                Some("hello") => vec![json!({"ok": true, "op": "hello",
                    "user": {"uid": 60001, "name": "atlas", "home": "/Users/atlas"},
                    "session": {"managerName": "Aqua", "onConsole": false},
                    "display": {"width": 100, "height": 100, "scale": 1},
                    "permissions": {"screenCapture": true, "accessibility": true},
                    "daemon": {"protocol": 1, "pid": 123, "capabilities": ["session-pin"]}})],
                Some("run") => vec![
                    json!({"event": "output", "data": STANDARD.encode(proof.to_string())}),
                    json!({"ok": true, "exitCode": 0, "timedOut": false, "signal": null,
                        "durationMs": 1}),
                ],
                _ => vec![
                    json!({"ok": false, "code": "bad-request", "error": "unknown op",
                    "uncertain": false}),
                ],
            };
            for line in lines {
                writer
                    .write_all(format!("{line}\n").as_bytes())
                    .await
                    .unwrap();
            }
        }
    });

    let error = collect(&place.client, &["a.bin".to_owned()], 1024)
        .await
        .unwrap_err();
    assert_eq!(error.code, "daemon", "{error:?}");
    assert!(error.error.contains("run atlas restart"));
    assert!(
        !asked
            .lock()
            .unwrap()
            .iter()
            .any(|request| request["op"] == "read"),
        "nothing was read"
    );
}

#[tokio::test]
async fn every_collection_has_its_own_folder() {
    let place = Place::new();
    place.serve();
    place.file("a.bin", b"a");
    place.settle();
    let first = place.collect(&["a.bin"], 1024).await;
    let second = place.collect(&["a.bin"], 1024).await;
    assert_ne!(first["receipt"], second["receipt"]);
    assert_ne!(
        first["collected"][0]["path"],
        second["collected"][0]["path"]
    );
    let record = Store::at(place.client.output_dir().unwrap())
        .load(place.client.task.as_ref().unwrap().key())
        .unwrap();
    assert_eq!(record.outputs.len(), 2);
    assert_eq!(record.outputs[0].files, 1);
    assert!(
        !serde_json::to_string(&record).unwrap().contains("a.bin"),
        "the task's record names no file"
    );
}

#[tokio::test]
async fn a_kept_copy_that_changed_is_not_intact() {
    let place = Place::new();
    place.serve();
    place.file("a.bin", b"aaaa");
    place.file("b.bin", b"bbbb");
    place.file("c.bin", b"cccc");
    place.settle();
    let reply = place.collect(&["a.bin", "b.bin", "c.bin"], 1024).await;
    let receipt = ReceiptId::parse(reply["receipt"].as_str().unwrap()).unwrap();
    let output = place.client.output_dir().unwrap();
    let task = place.client.task.as_ref().unwrap().key();
    assert!(intact(output, Some(task), &receipt).whole());

    let path = |n: usize| PathBuf::from(reply["collected"][n]["path"].as_str().unwrap());
    fs::write(path(0), b"aaab").unwrap();
    fs::remove_file(path(1)).unwrap();
    let found = intact(output, Some(task), &receipt);
    assert_eq!(found.changed.len(), 1);
    assert!(found.changed[0].source.ends_with("a.bin"));
    assert_eq!(found.missing.len(), 1);
    assert!(found.missing[0].source.ends_with("b.bin"));
    assert_eq!(found.kept.len(), 1);

    let other = ReceiptId::parse("1-abcdef").unwrap();
    assert!(intact(output, Some(task), &other).unknown);
}

#[tokio::test]
async fn collecting_writes_no_file_names_to_the_activity_log() {
    let place = Place::new();
    place.serve();
    place.file("private-name.bin", b"xyz");
    place.settle();
    place.collect(&["private-name.bin"], 1024).await;
    let log =
        fs::read_to_string(place.client.output_dir().unwrap().join("metadata.jsonl")).unwrap();
    assert!(!log.contains("private-name"), "{log}");
    let row: Value = serde_json::from_str(log.lines().last().unwrap()).unwrap();
    assert_eq!(row["operation"], "collect");
    assert_eq!(row["count"], 1);
    assert_eq!(row["bytes"], 3);
    assert_eq!(crate::doing::says(&row), "collected 1 file");
}

#[test]
fn receipts_are_exact() {
    assert!(ReceiptId::parse("1761650000123-a1b2c3").is_some());
    for text in [
        "1761650000123-A1B2C3",
        "x-a1b2c3",
        "1-a1b2c",
        "1-a1b2c3d",
        "-a1b2c3",
        "1",
    ] {
        assert!(ReceiptId::parse(text).is_none(), "{text}");
    }
    let made = ReceiptId::new(5);
    assert!(made.as_str().starts_with("5-"));
    assert!(ReceiptId::parse(made.as_str()).is_some());
}

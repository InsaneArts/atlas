//! A VM's environment end to end, through a real keeper and a real client:
//! plans frozen here and staged into a stand-in guest whose worker is
//! Atlas's own, carrying out each request against a home of the test's
//! own on a disk of the test's own, where the guest's copies are placed as
//! the guest names them.
use super::tests::{copy, stage_lanes};
use crate::{
    agent::Closing,
    client::Client,
    doing::Doing,
    environment::{
        NO_PROFILE, WORKER_REFUSED,
        model::{Packet, Request},
        worker::carry_out_in,
    },
    host_session::{Host, HostSession},
    vm::{
        copies::{Owned, Owners},
        keeper::{
            change::tests::{admitted, live, proven, record, restarted},
            clients::tests::{Serving, asked, client_of, serving},
            tests::{Saying, until},
        },
    },
};
use atlas_core::{
    error::{Code, Error},
    files::Token,
    protocol::{JobReply, Showing},
};
use atlas_session::{
    lane::{Header, LaneKind},
    standin::{StandIn, copies::ROOT},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering::SeqCst},
    },
    time::Duration,
};

/// The guest's account, as its proof names it, and its home.
const GUEST_UID: u32 = 502;
pub(super) const GUEST_HOME: &str = "/Users/guest";

/// How the guest's worker ends a request it carried out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Ending {
    /// Answered as the guest's daemon answers it.
    Answered,
    /// Ended once it applied its first file: no answer, and nothing kept.
    Killed,
    /// Carried out whole, then nothing said and nothing kept, as when its
    /// daemon cannot tell what it came to.
    Unanswered,
    /// Carried out whole and kept as what it came to, its answer never
    /// reaching the station.
    Kept,
}

/// The guest's disk as its worker sees it: the copies staged into it,
/// placed where the guest names them, and its account's home, which is
/// this test's own account on a folder of the test's own.
pub(super) struct Disk {
    _root: tempfile::TempDir,
    /// Where the guest's copies are, on this disk.
    open: PathBuf,
    /// Where the guest's home is, on this disk.
    pub(super) home: PathBuf,
    standin: Arc<StandIn>,
    /// Every environment request the guest was asked, as it came.
    pub(super) asked: Mutex<Vec<Value>>,
    /// How the worker's requests end.
    pub(super) ending: Mutex<Ending>,
    /// Run once a request was carried out and ended as it ends, before its
    /// answer, if any, is said.
    pub(super) during: Mutex<Option<Box<dyn Fn() + Send + Sync>>>,
    /// While set, a preview's answer is padded past what one request holds.
    inflating: AtomicBool,
}

impl Disk {
    fn new(standin: Arc<StandIn>) -> Self {
        let root = tempfile::tempdir().unwrap();
        let path = fs::canonicalize(root.path()).unwrap();
        let (open, home) = (path.join("open"), path.join("home"));
        fs::create_dir(&open).unwrap();
        fs::create_dir(&home).unwrap();
        Self {
            _root: root,
            open,
            home,
            standin,
            asked: Mutex::default(),
            ending: Mutex::new(Ending::Answered),
            during: Mutex::default(),
            inflating: AtomicBool::new(false),
        }
    }

    /// Every packet the worker was handed, as the station sent it.
    pub(super) fn packets(&self) -> Vec<Value> {
        self.asked
            .lock()
            .unwrap()
            .iter()
            .map(|asked| asked["packet"].clone())
            .collect()
    }

    /// Place every copy the stand-in keeps where the guest names it, file
    /// by file as its manifest says, and take away any it no longer keeps.
    fn place(&self) {
        let copies = self.standin.copies().clone();
        for entry in fs::read_dir(&self.open).unwrap().flatten() {
            let kept = entry
                .file_name()
                .to_str()
                .and_then(Token::parse)
                .is_some_and(|token| copies.contains_key(&token));
            if !kept {
                fs::remove_dir_all(entry.path()).unwrap();
            }
        }
        for (token, copy) in copies {
            let base = self.open.join(token.as_str()).join(&copy.name);
            let mut at = 0;
            for (entry, size, _) in copy.snapshot.files() {
                let path = entry
                    .path
                    .iter()
                    .fold(base.clone(), |path, part| path.join(part));
                fs::create_dir_all(path.parent().unwrap()).unwrap();
                let size = usize::try_from(size).unwrap();
                fs::write(&path, &copy.contents[at..at + size]).unwrap();
                at += size;
            }
        }
    }

    /// `value` as this disk names its paths, and its account as this
    /// test's own.
    fn onto(&self, value: &Value) -> Value {
        let text = value
            .to_string()
            .replace(ROOT, self.open.to_str().unwrap())
            .replace(GUEST_HOME, self.home.to_str().unwrap());
        let mut value: Value = serde_json::from_str(&text).unwrap();
        let own = nix::unistd::getuid().as_raw();
        value["expected"]["uid"] = json!(own);
        if value["request"]["plan"].is_object() {
            value["request"]["plan"]["uid"] = json!(own);
        }
        value
    }

    /// `value`, the worker's answer, as the guest names its paths and its
    /// account.
    fn off(&self, value: &Value) -> Value {
        let text = value
            .to_string()
            .replace(self.open.to_str().unwrap(), ROOT)
            .replace(self.home.to_str().unwrap(), GUEST_HOME);
        let mut value: Value = serde_json::from_str(&text).unwrap();
        if value["plan"].is_object() {
            value["plan"]["uid"] = json!(GUEST_UID);
        }
        value
    }

    /// What the guest's daemon answers the environment `request`, as it
    /// keeps what the change came to: the worker's whole answer, its own
    /// refusal, or a command's end.
    fn carried_out(&self, request: &Value) -> Value {
        self.asked.lock().unwrap().push(request.clone());
        self.place();
        let packet: Packet = serde_json::from_value(self.onto(&request["packet"])).unwrap();
        if let Request::Run { argv } = &packet.request {
            return self.ran(argv);
        }
        let first = match &packet.request {
            Request::Apply { plan } => plan
                .files
                .first()
                .map(|file| self.home.join(&file.payload.target)),
            _ => None,
        };
        let ending = *self.ending.lock().unwrap();
        let killing = ending == Ending::Killed;
        let guard = || {
            if killing && first.as_ref().is_some_and(|first| first.exists()) {
                return Err(crate::environment::problem("The worker was ended"));
            }
            Ok(())
        };
        // Installed as the guest's worker installs, but never registered
        // with this Mac's LaunchServices.
        let answered = if let Request::InstallApp { id, source, name } = &packet.request {
            crate::environment::installs::install_unregistered(&self.home, id, source, name)
        } else {
            carry_out_in(&self.home, packet, request["apply"] == true, &guard)
        };
        if killing {
            return Value::Null;
        }
        let reply = match answered {
            Ok(answer) => {
                let mut answer = self.off(&answer);
                if self.inflating.load(SeqCst) && answer["plan"].is_object() {
                    answer["plan"]["files"][0]["before"]["xattrs"]["user.padding"] =
                        json!("x".repeat(atlas_core::protocol::MAX_REQUEST));
                }
                json!({"ok": true, "op": "environment", "exitCode": 0, "result": answer})
            }
            Err(error) if error.uncertain => json!(error),
            Err(error) => {
                let mut refused = self.off(&json!(error));
                refused["refusal"] = json!("its-own-name");
                json!({"ok": true, "op": "environment", "exitCode": WORKER_REFUSED,
                    "result": refused})
            }
        };
        let said = match ending {
            Ending::Answered | Ending::Killed => reply,
            Ending::Unanswered => Value::Null,
            Ending::Kept => {
                let change = Token::parse(request["change"].as_str().unwrap()).unwrap();
                self.standin.done(&change, &reply);
                Value::Null
            }
        };
        if let Some(during) = &*self.during.lock().unwrap() {
            during();
        }
        said
    }

    /// What a command run with the guest's profile comes to: `env` prints
    /// its variables, and `cat` a file of its home.
    fn ran(&self, argv: &[String]) -> Value {
        let profile = self.home.join(".config/atlas/environment.json");
        let Ok(bytes) = fs::read(&profile) else {
            return json!(Error::new(
                NO_PROFILE,
                "No helper environment profile is installed"
            ));
        };
        let printed = match argv[0].as_str() {
            "/usr/bin/env" => {
                let profile: Value = serde_json::from_slice(&bytes).unwrap();
                profile["variables"]
                    .as_object()
                    .unwrap()
                    .iter()
                    .map(|(name, value)| format!("{name}={}\n", value.as_str().unwrap()))
                    .collect::<String>()
                    .into_bytes()
            }
            "/bin/cat" => fs::read(self.home.join(&argv[1])).unwrap(),
            _ => Vec::new(),
        };
        json!({"ok": true, "exitCode": 0, "timedOut": false, "signal": null, "truncated": false,
            "events": [{"event": "started", "pid": 321},
                {"event": "output", "data": STANDARD.encode(printed)}]})
    }

    /// The file `target` of the guest's home.
    pub(super) fn file(&self, target: &str) -> Option<Vec<u8>> {
        fs::read(self.home.join(target)).ok()
    }
}

/// The guest's own environment worker, `disk` its own, answering its
/// environment requests; its apps and jobs none.
fn worker(serving: &Serving, saying: &Saying) -> Arc<Disk> {
    let disk = Arc::new(Disk::new(serving.fixture.guest.standin.clone()));
    let working = disk.clone();
    *saying.otherwise.lock().unwrap() =
        Some(Arc::new(move |header: &Header, request: Value| {
            match (&header.kind, request["op"].as_str()) {
                (LaneKind::Change { .. }, Some("environment")) => working.carried_out(&request),
                (LaneKind::Read, Some("apps")) => json!({"ok": true, "apps": [], "starts": null}),
                (LaneKind::Read, Some("job")) => {
                    JobReply::Show(Showing::Every { jobs: Vec::new() }).into_value()
                }
                _ => json!({"ok": true, "echo": request}),
            }
        }));
    disk
}

/// A keeper serving a guest that proves itself and has a worker of its
/// own, an agent's client of it, and the guest's disk. The client records
/// no activity of its own, as the CLI's policy gives a plan's steps none.
pub(super) async fn environment() -> (Serving, Arc<Saying>, Client, Arc<Disk>, tempfile::TempDir) {
    let (serving, saying, mut client, directory) = proven().await;
    client.doing = None;
    let disk = worker(&serving, &saying);
    (serving, saying, client, disk, directory)
}

/// A manifest of two files, one executable, in a folder of its own.
pub(super) fn manifest() -> (tempfile::TempDir, PathBuf) {
    let folder = tempfile::tempdir().unwrap();
    fs::write(folder.path().join("notes.txt"), b"notes\n").unwrap();
    fs::write(folder.path().join("probe.sh"), b"#!/bin/sh\necho probe\n").unwrap();
    let manifest = folder.path().join("manifest.toml");
    fs::write(
        &manifest,
        "[[files]]\nsource = \"notes.txt\"\ntarget = \".config/probe/notes.txt\"\n\n\
         [[files]]\nsource = \"probe.sh\"\ntarget = \"bin/probe.sh\"\nexecutable = true\n",
    )
    .unwrap();
    (folder, manifest)
}

/// The plan `reply`, a preview's, names.
pub(super) fn plan_id(reply: &Value) -> String {
    reply["planId"].as_str().unwrap().to_owned()
}

/// Where plan `id` is frozen in `client`'s records.
pub(super) fn frozen(client: &Client, id: &str) -> PathBuf {
    client
        .output_dir()
        .unwrap()
        .join("environment")
        .join(format!("plan-{id}"))
}

/// The guest copy plan `id` keeps of its payloads.
pub(super) fn plan_copy(client: &Client, id: &str) -> Token {
    serde_json::from_slice(&fs::read(frozen(client, id).join("copy.json")).unwrap()).unwrap()
}

/// The markers of changes plan `id` keeps.
fn markers(client: &Client, id: &str) -> Vec<String> {
    fs::read_dir(frozen(client, id))
        .unwrap()
        .flatten()
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| name.starts_with("request-"))
        .collect()
}

/// Make the folder at `path` look `age` old.
fn aged(path: &Path, age: Duration) {
    fs::File::open(path)
        .unwrap()
        .set_modified(std::time::SystemTime::now() - age)
        .unwrap();
}

/// A preview of [`manifest`] through `client`: its reply.
pub(super) async fn previewed(client: &Client) -> Value {
    let (_folder, manifest) = manifest();
    let reply = crate::environment::sync(client, &manifest, false)
        .await
        .unwrap();
    assert_eq!(reply["ok"], true, "{reply}");
    reply
}

/// What the guest answers when it proves its work ended without knowing
/// what it came to.
pub(super) fn ended() -> Value {
    json!({"ok": false, "code": "uncertain", "error": "Its work is proven ended",
        "uncertain": true, "ended": true})
}

/// The keeper's recovery in the live generation.
pub(super) async fn recover(serving: &Serving) -> Value {
    asked(
        &serving.socket,
        &json!({"op": "recover", "generation": live(serving)}),
    )
    .await[0]
        .clone()
}

/// The one environment change the guest was asked for last.
pub(super) fn last_change(disk: &Disk) -> Token {
    let asked = disk.asked.lock().unwrap();
    Token::parse(asked.last().unwrap()["change"].as_str().unwrap()).unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_preview_on_a_vm_freezes_its_payloads_here_and_stages_them_into_its_guest_once() {
    let (serving, _saying, client, disk, _directory) = environment().await;

    let reply = previewed(&client).await;

    let id = plan_id(&reply);
    assert_eq!(reply["applied"], false, "{reply}");
    assert_eq!(reply["files"][0]["operation"], "create", "{reply}");
    let folder = frozen(&client, &id);
    assert_eq!(reply["retainedPlan"], json!(folder), "{reply}");
    assert_eq!(
        fs::read(folder.join("payloads/payload-0")).unwrap(),
        b"notes\n"
    );
    assert_eq!(
        fs::read(folder.join("payloads/payload-1")).unwrap(),
        b"#!/bin/sh\necho probe\n"
    );
    let staged = stage_lanes(&serving);
    assert_eq!(staged.len(), 1, "staged once: {staged:?}");
    let change = plan_copy(&client, &id);
    assert_eq!(staged[0], change);
    assert_eq!(
        copy(&serving, &change).unwrap().contents,
        b"notes\n#!/bin/sh\necho probe\n".to_vec()
    );
    match Owners::in_records(client.output_dir().unwrap()).of(&change) {
        Owned::By(owner) => assert_eq!(owner.plan.as_ref().map(Token::as_str), Some(&*id)),
        other => panic!("{other:?}"),
    }
    // The plan is the guest's: its account, its home, and its copy's paths.
    let plan: Value = serde_json::from_slice(&fs::read(folder.join("plan.json")).unwrap()).unwrap();
    assert_eq!(plan["uid"], GUEST_UID);
    assert_eq!(plan["home"], GUEST_HOME);
    assert_eq!(
        plan["files"][0]["payload"]["source"],
        format!("{ROOT}/{change}/payloads/payload-0")
    );
    let packets = disk.packets();
    assert_eq!(packets.len(), 1, "{packets:?}");
    assert_eq!(packets[0]["request"]["operation"], "preview");
    assert_eq!(packets[0]["expected"]["uid"], GUEST_UID);
    assert_eq!(
        packets[0]["expected"]["capturePid"], 123,
        "the guest's daemon"
    );
    assert!(
        disk.file(".config/probe/notes.txt").is_none(),
        "nothing applied"
    );
    assert!(client.controller.pending().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_applied_plan_on_a_vm_lets_its_copy_and_payloads_go_and_keeps_its_acknowledgement() {
    let (serving, _saying, client, disk, _directory) = environment().await;
    let id = plan_id(&previewed(&client).await);
    let change = plan_copy(&client, &id);

    let reply = crate::environment::apply(&client, &id).await.unwrap();

    assert_eq!(reply["ok"], true, "{reply}");
    assert_eq!(reply["applied"], true, "{reply}");
    assert_eq!(reply["operation"], "apply");
    assert_eq!(disk.file(".config/probe/notes.txt").unwrap(), b"notes\n");
    let script = disk.home.join("bin/probe.sh");
    assert_eq!(
        fs::metadata(&script).unwrap().permissions().mode() & 0o777,
        0o700
    );
    assert!(copy(&serving, &change).is_none(), "the guest's copy went");
    let folder = frozen(&client, &id);
    assert!(!folder.join("payloads").exists(), "the payloads went");
    assert!(!folder.join("copy.json").exists());
    assert!(folder.join("plan.json").exists(), "kept until it is undone");
    assert!(
        folder.join("applied.json").exists(),
        "the acknowledgement kept"
    );
    assert_eq!(applied_record(&client, &id)["acknowledged"], true);
    assert!(markers(&client, &id).is_empty());
    assert_eq!(
        Owners::in_records(client.output_dir().unwrap()).of(&change),
        Owned::Unknown
    );
    assert!(client.controller.pending().unwrap().is_empty());

    let again = crate::environment::apply(&client, &id).await.unwrap_err();
    assert!(again.error.contains("applied completely"), "{again:?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_rollback_on_a_vm_is_previewed_then_undone_and_its_plan_goes() {
    let (_serving, _saying, client, disk, _directory) = environment().await;
    let id = plan_id(&previewed(&client).await);
    crate::environment::apply(&client, &id).await.unwrap();

    let preview = crate::environment::rollback(&client, &id, false)
        .await
        .unwrap();
    let undone = crate::environment::rollback(&client, &id, true)
        .await
        .unwrap();

    assert_eq!(preview["ok"], true, "{preview}");
    assert_eq!(preview["preview"], true, "{preview}");
    assert_eq!(preview["operation"], "rollbackPreview");
    assert_eq!(
        preview["pendingTargets"],
        json!([".config/probe/notes.txt", "bin/probe.sh"])
    );
    assert_eq!(undone["ok"], true, "{undone}");
    assert_eq!(undone["rolledBack"], true, "{undone}");
    assert_eq!(undone["operation"], "rollback");
    assert!(undone["retainedPlan"].is_null(), "{undone}");
    assert!(!frozen(&client, &id).exists(), "the plan went once undone");
    assert!(disk.file(".config/probe/notes.txt").is_none());
    let packets = disk.packets();
    assert_eq!(packets.len(), 4, "{packets:?}");
    assert_eq!(
        packets[2]["request"],
        json!({"operation": "rollback", "id": id, "apply": false})
    );
    assert_eq!(
        packets[3]["request"],
        json!({"operation": "rollback", "id": id, "apply": true})
    );
    // A preview and an undo's preview are given a read's time; an apply and
    // an undo, two minutes, on the lane the guest takes them on.
    let times: Vec<Value> = disk
        .asked
        .lock()
        .unwrap()
        .iter()
        .map(|asked| asked["timeoutMs"].clone())
        .collect();
    assert_eq!(
        times,
        json!([20_000, 120_000, 20_000, 120_000])
            .as_array()
            .unwrap()
            .clone()
    );
    assert!(client.controller.pending().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_list_on_a_vm_joins_the_guests_transactions_with_the_plans_frozen_here() {
    let (_serving, _saying, client, _disk, _directory) = environment().await;
    let applied = plan_id(&previewed(&client).await);
    crate::environment::apply(&client, &applied).await.unwrap();
    let (_folder, other) = manifest();
    let other = fs::read_to_string(&other)
        .unwrap()
        .replace(".config/probe", ".config/other")
        .replace("bin/probe.sh", "bin/other.sh");
    let folder = tempfile::tempdir().unwrap();
    fs::write(folder.path().join("notes.txt"), b"other\n").unwrap();
    fs::write(folder.path().join("probe.sh"), b"#!/bin/sh\n").unwrap();
    fs::write(folder.path().join("other.toml"), other).unwrap();
    let previewed = crate::environment::sync(&client, &folder.path().join("other.toml"), false)
        .await
        .unwrap();
    let previewed = plan_id(&previewed);

    let listed = crate::environment::list(&client, false).await.unwrap();

    assert_eq!(listed["ok"], true, "{listed}");
    let plans = listed["plans"].as_array().unwrap();
    let state = |id: &str| {
        plans
            .iter()
            .find(|plan| plan["planId"] == id)
            .map(|plan| plan["state"].clone())
    };
    assert_eq!(state(&applied), Some(json!("applied")), "{listed}");
    assert_eq!(state(&previewed), Some(json!("previewed")), "{listed}");
    assert!(client.controller.pending().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_list_while_the_guest_holds_agents_gives_the_plans_here_with_the_guests_word_unknown() {
    let (serving, _saying, client, disk, _directory) = environment().await;
    let id = plan_id(&previewed(&client).await);
    let before = disk.asked.lock().unwrap().len();
    *serving.fixture.guest.stop.lock().unwrap() = "stopped";

    let listed = crate::environment::list(&client, false).await.unwrap();

    assert_eq!(listed["transactions"], "unknown", "{listed}");
    assert!(listed["why"].as_str().unwrap().contains("work"), "{listed}");
    assert_eq!(listed["plans"][0]["planId"], id, "{listed}");
    assert_eq!(listed["plans"][0]["state"], "unknown", "{listed}");
    assert_eq!(disk.asked.lock().unwrap().len(), before, "nothing asked");
    assert!(client.controller.pending().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_profiles_variable_reaches_a_command_environment_run_starts_on_a_vm() {
    let (_serving, _saying, client, disk, _directory) = environment().await;
    let folder = tempfile::tempdir().unwrap();
    fs::write(folder.path().join("notes.txt"), b"notes\n").unwrap();
    let profile = folder.path().join("profile.toml");
    fs::write(
        &profile,
        "version = 2\nenvironment = [\"CARGO_PKG_NAME\"]\n\n[[files]]\nsource = \"notes.txt\"\n\
         target = \".config/probe/notes.txt\"\n",
    )
    .unwrap();

    let synced = crate::environment::profile::sync(&client, &profile, true)
        .await
        .unwrap();
    let ran = crate::environment::run(
        &client,
        vec!["/usr/bin/env".to_owned()],
        Duration::from_secs(20),
    )
    .await
    .unwrap();
    let read = crate::environment::run(
        &client,
        vec!["/bin/cat".to_owned(), ".config/probe/notes.txt".to_owned()],
        Duration::from_secs(20),
    )
    .await
    .unwrap();

    assert_eq!(synced["ok"], true, "{synced}");
    assert_eq!(synced["applied"], true, "{synced}");
    assert_eq!(synced["readinessBeforeApply"], json!({"checked": false}));
    assert_eq!(synced["environmentNames"], json!(["CARGO_PKG_NAME"]));
    let printed = |reply: &Value| {
        String::from_utf8(atlas_core::transport::run_output(reply, 65536).unwrap()).unwrap()
    };
    assert_eq!(
        printed(&ran),
        format!("CARGO_PKG_NAME={}\n", env!("CARGO_PKG_NAME"))
    );
    assert_eq!(printed(&read), "notes\n");
    let packets = disk.packets();
    let run = packets.last().unwrap();
    assert_eq!(
        run["request"],
        json!({"operation": "run", "argv": ["/bin/cat", ".config/probe/notes.txt"]})
    );
    let asked = disk.asked.lock().unwrap().last().unwrap().clone();
    assert_eq!(asked["apply"], true);
    assert!(client.controller.pending().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_command_with_no_profile_on_a_vm_is_refused_in_the_workers_code() {
    let (_serving, _saying, client, _disk, _directory) = environment().await;

    let refused = crate::environment::run(
        &client,
        vec!["/usr/bin/env".to_owned()],
        Duration::from_secs(20),
    )
    .await
    .unwrap_err();

    assert_eq!(refused.code, NO_PROFILE, "{refused:?}");
    assert_eq!(
        refused.error,
        "No environment profile is installed in work. Use atlas run instead, or run atlas \
         environment profile to set one up."
    );
    assert!(!refused.uncertain);
    assert!(client.controller.pending().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_apply_whose_reply_was_lost_is_read_back_from_the_guest_and_settled() {
    let (serving, _saying, client, disk, _directory) = environment().await;
    let id = plan_id(&previewed(&client).await);
    let change = plan_copy(&client, &id);
    serving.fixture.guest.standin.dropping.store(1, SeqCst);

    let reply = crate::environment::apply(&client, &id).await.unwrap();

    assert_eq!(reply["applied"], true, "{reply}");
    assert_eq!(disk.file(".config/probe/notes.txt").unwrap(), b"notes\n");
    assert!(copy(&serving, &change).is_none(), "{reply}");
    assert!(frozen(&client, &id).join("applied.json").exists());
    assert!(markers(&client, &id).is_empty());
    assert!(client.controller.pending().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_apply_whose_command_was_killed_part_way_stays_pending_then_resumes_from_its_frozen_plan()
 {
    let (serving, _saying, client, disk, _directory) = environment().await;
    let id = plan_id(&previewed(&client).await);
    *disk.ending.lock().unwrap() = Ending::Killed;
    let applying = tokio::spawn({
        let (client, id) = (client.clone(), id.clone());
        async move { crate::environment::apply(&client, &id).await }
    });
    until("the first file applied", || {
        disk.file(".config/probe/notes.txt").is_some()
    })
    .await;
    let change = last_change(&disk);
    until("the guest carrying it out", || {
        record(&serving, &change) == Some(None)
    })
    .await;

    applying.abort();
    let _ = applying.await;

    assert_eq!(client.controller.pending().unwrap(), [change.as_str()]);
    assert_eq!(markers(&client, &id), [format!("request-{change}")]);
    let held = crate::environment::apply(&client, &id).await.unwrap_err();
    assert!(held.error.contains("does not know yet"), "{held:?}");
    *disk.ending.lock().unwrap() = Ending::Answered;
    serving.fixture.guest.standin.done(&change, &ended());
    recover(&serving).await;
    until("its claim cleared", || {
        client.controller.pending().unwrap().is_empty()
    })
    .await;
    let listed = crate::environment::list(&client, false).await.unwrap();
    assert_eq!(listed["plans"][0]["state"], "applying", "{listed}");
    assert!(
        markers(&client, &id).is_empty(),
        "the marker went once settled"
    );

    let resumed = crate::environment::apply(&client, &id).await.unwrap();

    assert_eq!(resumed["applied"], true, "{resumed}");
    assert_eq!(
        disk.file("bin/probe.sh").unwrap(),
        b"#!/bin/sh\necho probe\n"
    );
    let packets = disk.packets();
    let applies: Vec<&Value> = packets
        .iter()
        .filter(|packet| packet["request"]["operation"] == "apply")
        .collect();
    assert_eq!(applies.len(), 2, "{packets:?}");
    assert_eq!(applies[0], applies[1], "the same frozen plan sent again");
    assert!(client.controller.pending().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_stop_mid_rollback_leaves_it_pending_with_its_plan_until_the_guest_answers() {
    let (serving, _saying, client, disk, _directory) = environment().await;
    let id = plan_id(&previewed(&client).await);
    crate::environment::apply(&client, &id).await.unwrap();
    let guest = serving.fixture.guest.standin.clone();
    guest.holding.store(true, SeqCst);
    let agent = client.clone();
    let undoing = tokio::spawn({
        let (agent, id) = (agent.clone(), id.clone());
        async move { crate::environment::rollback(&agent, &id, true).await }
    });
    until("asked", || {
        guest.changes.lock().unwrap().values().any(Option::is_none)
    })
    .await;
    let change = guest
        .changes
        .lock()
        .unwrap()
        .iter()
        .find(|(_, record)| record.is_none())
        .map(|(change, _)| change.clone())
        .unwrap();

    client.controller.note_stop(true).unwrap();
    let stopped = undoing.await.unwrap().unwrap_err();

    assert_eq!(stopped.code, Code::Stopped, "{stopped:?}");
    assert!(stopped.uncertain, "{stopped:?}");
    assert_eq!(
        stopped.details["pendingChange"],
        json!(change),
        "{stopped:?}"
    );
    assert!(stopped.error.contains("retained"), "{stopped:?}");
    assert!(
        disk.file(".config/probe/notes.txt").is_some(),
        "nothing undone yet"
    );
    assert_eq!(client.controller.pending().unwrap(), [change.as_str()]);
    assert_eq!(markers(&client, &id), [format!("request-{change}")]);
    assert!(frozen(&client, &id).join("plan.json").exists());
    until("its lane cancelled", || {
        record(&serving, &change).is_some_and(|record| record.is_some())
    })
    .await;
    guest.holding.store(false, SeqCst);
    guest.done(&change, &ended());
    let revision = client.controller.stop_revision().unwrap();
    client.controller.lift_stop(revision.as_ref()).unwrap();
    recover(&serving).await;
    until("its claim cleared", || {
        client.controller.pending().unwrap().is_empty()
    })
    .await;

    let undone = crate::environment::rollback(&agent, &id, true)
        .await
        .unwrap();

    assert_eq!(undone["rolledBack"], true, "{undone}");
    assert!(!frozen(&client, &id).exists());
    assert!(client.controller.pending().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_apply_proved_in_a_generation_that_ended_is_refused_with_nothing_sent_or_left() {
    let (serving, _saying, client, disk, _directory) = environment().await;
    let id = plan_id(&previewed(&client).await);
    let verified = client.verify_vm().await.unwrap();
    restarted(&serving).await;
    let asked = disk.asked.lock().unwrap().len();

    let refused = crate::vm::environment::apply_verified(&client, &verified, &id)
        .await
        .unwrap_err();

    assert_eq!(refused.code, Code::SessionChanged, "{refused:?}");
    assert!(!refused.uncertain, "{refused:?}");
    assert_eq!(disk.asked.lock().unwrap().len(), asked, "nothing sent");
    assert!(markers(&client, &id).is_empty(), "nothing left marked");
    assert!(
        !frozen(&client, &id).join("applied.json").exists(),
        "an apply refused for certain keeps no record that it may have applied"
    );
    assert!(
        frozen(&client, &id).join("plan.json").exists(),
        "the plan kept"
    );
    assert!(client.controller.pending().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_guest_without_the_environment_worker_is_refused_before_anything_is_made_or_staged() {
    let (serving, saying, client, disk, _directory) = environment().await;
    *saying.hello.lock().unwrap() = Some(|hello: &mut Value| {
        let capabilities = hello["daemon"]["capabilities"].as_array_mut().unwrap();
        capabilities.retain(|name| name != atlas_session::compatibility::ENVIRONMENT);
    });
    let (_folder, manifest) = manifest();

    let refused = crate::environment::sync(&client, &manifest, false)
        .await
        .unwrap_err();

    assert_eq!(refused.code, Code::Update, "{refused:?}");
    assert_eq!(
        refused.error,
        "work is running an older Atlas that has no environment worker"
    );
    assert!(stage_lanes(&serving).is_empty(), "nothing staged");
    assert!(disk.asked.lock().unwrap().is_empty());
    assert!(
        !client.output_dir().unwrap().join("environment").exists(),
        "nothing made"
    );
    assert!(client.controller.pending().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_profile_or_discovery_naming_a_path_on_this_mac_is_refused_on_a_vm_with_nothing_made() {
    let (serving, _saying, client, _disk, _directory) = environment().await;
    let folder = tempfile::tempdir().unwrap();
    let profile = folder.path().join("profile.toml");
    fs::write(
        &profile,
        "version = 2\n\n[[references]]\npath = \"/Users/Shared\"\n",
    )
    .unwrap();
    let lanes = admitted(&serving).len();
    let words = "work keeps its own files, so a profile can't point it at a path on this Mac; \
                 copy them instead as [[files]] entries.";

    let refused = crate::environment::profile::sync(&client, &profile, true)
        .await
        .unwrap_err();
    let discovered = crate::environment::discovery::discover(
        &client,
        &[],
        &[],
        &[],
        &[PathBuf::from("/Users/Shared")],
    )
    .await
    .unwrap_err();
    let discovery =
        crate::environment::discovery::discover(&client, &["cat".to_owned()], &[], &[], &[])
            .await
            .unwrap_err();

    assert_eq!(refused.error, words);
    assert_eq!(discovered.error, words);
    assert_eq!(
        discovery.error,
        client.unreachable().error,
        "discovery stays refused"
    );
    assert_eq!(
        admitted(&serving).len(),
        lanes,
        "nothing asked of the guest"
    );
    assert!(!client.output_dir().unwrap().join("environment").exists());
    assert!(client.controller.pending().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_plan_too_large_for_one_request_is_refused_at_preview_and_its_copy_goes() {
    let (serving, _saying, client, disk, _directory) = environment().await;
    // A file already as selected, whose fingerprint the plan carries.
    fs::create_dir_all(disk.home.join(".config/probe")).unwrap();
    fs::write(disk.home.join(".config/probe/notes.txt"), b"notes\n").unwrap();
    fs::set_permissions(
        disk.home.join(".config/probe/notes.txt"),
        fs::Permissions::from_mode(0o600),
    )
    .unwrap();
    disk.inflating.store(true, SeqCst);
    let (_folder, manifest) = manifest();

    let refused = crate::environment::sync(&client, &manifest, false)
        .await
        .unwrap_err();

    assert!(refused.error.contains("too large"), "{refused:?}");
    assert!(refused.error.contains("select fewer files"), "{refused:?}");
    let staged = stage_lanes(&serving);
    assert_eq!(staged.len(), 1);
    assert!(copy(&serving, &staged[0]).is_none(), "its copy went");
    assert_eq!(
        Owners::in_records(client.output_dir().unwrap()).of(&staged[0]),
        Owned::Unknown
    );
    let plans = fs::read_dir(client.output_dir().unwrap().join("environment"))
        .unwrap()
        .count();
    assert_eq!(plans, 0, "nothing frozen");
    assert!(client.controller.pending().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_change_whose_outcome_is_unknown_keeps_its_plan_from_being_undone_or_pruned() {
    let (serving, _saying, client, _disk, _directory) = environment().await;
    let id = plan_id(&previewed(&client).await);
    let change = plan_copy(&client, &id);
    let unknown = Token::new();
    drop(client.controller.quarantine_as(unknown.as_str()).unwrap());
    let folder = frozen(&client, &id);
    fs::write(folder.join(format!("request-{unknown}")), b"").unwrap();
    aged(&folder, Duration::from_secs(8 * 24 * 60 * 60));

    let refused = crate::environment::rollback(&client, &id, true)
        .await
        .unwrap_err();
    previewed(&client).await;

    assert!(refused.error.contains("does not know yet"), "{refused:?}");
    assert!(folder.join("plan.json").exists(), "never pruned");
    assert!(copy(&serving, &change).is_some(), "its copy kept");
    client.controller.clear_if(unknown.as_str()).unwrap();
    crate::environment::list(&client, false).await.unwrap();
    assert!(markers(&client, &id).is_empty(), "dropped once settled");
    aged(&folder, Duration::from_secs(8 * 24 * 60 * 60));
    previewed(&client).await;
    assert!(!folder.exists(), "pruned once nothing of it is unknown");
    assert!(copy(&serving, &change).is_none(), "with its copy");
    assert!(client.controller.pending().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_guest_that_no_longer_knows_an_acknowledged_apply_never_has_its_plan_forgotten_here() {
    let (_serving, _saying, client, disk, _directory) = environment().await;
    let id = plan_id(&previewed(&client).await);
    crate::environment::apply(&client, &id).await.unwrap();
    fs::remove_dir_all(disk.home.join(crate::environment::STATE)).unwrap();
    let folder = frozen(&client, &id);

    let preview = crate::environment::rollback(&client, &id, false)
        .await
        .unwrap();
    let undo = crate::environment::rollback(&client, &id, true)
        .await
        .unwrap();
    aged(&folder, Duration::from_secs(8 * 24 * 60 * 60));
    previewed(&client).await;
    let listed = crate::environment::list(&client, false).await.unwrap();

    assert_eq!(preview["ok"], false, "{preview}");
    assert_eq!(preview["error"], crate::environment::worker::MISSING);
    assert_eq!(undo["ok"], false, "{undo}");
    assert!(folder.join("applied.json").exists(), "{undo}");
    assert!(folder.join("plan.json").exists(), "never pruned");
    let plan = listed["plans"]
        .as_array()
        .unwrap()
        .iter()
        .find(|plan| plan["planId"] == id)
        .cloned()
        .unwrap();
    assert_eq!(plan["state"], "unknown", "{listed}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn closing_and_cleaning_up_after_a_preview_leave_its_copy_for_the_apply() {
    let (serving, _saying, mut client, disk, _directory) = environment().await;
    let host = HostSession::new(Host::Codex, "previewing").unwrap();
    let session = host.key().clone();
    client.host_session = Some(host);
    let id = plan_id(&previewed(&client).await);
    let change = plan_copy(&client, &id);
    // Claimed as a copy too, as the session's cleanup would find one.
    let task = crate::task::Task::of(&session, None).unwrap();
    crate::task::Claims::in_store(
        crate::task::store::Store::at(client.output_dir().unwrap()),
        task,
    )
    .claim(
        &crate::task::Owned::Copy { id: change.clone() },
        crate::task::State::Made,
    )
    .unwrap();

    let closed = crate::agent::close(
        &client
            .clone()
            .doing(Doing::new("close").app(Some("payloads"))),
        Closing::App("payloads"),
    )
    .await
    .unwrap();
    let mut person = client.clone();
    person.actor = crate::client::Actor::Viewer;
    let _ = crate::cleanup::for_person(&person, &session, crate::cleanup::Mode::Apply).await;
    let kept = copy(&serving, &change).is_some();
    let owned = Owners::in_records(client.output_dir().unwrap()).of(&change);
    let applied = crate::environment::apply(&client, &id).await.unwrap();

    assert_eq!(closed["removed"], json!([]), "{closed}");
    assert!(kept, "neither close nor cleanup took the plan's copy");
    assert!(matches!(owned, Owned::By(_)), "{owned:?}");
    assert_eq!(applied["applied"], true, "{applied}");
    assert_eq!(disk.file(".config/probe/notes.txt").unwrap(), b"notes\n");
    assert!(copy(&serving, &change).is_none(), "the apply let it go");
}

/// A folder of the test's own inside Atlas's own sockets folder, which the
/// system names outside the home: Atlas's own, as a real VM's records are.
fn inside_atlas_own() -> tempfile::TempDir {
    let sockets = atlas_core::own_folder::sockets().unwrap();
    atlas_core::files::private_folder(&sockets, Code::State).unwrap();
    tempfile::Builder::new()
        .prefix("test-records-")
        .tempdir_in(&sockets)
        .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn plans_are_frozen_in_records_inside_atlas_own_folder_which_no_agent_may_hand_over() {
    let serving = serving(true).await;
    let saying = serving.fixture.guest.proving();
    let disk = worker(&serving, &saying);
    let records = inside_atlas_own();
    assert!(atlas_core::own_folder::refuse(records.path()).is_err());
    let client = client_of(&serving, records.path()).doing(Doing::new("environment"));

    let id = plan_id(&previewed(&client).await);
    let payloads = frozen(&client, &id).join("payloads");
    let verified = client.verify_vm().await.unwrap();
    let ownership = client.controller.acquire_beside_commands().unwrap();
    let handed = crate::vm::copies::hand_over_to(
        &client,
        &ownership,
        vec![crate::handoff::Target::Path(payloads.clone())],
        &verified,
    )
    .await
    .unwrap_err();
    drop(ownership);
    let applied = crate::environment::apply(&client, &id).await.unwrap();
    let undone = crate::environment::rollback(&client, &id, true)
        .await
        .unwrap();

    assert_eq!(handed.error, atlas_core::own_folder::REFUSAL, "{handed:?}");
    assert_eq!(
        stage_lanes(&serving).len(),
        1,
        "only the plan's own folder staged"
    );
    assert_eq!(applied["applied"], true, "{applied}");
    assert_eq!(undone["rolledBack"], true, "{undone}");
    assert!(disk.file(".config/probe/notes.txt").is_none());
    assert!(!frozen(&client, &id).exists());
    assert!(client.controller.pending().unwrap().is_empty());
}

/// The guest's journal lost, plan `id`'s folder left more than a week, and
/// another preview made, which prunes what nothing needs.
async fn forgotten_then_pruned(client: &Client, disk: &Disk, id: &str) {
    fs::remove_dir_all(disk.home.join(crate::environment::STATE)).unwrap();
    aged(&frozen(client, id), Duration::from_secs(8 * 24 * 60 * 60));
    previewed(client).await;
}

/// What this Mac keeps of an apply of plan `id` that may have happened.
pub(super) fn applied_record(client: &Client, id: &str) -> Value {
    serde_json::from_slice(&fs::read(frozen(client, id).join("applied.json")).unwrap()).unwrap()
}

/// An apply of plan `id` the guest carries out whole, ending as `ending`
/// says, its command killed once it is: its change.
async fn killed_once_carried_out(
    client: &Client,
    disk: &Arc<Disk>,
    id: &str,
    ending: Ending,
) -> Token {
    *disk.ending.lock().unwrap() = ending;
    let carried = Arc::new(AtomicBool::new(false));
    *disk.during.lock().unwrap() = Some(Box::new({
        let carried = carried.clone();
        move || carried.store(true, SeqCst)
    }));
    let applying = tokio::spawn({
        let (client, id) = (client.clone(), id.to_owned());
        async move { crate::environment::apply(&client, &id).await }
    });
    until("carried out", || carried.load(SeqCst)).await;
    applying.abort();
    let _ = applying.await;
    *disk.ending.lock().unwrap() = Ending::Answered;
    *disk.during.lock().unwrap() = None;
    last_change(disk)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_apply_carried_out_but_never_answered_keeps_its_plan_once_the_guest_forgets_it() {
    let (serving, _saying, client, disk, _directory) = environment().await;
    let id = plan_id(&previewed(&client).await);
    let change = killed_once_carried_out(&client, &disk, &id, Ending::Unanswered).await;
    assert_eq!(applied_record(&client, &id)["acknowledged"], false);

    serving.fixture.guest.standin.done(&change, &ended());
    recover(&serving).await;
    until("its claim cleared", || {
        client.controller.pending().unwrap().is_empty()
    })
    .await;
    crate::environment::list(&client, false).await.unwrap();
    assert!(
        markers(&client, &id).is_empty(),
        "its marker went once settled"
    );
    forgotten_then_pruned(&client, &disk, &id).await;

    assert!(
        frozen(&client, &id).join("plan.json").exists(),
        "never pruned"
    );
    assert_eq!(applied_record(&client, &id)["acknowledged"], false);
    assert_eq!(disk.file(".config/probe/notes.txt").unwrap(), b"notes\n");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_apply_the_keeper_recovered_after_its_command_died_keeps_its_plan_once_the_guest_forgets_it()
 {
    let (serving, _saying, client, disk, _directory) = environment().await;
    let id = plan_id(&previewed(&client).await);
    let change = killed_once_carried_out(&client, &disk, &id, Ending::Kept).await;
    assert_eq!(record(&serving, &change).unwrap().unwrap()["ok"], true);

    recover(&serving).await;
    until("its claim cleared", || {
        client.controller.pending().unwrap().is_empty()
    })
    .await;
    crate::environment::list(&client, false).await.unwrap();
    assert!(
        markers(&client, &id).is_empty(),
        "its marker went once settled"
    );
    forgotten_then_pruned(&client, &disk, &id).await;

    assert!(
        frozen(&client, &id).join("plan.json").exists(),
        "never pruned"
    );
    assert_eq!(applied_record(&client, &id)["acknowledged"], false);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_apply_whose_acknowledgement_could_not_be_kept_stays_one_that_may_have_applied() {
    let (_serving, _saying, client, disk, _directory) = environment().await;
    let id = plan_id(&previewed(&client).await);
    let folder = frozen(&client, &id);
    let closed = || fs::set_permissions(&folder, fs::Permissions::from_mode(0o500)).unwrap();
    let open = || fs::set_permissions(&folder, fs::Permissions::from_mode(0o700)).unwrap();
    let asked = disk.asked.lock().unwrap().len();

    // Its record cannot be kept: nothing is sent.
    closed();
    let refused = crate::environment::apply(&client, &id).await;
    open();
    assert!(refused.is_err(), "{refused:?}");
    assert_eq!(disk.asked.lock().unwrap().len(), asked, "nothing sent");
    assert!(!folder.join("applied.json").exists());
    assert!(client.controller.pending().unwrap().is_empty());

    // Its acknowledgement cannot be marked: it stays one that may have applied.
    *disk.during.lock().unwrap() = Some(Box::new({
        let folder = folder.clone();
        move || fs::set_permissions(&folder, fs::Permissions::from_mode(0o500)).unwrap()
    }));
    let applied = crate::environment::apply(&client, &id).await.unwrap();
    *disk.during.lock().unwrap() = None;
    open();
    assert_eq!(applied["applied"], true, "{applied}");
    assert!(
        applied["warnings"][0]
            .as_str()
            .unwrap()
            .contains("may have applied"),
        "{applied}"
    );
    assert_eq!(applied_record(&client, &id)["acknowledged"], false);
    forgotten_then_pruned(&client, &disk, &id).await;

    assert!(folder.join("plan.json").exists(), "never pruned");
    assert!(client.controller.pending().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_apply_or_undo_on_a_vm_is_refused_before_it_is_sent_once_the_person_stopped_agents() {
    let (_serving, _saying, client, disk, _directory) = environment().await;
    let previewed = plan_id(&previewed(&client).await);
    let undone = plan_id(&{
        let (_folder, manifest) = manifest();
        let other = fs::read_to_string(&manifest)
            .unwrap()
            .replace(".config/probe", ".config/undone")
            .replace("bin/probe.sh", "bin/undone.sh");
        fs::write(&manifest, other).unwrap();
        crate::environment::sync(&client, &manifest, true)
            .await
            .unwrap()
    });
    let asked = disk.asked.lock().unwrap().len();
    client.controller.note_stop(true).unwrap();

    let applied = crate::environment::apply(&client, &previewed)
        .await
        .unwrap_err();
    let rolled_back = crate::environment::rollback(&client, &undone, true)
        .await
        .unwrap_err();

    for held in [&applied, &rolled_back] {
        assert_eq!(held.code, Code::Stopped, "{held:?}");
        assert!(!held.uncertain, "{held:?}");
        assert!(held.error.contains("work"), "{held:?}");
    }
    assert_eq!(disk.asked.lock().unwrap().len(), asked, "nothing sent");
    assert!(markers(&client, &previewed).is_empty());
    assert!(markers(&client, &undone).is_empty());
    assert!(
        !frozen(&client, &previewed).join("applied.json").exists(),
        "an apply refused for certain keeps no record that it may have applied"
    );
    assert!(
        disk.file(".config/undone/notes.txt").is_some(),
        "nothing undone"
    );
    assert!(client.controller.pending().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_apply_awaiting_its_reply_when_the_person_stops_agents_is_left_pending_with_its_plan() {
    let (serving, _saying, client, _disk, _directory) = environment().await;
    let id = plan_id(&previewed(&client).await);
    let guest = serving.fixture.guest.standin.clone();
    guest.holding.store(true, SeqCst);
    let applying = tokio::spawn({
        let (client, id) = (client.clone(), id.clone());
        async move { crate::environment::apply(&client, &id).await }
    });
    until("asked", || {
        guest.changes.lock().unwrap().values().any(Option::is_none)
    })
    .await;
    let change = guest
        .changes
        .lock()
        .unwrap()
        .iter()
        .find(|(_, record)| record.is_none())
        .map(|(change, _)| change.clone())
        .unwrap();

    client.controller.note_stop(true).unwrap();
    let stopped = applying.await.unwrap().unwrap_err();

    assert_eq!(stopped.code, Code::Stopped, "{stopped:?}");
    assert!(stopped.uncertain, "{stopped:?}");
    assert_eq!(
        stopped.details["pendingChange"],
        json!(change),
        "{stopped:?}"
    );
    assert_eq!(client.controller.pending().unwrap(), [change.as_str()]);
    assert_eq!(markers(&client, &id), [format!("request-{change}")]);
    assert_eq!(applied_record(&client, &id)["acknowledged"], false);
    guest.holding.store(false, SeqCst);
}

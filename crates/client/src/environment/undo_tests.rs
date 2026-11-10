use super::*;
use crate::test_support::{Helper, completed};
use atlas_core::files::Lock;
use std::os::unix::fs::DirBuilderExt;

/// A helper home, kept beside the shared command, holding one applied copy
/// of `.config/probe/settings.toml`.
struct Applied {
    _temp: tempfile::TempDir,
    root: PathBuf,
    home: PathBuf,
    plan: Plan,
}

impl Applied {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(temp.path()).unwrap();
        let home = root.join("home");
        fs::create_dir(&home).unwrap();
        let source = root.join("settings.toml");
        fs::write(&source, b"copied").unwrap();
        let files = vec![Payload {
            source,
            target: ".config/probe/settings.toml".into(),
            sha256: digest(b"copied"),
            executable: false,
        }];
        let uid = nix::unistd::getuid().as_raw();
        let id = atlas_core::files::token();
        let preview = worker::preview(&home, uid, &id, files, &|| Ok(())).unwrap();
        let plan: Plan = serde_json::from_value(preview["plan"].clone()).unwrap();
        let applied = worker::apply_plan(&home, &plan, &|| Ok(())).unwrap();
        assert_eq!(applied["ok"], true, "{applied}");
        Self {
            _temp: temp,
            root,
            home,
            plan,
        }
    }

    fn id(&self) -> Token {
        Token::parse(&self.plan.id).unwrap()
    }

    /// Make the helper's record of the copy one it cannot read.
    fn break_record(&self) {
        let store = state::Store::open(&self.home, Lock::Exclusive).unwrap();
        fs::write(
            store.folder(&self.plan.id).join("transaction.json"),
            b"{\"version\": 1}",
        )
        .unwrap();
    }

    /// Keep the frozen plan here, as a copy Atlas has yet to send.
    fn freeze_here(&self) {
        let stage = self.root.join(format!("environment-plan-{}", self.plan.id));
        fs::DirBuilder::new().mode(0o700).create(&stage).unwrap();
        let plan = Plan {
            uid: helper_uid(),
            ..self.plan.clone()
        };
        fs::write(stage.join("plan.json"), serde_json::to_vec(&plan).unwrap()).unwrap();
    }

    /// How far the copy got, as `plan_states` reads it from the helper's
    /// worker, its list changed by `edit` before it is sent.
    async fn states(&self, edit: impl Fn(&mut Value)) -> Result<Vec<PlanState>> {
        let (client, helper) = Helper::staging(&self.root, helper_uid());
        let home = self.home.clone();
        helper
            .serve(plan_states(&client, &[self.id()]), move |request| {
                let packet: Packet = decode(&fs::read(request["argv"][2].as_str()?).ok()?).ok()?;
                let Request::Transactions { all, ids } = packet.request else {
                    panic!("only the list of transactions is asked for");
                };
                let mut listed = worker::transactions(&home, all, &ids, &|| Ok(())).unwrap();
                edit(&mut listed);
                Some(completed(&listed.to_string()))
            })
            .await
    }
}

/// The account the stand-in helper is, as a helper must be another
/// account than the one running the test.
fn helper_uid() -> u32 {
    nix::unistd::User::from_name("nobody")
        .unwrap()
        .unwrap()
        .uid
        .as_raw()
}

#[tokio::test]
async fn an_unreadable_transaction_never_proves_that_a_plan_is_gone() {
    let readable = Applied::new();
    assert_eq!(
        readable.states(|_| ()).await.unwrap(),
        [PlanState {
            phase: PlanPhase::Applied,
            targets: vec![".config/probe/settings.toml".into()],
        }]
    );

    let unread = Applied::new();
    unread.break_record();
    let later = Applied::new();
    let frozen = Applied::new();
    frozen.break_record();
    frozen.freeze_here();
    let said = [
        unread.states(|_| ()).await,
        later
            .states(|listed| listed["transactions"][0]["files"] = json!("later"))
            .await,
        frozen.states(|_| ()).await,
    ];
    assert!(said.iter().all(Result::is_err), "{said:?}");
}

#[tokio::test]
async fn a_record_whose_time_cannot_be_read_still_names_its_plan() {
    let applied = Applied::new();
    let store = state::Store::open(&applied.home, Lock::Shared).unwrap();
    let before_1970 = std::time::UNIX_EPOCH - std::time::Duration::from_secs(24 * 60 * 60);
    fs::File::options()
        .write(true)
        .open(store.folder(&applied.plan.id).join("transaction.json"))
        .unwrap()
        .set_modified(before_1970)
        .unwrap();
    drop(store);

    assert_eq!(
        applied.states(|_| ()).await.unwrap(),
        [PlanState {
            phase: PlanPhase::Applied,
            targets: vec![".config/probe/settings.toml".into()],
        }]
    );
}

#[tokio::test]
async fn a_record_folder_that_is_not_a_folder_never_proves_that_a_plan_is_gone() {
    let applied = Applied::new();
    let store = state::Store::open(&applied.home, Lock::Exclusive).unwrap();
    let folder = store.folder(&applied.plan.id);
    fs::remove_dir_all(&folder).unwrap();
    fs::write(&folder, b"not a folder").unwrap();
    drop(store);

    let said = applied.states(|_| ()).await;
    assert!(said.is_err(), "{said:?}");
}

#[test]
fn reply_observations_keep_known_phases_and_leave_other_values_unknown() {
    for (phase, expected) in [
        (json!("applying"), Some(Phase::Applying)),
        (json!("applied"), Some(Phase::Applied)),
        (json!("rollingBack"), Some(Phase::RollingBack)),
        (json!("rolledBack"), Some(Phase::RolledBack)),
        (json!("future"), None),
        (Value::Null, None),
        (json!(false), None),
        (json!(7), None),
        (json!([]), None),
        (json!({}), None),
        (json!({"applying": null}), None),
        (json!({"applied": null}), None),
        (json!({"rollingBack": null}), None),
        (json!({"rolledBack": null}), None),
    ] {
        assert_eq!(WorkerReply::read(&json!({"phase": phase})).phase, expected);
    }
    assert_eq!(WorkerReply::read(&json!({})).phase, None);
}

#[test]
fn reply_observations_preserve_explicit_proof_and_error_defaults() {
    let fallback = "The helper did not finish undoing this copy";
    for (value, error) in [
        (Value::Null, fallback),
        (json!("false"), "false"),
        (json!(7), fallback),
        (json!([]), fallback),
        (json!({}), fallback),
    ] {
        let reply = WorkerReply::read(&json!({
            "ok": value, "resumeRequired": value, "error": value
        }));
        assert_eq!(reply.ok, None);
        assert_eq!(reply.resume_required, None);
        assert_eq!(reply.error, error);
    }
    for value in [false, true] {
        let reply = WorkerReply::read(&json!({
            "ok": value, "resumeRequired": value, "error": worker::MISSING
        }));
        assert_eq!(reply.ok, Some(value));
        assert_eq!(reply.resume_required, Some(value));
        assert_eq!(reply.error, worker::MISSING);
    }
}

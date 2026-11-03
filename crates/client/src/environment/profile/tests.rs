use super::*;
use crate::{
    environment::{
        RESERVED_VARIABLES,
        model::{Packet, Plan, Request, decode},
        stage::{leftovers, requests},
        worker,
    },
    test_support::{Helper, completed},
};
use tokio::sync::Notify;

/// The account the stand-in helper is, as a helper must be another
/// account than the one running the test.
fn helper_uid() -> u32 {
    nix::unistd::User::from_name("nobody")
        .unwrap()
        .unwrap()
        .uid
        .as_raw()
}

/// A client of a stand-in helper kept in a fresh folder, with its output
/// folder, a helper home and a profile that selects no files.
struct Syncing {
    _temp: tempfile::TempDir,
    root: PathBuf,
    home: PathBuf,
    output: PathBuf,
    profile: PathBuf,
    client: Client,
    helper: Helper,
}

impl Syncing {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(temp.path()).unwrap();
        let (client, helper) = Helper::staging(&root, helper_uid());
        let output = client.output_dir().unwrap().to_owned();
        fs::create_dir_all(&output).unwrap();
        let home = root.join("home");
        fs::create_dir(&home).unwrap();
        let profile = root.join("profile.toml");
        fs::write(&profile, "version = 2\n").unwrap();
        Self {
            _temp: temp,
            root,
            home,
            output,
            profile,
            client,
            helper,
        }
    }

    /// The generated sources a sync keeps in the output folder now.
    fn generated(&self) -> Vec<PathBuf> {
        fs::read_dir(&self.output)
            .unwrap()
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| {
                path.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("environment-profile-")
            })
            .collect()
    }

    /// The helper's answer to a preview, made on its home.
    fn previewed(&self, id: &str, files: Vec<super::super::model::Payload>) -> Value {
        let value = worker::preview(&self.home, helper_uid(), id, files, &|| Ok(())).unwrap();
        completed(&value.to_string())
    }

    /// Sync the profile, applying it when `apply`, until `held` is
    /// notified: none when it was cut short then.
    async fn sync_until(&self, apply: bool, held: &Notify) -> Option<Result<Value>> {
        tokio::select! {
            biased;
            () = held.notified() => None,
            result = sync(&self.client, &self.profile, apply) => Some(result),
        }
    }
}

/// What a run of the environment worker asks of it.
fn asked(request: &Value) -> Option<Request> {
    let packet: Packet = decode(&fs::read(request["argv"][2].as_str()?).ok()?).ok()?;
    Some(packet.request)
}

#[tokio::test]
async fn cancelling_profile_synchronization_removes_its_generated_environment_source() {
    let syncing = Syncing::new();
    let held = Notify::new();

    let finished = syncing
        .helper
        .serve(syncing.sync_until(false, &held), |request| {
            let Some(Request::Preview { .. }) = asked(request) else {
                panic!("only a preview is asked for");
            };
            let sources = syncing.generated();
            assert_eq!(sources.len(), 1, "{sources:?}");
            assert!(sources[0].join("environment.json").exists());
            held.notify_one();
            None
        })
        .await;

    assert!(finished.is_none(), "{finished:?}");
    assert_eq!(syncing.generated(), Vec::<PathBuf>::new());
}

#[tokio::test]
async fn cancelling_after_staging_preserves_recovery_without_retaining_the_generated_source() {
    let syncing = Syncing::new();
    let held = Notify::new();

    let finished = syncing
        .helper
        .serve(syncing.sync_until(true, &held), |request| {
            match asked(request)? {
                Request::Preview { id, files } => Some(syncing.previewed(&id, files)),
                Request::Apply { .. } => {
                    held.notify_one();
                    None
                }
                _ => panic!("only a preview and its apply are asked for"),
            }
        })
        .await;

    assert!(finished.is_none(), "{finished:?}");
    assert_eq!(syncing.generated(), Vec::<PathBuf>::new());
    let stages: Vec<PathBuf> = leftovers(&syncing.root)
        .into_iter()
        .filter(|stage| {
            stage
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("environment-plan-")
        })
        .collect();
    assert_eq!(stages.len(), 1, "{stages:?}");
    let plan: Plan = decode(&fs::read(stages[0].join("plan.json")).unwrap()).unwrap();
    assert_eq!(plan.uid, helper_uid());
    let bytes = selected_environment(&load(&syncing.profile).unwrap()).unwrap();
    assert_eq!(fs::read(&plan.files[0].payload.source).unwrap(), bytes);
    assert!(requests(&stages[0]), "the apply's request is kept");
}

#[tokio::test]
async fn a_finished_profile_synchronization_removes_its_generated_source() {
    let syncing = Syncing::new();
    let synced = syncing
        .helper
        .serve(
            sync(&syncing.client, &syncing.profile, false),
            |request| match asked(request)? {
                Request::Preview { id, files } => Some(syncing.previewed(&id, files)),
                _ => panic!("only a preview is asked for"),
            },
        )
        .await
        .unwrap();
    assert_eq!(synced["ok"], true, "{synced}");
    assert!(synced.get("cleanupWarning").is_none(), "{synced}");
    assert_eq!(syncing.generated(), Vec::<PathBuf>::new());

    let syncing = Syncing::new();
    let failed = syncing
        .helper
        .serve(sync(&syncing.client, &syncing.profile, false), |_| None)
        .await
        .unwrap_err();
    assert!(!failed.error.contains("cleanup"), "{failed:?}");
    assert_eq!(syncing.generated(), Vec::<PathBuf>::new());
}

#[tokio::test]
async fn a_generated_source_that_will_not_go_is_reported() {
    // Permissions do not hold root back, so nothing would fail.
    if nix::unistd::geteuid().is_root() {
        return;
    }
    use std::os::unix::fs::PermissionsExt;
    let syncing = Syncing::new();
    let synced = syncing
        .helper
        .serve(
            sync(&syncing.client, &syncing.profile, false),
            |request| match asked(request)? {
                Request::Preview { id, files } => {
                    for source in syncing.generated() {
                        fs::set_permissions(&source, fs::Permissions::from_mode(0o500)).unwrap();
                    }
                    Some(syncing.previewed(&id, files))
                }
                _ => panic!("only a preview is asked for"),
            },
        )
        .await
        .unwrap();
    let sources = syncing.generated();
    for source in &sources {
        fs::set_permissions(source, fs::Permissions::from_mode(0o700)).unwrap();
    }

    assert_eq!(sources.len(), 1, "{sources:?}");
    let warning = synced["cleanupWarning"].as_str().unwrap();
    assert!(
        warning.contains(&sources[0].to_string_lossy().into_owned()),
        "{warning}"
    );
}

fn profile(value: Value) -> Profile {
    serde_json::from_value(value).unwrap()
}

#[test]
fn profile_cannot_shadow_helper_identity_or_generated_state() {
    for name in RESERVED_VARIABLES {
        assert!(validate(&profile(json!({"version":2,"environment":[name]}))).is_err());
    }

    assert!(
        validate(&profile(
            json!({"version":2,"files":[{"source":"x","target":ENVIRONMENT_TARGET}]})
        ))
        .is_err()
    );
    assert!(validate(&profile(json!({"version":2,"environment":["KEY","KEY"]}))).is_err());
    assert!(validate(&profile(json!({"version":2,"environment":["KEY=value"]}))).is_err());
    assert!(
        validate(&profile(
            json!({"version":2,"environment":["OLLAMA_MODELS"]})
        ))
        .is_ok()
    );
}

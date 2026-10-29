//! `atlas install-app`: put an app from this Mac into the helper's
//! `~/Applications` for the current task, never over anything already
//! there, and record it so `atlas cleanup --apply` moves it aside once the
//! task is done, unless it changed meanwhile. The install is claimed for
//! the task before the helper is asked, so a crash on the way leaves it
//! findable by its ID; and it is the task's own install that `open --app
//! NAME` falls back to when LaunchServices does not know the name yet.
use crate::{
    client::Client,
    environment,
    error::{Code, Error, Result},
    files::Token,
    handoff::{self, Running, Target},
    task::{Claims, Owned, State, store::Store as Tasks},
    text::same_text,
};
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
};

/// Install the app at `path` into the helper for the current task.
pub async fn install(client: &Client, path: &Path) -> Result<Value> {
    let (bundle, name) = bundle(path)?;
    let task = client.task.clone().ok_or_else(|| {
        Error::new(
            Code::Usage,
            "Atlas installs an app only for a task, so atlas cleanup can remove it. Set \
             ATLAS_SESSION=NAME before the task's first command, then install again.",
        )
    })?;
    let app = crate::doing::bounded(name.trim_end_matches(".app"));
    let output = client.output_dir()?;
    let claims = Claims::in_store(Tasks::at(output), task.clone());
    if let Some(earlier) = earlier_install(&Tasks::at(output), task.key(), &app) {
        return Err(Error::new(
            Code::Input,
            format!(
                "This task already installed {app} (installId {earlier}). Clean it up with \
                 atlas cleanup --apply before installing it again."
            ),
        )
        .with("installId", earlier));
    }

    // Copying, installing and removing the copy happen under one ownership,
    // so no other open, close or cleanup interleaves.
    let ownership = client.controller.acquire(false)?;
    let running = Running::listed(&client.apps().await?);

    let id = Token::new();
    let install = Owned::Install {
        id: id.clone(),
        app: app.clone(),
    };
    claims.claim(&install, State::Intent)?;

    let handed =
        match handoff::hand_over(client, &ownership, vec![Target::Path(bundle)], &running).await {
            Ok(handed) => handed,
            Err(error) => {
                let _ = claims.settle_all(&[(install, State::Gone)]);
                return Err(error);
            }
        };
    let source = PathBuf::from(handed.paths.first().cloned().unwrap_or_default());
    let transport = handed.copied.first().map(|copied| copied.token.clone());

    let result = environment::install_app(client, &ownership, &id, &source, &name).await;
    let made = match &result {
        // Its outcome is not known: the install and the copy it came from
        // stay claimed, for cleanup to settle by their IDs.
        Err(error) if error.uncertain => return result,
        Err(_) => false,
        Ok(reply) => reply["ok"] == true || reply["resumeRequired"] == true,
    };
    let mut settled = vec![(install, if made { State::Made } else { State::Gone })];
    if let Some(token) = transport {
        let session = task.session().clone();
        let gone = matches!(
            handoff::remove_copy(Path::new(handoff::ROOT), &token, &session, &running),
            handoff::CopyRemoval::Removed | handoff::CopyRemoval::AlreadyGone
        );
        if gone {
            settled.push((Owned::Copy { id: token }, State::Gone));
        }
    }
    let _ = claims.settle_all(&settled);
    result
}

/// The bundle `path` names, made absolute, and its file name: a real
/// folder (never a link) called `NAME.app` with a Contents/Info.plist.
fn bundle(path: &Path) -> Result<(PathBuf, String)> {
    let usage = || {
        Error::new(
            Code::Usage,
            "Give the path of an app bundle on this Mac, such as ./build/Probe.app",
        )
    };
    let absolute = std::path::absolute(path).map_err(|_| usage())?;
    let real = fs::symlink_metadata(&absolute).is_ok_and(|metadata| metadata.is_dir());
    let name = absolute
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| name.len() > 4 && name.ends_with(".app") && !name.starts_with('.'))
        .map(str::to_owned);
    let plist = absolute.join("Contents/Info.plist");
    match name {
        Some(name)
            if real && fs::symlink_metadata(&plist).is_ok_and(|metadata| metadata.is_file()) =>
        {
            let bundle = fs::canonicalize(&absolute).map_err(|_| usage())?;
            Ok((bundle, name))
        }
        _ => Err(usage()),
    }
}

/// The install of an app named `app` that task `task` made and has not
/// cleaned up yet, by its ID.
fn earlier_install(tasks: &Tasks, task: &crate::host_session::Key, app: &str) -> Option<Token> {
    install_named(tasks, task, app, State::live)
}

/// The install of an app named `app` that task `task` claims in a state
/// `wanted` takes, by its ID.
fn install_named(
    tasks: &Tasks,
    task: &crate::host_session::Key,
    app: &str,
    wanted: impl Fn(State) -> bool,
) -> Option<Token> {
    tasks
        .load(task)?
        .claims()
        .find_map(|(owned, state)| match owned {
            Owned::Install { id, app: named } if same_text(&named, app) && wanted(state) => {
                Some(id)
            }
            _ => None,
        })
}

/// Where the current task's own install of the app named `app` is in the
/// helper, while the helper says its folder there is still the one Atlas
/// placed; none for any other app, or when that cannot be told.
pub(crate) async fn installed_path(client: &Client, app: &str) -> Option<String> {
    let task = client.task.as_ref()?;
    let tasks = Tasks::at(client.output_dir().ok()?);
    let id = install_named(&tasks, task.key(), app, |state| state == State::Made)?;
    let answer = environment::install_states(client, std::slice::from_ref(&id))
        .await
        .ok()?;
    let found = answer
        .into_iter()
        .find(|found| found.install_id == id.as_str())?;
    if !found.verified() {
        return None;
    }
    Some(found.placed?.path.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        host_session::{Host, HostSession},
        task::Task,
        test_support::{Helper, UID},
    };
    use serde_json::json;
    use std::os::unix::fs::DirBuilderExt;

    /// A minimal bundle named `name` in `directory`.
    fn bundle_in(directory: &Path, name: &str) -> PathBuf {
        let bundle = directory.join(name);
        fs::create_dir_all(bundle.join("Contents/MacOS")).unwrap();
        fs::write(bundle.join("Contents/Info.plist"), "<plist/>").unwrap();
        bundle
    }

    #[tokio::test]
    async fn an_install_without_a_task_is_refused() {
        let directory = tempfile::tempdir().unwrap();
        let client = crate::test_support::client(directory.path());
        let bundle = bundle_in(directory.path(), "Probe.app");

        let error = install(&client, &bundle).await.unwrap_err();
        assert_eq!((error.code, error.uncertain), (Code::Usage, false));
        assert!(error.error.contains("ATLAS_SESSION"), "{error}");
    }

    #[test]
    fn only_a_real_app_bundle_is_installed() {
        let directory = tempfile::tempdir().unwrap();
        let good = bundle_in(directory.path(), "Probe.app");
        assert_eq!(bundle(&good).unwrap().1, "Probe.app");

        let hidden = bundle_in(directory.path(), ".Probe.app");
        let plain = directory.path().join("Probe");
        fs::DirBuilder::new().mode(0o700).create(&plain).unwrap();
        let empty = directory.path().join("Empty.app");
        fs::create_dir(&empty).unwrap();
        let link = directory.path().join("Link.app");
        std::os::unix::fs::symlink(&good, &link).unwrap();
        for refused in [
            hidden,
            plain,
            empty,
            link,
            directory.path().join("Missing.app"),
        ] {
            let error = bundle(&refused).unwrap_err();
            assert_eq!(error.code, "usage", "{}", refused.display());
        }
    }

    #[tokio::test]
    async fn the_same_app_installed_twice_by_one_task_names_the_earlier_install() {
        let directory = tempfile::tempdir().unwrap();
        let mut client = crate::test_support::client(directory.path());
        let host = HostSession::new(Host::Codex, "twice").unwrap();
        let task = Task::of(host.key(), Some("probe")).unwrap();
        client.host_session = Some(host);
        client.task = Some(task.clone());
        let earlier = Token::new();
        let output = client.output_dir().unwrap().to_owned();
        Claims::in_store(Tasks::at(&output), task)
            .claim(
                &Owned::Install {
                    id: earlier.clone(),
                    app: "Probe".into(),
                },
                State::Made,
            )
            .unwrap();

        let bundle = bundle_in(directory.path(), "Probe.app");
        let error = install(&client, &bundle).await.unwrap_err();
        assert_eq!(error.code, "input");
        assert_eq!(error.details["installId"], json!(earlier));
    }

    /// A client of a stand-in helper with a task, both kept in
    /// `directory`. The helper is the system's own `nobody`, an account
    /// that exists everywhere, so requests can be staged for it.
    fn tasked(directory: &Path) -> (Client, Helper, Task) {
        let nobody = nix::unistd::User::from_name("nobody").unwrap().unwrap();
        let (mut client, helper) = Helper::staging(directory, nobody.uid.as_raw());
        let host = HostSession::new(Host::Codex, "installing").unwrap();
        let task = Task::of(host.key(), Some("probe")).unwrap();
        client.host_session = Some(host);
        client.task = Some(task.clone());
        (client, helper, task)
    }

    /// The helper's answer to `request`: its apps, that it can read the
    /// bundle, and `worker` for the environment worker.
    fn answer(request: &Value, worker: &Option<Value>) -> Option<Value> {
        if request["op"] == "apps" {
            return Some(json!({"ok": true, "apps": []}));
        }
        if request["argv"][1] == "environment-worker" {
            return worker
                .as_ref()
                .map(|reply| crate::test_support::completed(&reply.to_string()));
        }
        Some(crate::test_support::completed("1\n"))
    }

    fn claimed(client: &Client, task: &Task) -> Vec<(Owned, State)> {
        Tasks::at(client.output_dir().unwrap())
            .load(task.key())
            .unwrap()
            .claims()
            .collect()
    }

    #[tokio::test]
    async fn an_install_the_helper_made_is_claimed_made_for_its_task() {
        let directory = tempfile::tempdir().unwrap();
        let (client, helper, task) = tasked(directory.path());
        let bundle = bundle_in(directory.path(), "Probe.app");
        let worker = Some(json!({"ok": true, "installed": true, "app": "Probe"}));

        let reply = helper
            .serve(install(&client, &bundle), |request| {
                answer(request, &worker)
            })
            .await
            .unwrap();
        assert_eq!(reply["installed"], true);
        let claims = claimed(&client, &task);
        assert_eq!(claims.len(), 1);
        assert!(matches!(&claims[0], (Owned::Install { app, .. }, State::Made) if app == "Probe"));
        assert!(client.controller.pending().unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_lost_install_reply_keeps_its_record_for_cleanup() {
        let directory = tempfile::tempdir().unwrap();
        let (client, helper, task) = tasked(directory.path());
        let bundle = bundle_in(directory.path(), "Probe.app");

        let error = helper
            .serve(install(&client, &bundle), |request| answer(request, &None))
            .await
            .unwrap_err();
        assert!(error.uncertain, "{error}");
        let claims = claimed(&client, &task);
        assert!(matches!(&claims[0], (Owned::Install { .. }, State::Intent)));
        assert!(!client.controller.pending().unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_refused_install_leaves_nothing_claimed() {
        let directory = tempfile::tempdir().unwrap();
        let (client, helper, task) = tasked(directory.path());
        let bundle = bundle_in(directory.path(), "Probe.app");
        let refusal = json!({"ok": false, "code": "input",
            "error": "The helper already has Applications/Probe.app."});
        let reply = helper
            .serve(install(&client, &bundle), |request| {
                if request["argv"][1] == "environment-worker" {
                    let mut refused = refusal.clone();
                    refused["refusal"] = json!(
                        request["argv"][2]
                            .as_str()
                            .and_then(|packet| Path::new(packet).parent()?.file_name()?.to_str())
                            .unwrap_or_default()
                    );
                    return Some(crate::test_support::exited(
                        environment::WORKER_REFUSED,
                        &refused.to_string(),
                    ));
                }
                answer(request, &None)
            })
            .await
            .unwrap();
        assert_eq!(
            (&reply["ok"], &reply["code"]),
            (&json!(false), &json!("input"))
        );
        assert!(reply.get("resumeRequired").is_none(), "{reply}");
        let claims = claimed(&client, &task);
        assert!(matches!(&claims[0], (Owned::Install { .. }, State::Gone)));
    }

    #[tokio::test]
    async fn a_refused_app_list_installs_and_claims_nothing() {
        let directory = tempfile::tempdir().unwrap();
        let (mut client, helper) = Helper::client(directory.path(), UID);
        let host = HostSession::new(Host::Codex, "refused").unwrap();
        let task = Task::of(host.key(), None).unwrap();
        client.host_session = Some(host);
        client.task = Some(task.clone());
        let bundle = bundle_in(directory.path(), "Probe.app");

        let error = helper
            .serve(install(&client, &bundle), |_| {
                Some(json!({"ok": false, "code": "busy", "error": "Busy", "uncertain": false}))
            })
            .await
            .unwrap_err();
        assert_eq!((error.code, error.uncertain), (Code::Busy, false));
        let output = client.output_dir().unwrap();
        assert!(Tasks::at(output).load(task.key()).is_none());
    }
}

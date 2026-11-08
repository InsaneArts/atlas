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
    handoff::{self, Target},
    task::{Claims, Owned, State, store::Store as Tasks},
};
use atlas_core::{
    error::{Code, Error, Result},
    files::Token,
    name::same_text,
};
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};

/// Install the app at `path` into the helper for the current task.
///
/// # Errors
///
/// Rejects an invalid bundle, unsafe destination or changed helper identity, and returns
/// copy or registration failures. Unknown installation retains its ownership record.
pub async fn install(client: &Client, path: &Path) -> Result<Value> {
    let (bundle, name) = bundle(path)?;
    let task = client.task.clone().ok_or_else(|| {
        Error::new(
            Code::Usage,
            "Atlas installs an app only for a task, so atlas cleanup can remove it. Set \
             ATLAS_SESSION=NAME before the task's first command, then install again.",
        )
    })?;
    // Before anything is made or the helper asked: the person's sign-ins and secrets never reach it, by the path
    // given or the bundle it leads to.
    let target = Target::Path(bundle);
    crate::credential_files::refuse([path])?;
    handoff::refuse_targets(std::slice::from_ref(&target))?;
    let app = atlas_core::name::bounded(name.trim_end_matches(".app"));
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
        .with("installId", serde_json::json!(earlier)));
    }

    // Copying, installing and removing the copy happen under one ownership,
    // so no other open, close or cleanup interleaves.
    let ownership = Arc::new(client.controller.acquire(false)?);
    let listed = client.running_apps().await?;
    let running = listed.rows();

    let id = Token::new();
    let install = Owned::Install {
        id: id.clone(),
        app: app.clone(),
    };
    claims.claim(&install, State::Intent)?;

    let handed = match handoff::hand_over(client, &ownership, vec![target], running).await {
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
        Ok(reply) => environment::placed_app(reply),
    };
    let mut settled = vec![(install, if made { State::Made } else { State::Gone })];
    if let Some(token) = transport {
        let session = task.session().clone();
        let gone = matches!(
            handoff::remove_copy(Path::new(handoff::ROOT), &token, &session, running),
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
mod tests;

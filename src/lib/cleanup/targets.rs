//! What a cleanup's quit step signals: the processes running from the
//! task's own bundles, each chosen by where it runs from, never by its
//! name, and only from a bundle proven to be the task's right now: a copy
//! folder labelled for this session (or for none), an install whose
//! destination still holds the exact folder Atlas placed. Each carries the
//! node its bundle was proven to have, which the helper checks again
//! before it signals.
use super::plan::Bundle;
use crate::{
    app_selection::{Process, Within},
    control::Token,
    handoff,
    host_session::Key,
};
use serde_json::{Value, json};
use std::path::Path;

/// What the quit step signals and waits for, and what it leaves.
#[derive(Debug, Default)]
pub(crate) struct Targets {
    pub processes: Vec<Process>,
    /// The proven bundles' folders, which nothing must run from once done.
    pub folders: Vec<String>,
    /// The apps that may run from an install through a copy macOS moved
    /// them to that cannot be traced to it: never signalled.
    pub untraced: Vec<String>,
}

/// The installs among `bundles`, by their IDs, each once.
pub(crate) fn installs_of(bundles: &[Bundle]) -> Vec<Token> {
    let mut ids: Vec<Token> = Vec::new();
    for bundle in bundles {
        if let Bundle::Install(id) = bundle
            && !ids.contains(id)
        {
            ids.push(id.clone());
        }
    }
    ids
}

/// What the helper said of install `id` in `installs`, its answer.
pub(crate) fn install_in<'a>(installs: &'a Value, id: &Token) -> Option<&'a Value> {
    installs["installs"]
        .as_array()?
        .iter()
        .find(|install| install["installId"] == id.as_str())
}

/// What the quit step signals for `bundles` of `session`'s work: the apps
/// in `apps` (a list the helper gave) running from a proven bundle, and
/// the other processes `installs` (the helper's word on them) says run
/// from a proven install.
pub(crate) fn targets(
    copies: &Path,
    session: &Key,
    bundles: &[Bundle],
    installs: &Value,
    apps: &Value,
) -> Targets {
    let mut proven: Vec<Within> = Vec::new();
    let mut targets = Targets::default();
    for bundle in bundles {
        let within = match bundle {
            Bundle::Copy(token) => handoff::copy_node(copies, token, session).map(|node| Within {
                folder: handoff::copy_folder(copies, token)
                    .to_string_lossy()
                    .into_owned(),
                node,
            }),
            Bundle::Install(id) => install_in(installs, id).and_then(proven_install),
        };
        let Some(within) = within else {
            continue;
        };

        if let Bundle::Install(id) = bundle
            && let Some(install) = install_in(installs, id)
        {
            for process in install["processes"].as_array().into_iter().flatten() {
                if let Some(process) = helper_process(process, &within) {
                    targets.processes.push(process);
                }
            }
            for process in install["unproven"].as_array().into_iter().flatten() {
                let app = install["app"].as_str().unwrap_or("an app").to_owned();
                if process["pid"].is_u64() && !targets.untraced.contains(&app) {
                    targets.untraced.push(app);
                }
            }
        }
        proven.push(within);
    }

    for app in apps["apps"].as_array().into_iter().flatten() {
        let (Some(pid), Some(executable)) = (app["pid"].as_u64(), app["executable"].as_str())
        else {
            continue;
        };
        let Some(within) = proven
            .iter()
            .find(|within| handoff::runs_from(executable, Path::new(&within.folder)))
        else {
            continue;
        };
        if pid > 0 && !targets.processes.iter().any(|known| known.pid == pid) {
            targets.processes.push(Process {
                pid,
                executable: executable.to_owned(),
                name: app["name"].as_str().unwrap_or("").to_owned(),
                protected: false,
                within: Some(within.clone()),
            });
        }
    }
    targets.folders = proven.into_iter().map(|within| within.folder).collect();
    targets
}

/// An install's folder and node, when the helper verified that its
/// destination holds the exact folder Atlas placed.
fn proven_install(install: &Value) -> Option<Within> {
    if install["verified"] != true {
        return None;
    }
    let folder = install["path"].as_str()?.to_owned();
    let node = format!(
        "{}:{}",
        install["root"]["device"].as_u64()?,
        install["root"]["inode"].as_u64()?
    );
    Some(Within { folder, node })
}

/// A process the helper listed as running from an install, to signal only
/// while that install is still the one proven.
fn helper_process(process: &Value, within: &Within) -> Option<Process> {
    let executable = process["executable"].as_str()?;
    Some(Process {
        pid: process["pid"].as_u64().filter(|pid| *pid > 0)?,
        executable: executable.to_owned(),
        name: Path::new(executable)
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default(),
        protected: false,
        within: Some(within.clone()),
    })
}

/// The app processes in `apps` (a list the helper gave) that still run
/// from one of `folders`.
pub(crate) fn still_running(apps: &Value, folders: &[String]) -> Vec<Value> {
    apps["apps"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|app| {
            app["executable"].as_str().is_some_and(|executable| {
                folders
                    .iter()
                    .any(|folder| handoff::runs_from(executable, Path::new(folder)))
            })
        })
        .map(|app| json!({"app": app["name"], "pid": app["pid"]}))
        .collect()
}

/// The processes `installs` (the helper's word on them) says still run
/// from a verified install.
pub(crate) fn still_in_installs(installs: &Value) -> usize {
    installs["installs"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|install| install["verified"] == true)
        .map(|install| install["processes"].as_array().map_or(0, Vec::len))
        .sum()
}

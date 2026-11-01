//! Which process a command about an app is for: a process ID, or a name
//! found among the helper's windows and running apps.
use super::windows::verified_window_snapshot;
use crate::{
    actions::Expect,
    client::{Client, Verified},
    error::{Code, Error, Result},
    native::windows::{MENU_LAYER, Shown, Window},
    process::{Instance, Start},
    ui::text::same_text,
};
use serde_json::Value;
use std::collections::BTreeMap;

/// Bind the target to the identity read with its helper snapshot. Older
/// helpers omit identities: the input service then pins at admission.
/// A current snapshot without this PID must never adopt its replacement.
pub(super) fn snapshot_expect(pid: i64, starts: Option<&BTreeMap<i64, Start>>) -> Result<Expect> {
    let expect = Expect::new(pid)?;
    let Some(starts) = starts else {
        return Ok(expect);
    };
    let start = starts
        .get(&pid)
        .copied()
        .filter(|start| start[0] > 0 && start[1] < 1_000_000)
        .ok_or_else(|| {
            Error::new(
                Code::Elements,
                "The app's process changed while it was being resolved; look again",
            )
        })?;
    Expect::from_instance(Instance::new(pid as i32, start))
}

/// An app's identity from the helper's app list, when every process of
/// that name agrees on its bundle. Ambiguous names never borrow an ID.
pub(super) fn named_app<'a>(apps: &'a Value, name: &str) -> Option<&'a Value> {
    let mut named = apps["apps"]
        .as_array()?
        .iter()
        .filter(|app| app["name"].as_str().is_some_and(|app| same_text(app, name)));
    let first = named.next()?;
    named
        .all(|app| app["bundleId"] == first["bundleId"])
        .then_some(first)
}

/// The app an element command is about: a PID, or a name that must resolve
/// to exactly one process with a window of its own on screen.
#[derive(Debug, Clone)]
pub enum Target {
    /// The app with this name, whatever its case.
    Name(String),
    /// The process with this ID.
    Pid(i64),
}

/// The processes named `name` that have windows on screen, by the kind of
/// window: ordinary ones first, then floating panels and alerts, then open
/// menus. An app showing only one of those is found by it.
pub(super) fn app_pids(windows: &[Window], name: &str) -> Vec<i64> {
    let candidates: Vec<&Window> = windows
        .iter()
        .filter(|window| {
            window.is_apps_or_menu()
                && window
                    .owner
                    .as_deref()
                    .is_some_and(|owner| same_text(owner, name))
        })
        .collect();

    let tiers: [&dyn Fn(&Window) -> bool; 3] = [
        &|window| window.layer == 0,
        &|window| window.layer != MENU_LAYER,
        &|_| true,
    ];

    let mut pids: Vec<i64> = tiers
        .iter()
        .map(|tier| {
            candidates
                .iter()
                .filter(|window| tier(window))
                .map(|window| window.pid)
                .collect::<Vec<_>>()
        })
        .find(|pids| !pids.is_empty())
        .unwrap_or_default();
    pids.sort_unstable();
    pids.dedup();

    pids
}

/// The one process among `pids` a command about `name` is for.
pub(super) fn one_pid(name: &str, pids: &[i64]) -> Result<i64> {
    match pids {
        [pid] => Ok(*pid),
        [] => Err(Error::new(
            Code::Elements,
            format!("No window of {name} is on screen"),
        )),
        _ => Err(Error::new(
            Code::Elements,
            format!("Several {name} processes have windows ({pids:?}); pass --pid"),
        )),
    }
}

/// The process a command is about: the one with the app's windows, or,
/// when none of its windows is open, the active app of that name (its
/// menu bar is still on screen). Several apps of the name with windows
/// stay ambiguous. It is found in the helper `verified` describes, which
/// the caller verified just before, so it is not verified again for each
/// list it reads.
pub(super) async fn target_pid(
    client: &Client,
    verified: &Verified,
    target: &Target,
) -> Result<i64> {
    Ok(target_expect(client, verified, target).await?.pid())
}

/// Resolve input and named clicks through the same helper snapshot.
pub(super) async fn target_expect(
    client: &Client,
    verified: &Verified,
    target: &Target,
) -> Result<Expect> {
    let listed = match target {
        Target::Pid(_) => None,
        Target::Name(_) => Some(verified_window_snapshot(client, verified).await?),
    };
    target_among(client, verified, target, listed.as_ref()).await
}

/// Resolve with an already-read window snapshot when available.
pub(super) async fn target_among(
    client: &Client,
    verified: &Verified,
    target: &Target,
    listed: Option<&Shown>,
) -> Result<Expect> {
    if let Target::Name(name) = target
        && let Some(listed) = listed
    {
        let pids = app_pids(&listed.windows, name);
        if !pids.is_empty() {
            return snapshot_expect(one_pid(name, &pids)?, listed.starts.as_ref());
        }
    }

    let apps = client.apps_in(verified).await?;
    let starts: Option<BTreeMap<i64, Start>> = apps
        .get("starts")
        .map(|value| serde_json::from_value(value.clone()))
        .transpose()
        .map_err(|_| {
            Error::new(
                Code::Protocol,
                "The helper's process identities were malformed",
            )
        })?;
    // apps_in returns only replies whose apps field passed array validation.
    let rows = apps["apps"].as_array().expect("validated app list");
    let name = match target {
        Target::Pid(pid) => {
            if !rows.iter().any(|app| app["pid"] == *pid) {
                return Err(Error::new(
                    Code::Elements,
                    format!(
                        "No app in the helper has the process ID {pid}; run atlas apps to see them"
                    ),
                ));
            }
            return snapshot_expect(*pid, starts.as_ref());
        }
        Target::Name(name) => name,
    };
    let named: Vec<&Value> = rows.iter().filter(|app| app_named(app, name)).collect();
    if let Some(pid) = named
        .iter()
        .find(|app| app["active"] == true)
        .and_then(|app| app["pid"].as_i64())
    {
        return snapshot_expect(pid, starts.as_ref());
    }
    Err(Error::new(
        Code::Elements,
        if named.is_empty() {
            format!("{name} is not running: run atlas open --app {name}")
        } else {
            format!(
                "No window of {name} is on screen: open one in it, or run atlas wait --app {name}"
            )
        },
    ))
}

/// Whether an `apps` row is the app called `name`: by the name it shows,
/// which may be localized, or by the file name of its executable or of its
/// own (innermost) bundle.
pub(super) fn app_named(app: &Value, name: &str) -> bool {
    let shown = app["name"].as_str().is_some_and(|own| same_text(own, name));
    let executable = app["executable"].as_str().map(std::path::Path::new);
    let bundle = executable.and_then(|path| {
        path.ancestors()
            .find(|ancestor| ancestor.extension().is_some_and(|ext| ext == "app"))
    });

    shown
        || [executable, bundle]
            .into_iter()
            .flatten()
            .filter_map(|path| path.file_stem()?.to_str())
            .any(|own| same_text(own, name))
}

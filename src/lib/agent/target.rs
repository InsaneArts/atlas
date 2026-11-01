//! Which process a command about an app is for: a process ID, or a name
//! found among the helper's windows and running apps.
use super::windows::{DockRead, window_snapshot};
use crate::{
    actions::Expect,
    client::{AppList, AppRow, Client, Verified},
    error::{Code, Error, Result},
    name::same_text,
    process::{Instance, Start},
    screen::windows::{MENU_LAYER, Shown, Window},
};
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
pub(super) fn named_app<'a>(apps: &'a AppList, name: &str) -> Option<&'a AppRow> {
    let mut named = apps
        .rows()
        .iter()
        .filter(|app| app.name().is_some_and(|app| same_text(app, name)));
    let first = named.next()?;
    named
        .all(|app| app.bundle_id() == first.bundle_id())
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
    let tier = |window: &Window| match window.layer {
        0 => 0,
        MENU_LAYER => 2,
        _ => 1,
    };
    let best = candidates.iter().map(|window| tier(window)).min();
    let mut pids: Vec<i64> = candidates
        .into_iter()
        .filter(|window| Some(tier(window)) == best)
        .map(|window| window.pid)
        .collect();
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

/// Resolve input and named clicks through the same helper snapshot.
pub(super) async fn target_expect(
    client: &Client,
    verified: &Verified,
    target: &Target,
) -> Result<Expect> {
    let listed = match target {
        Target::Pid(_) => None,
        Target::Name(_) => Some(window_snapshot(client, verified, DockRead::Skip).await?),
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
    let starts = apps.starts()?;
    let rows = apps.rows();
    let name = match target {
        Target::Pid(pid) => {
            if !rows.iter().any(|app| app.pid() as i64 == *pid) {
                return Err(Error::new(
                    Code::Elements,
                    format!(
                        "No app in the helper has the process ID {pid}; run atlas apps to see them"
                    ),
                ));
            }
            return snapshot_expect(*pid, starts);
        }
        Target::Name(name) => name,
    };
    let mut named = rows.iter().filter(|app| app_named(app, name)).peekable();
    let absent = named.peek().is_none();
    if let Some(app) = named.find(|app| app.active()) {
        return snapshot_expect(app.pid() as i64, starts);
    }
    Err(Error::new(
        Code::Elements,
        if absent {
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
pub(super) fn app_named(app: &AppRow, name: &str) -> bool {
    let shown = app.name().is_some_and(|own| same_text(own, name));
    let executable = app.executable().map(std::path::Path::new);
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

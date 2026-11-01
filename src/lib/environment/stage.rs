use super::*;

/// Whether `stage` holds a retained request: a step whose outcome is not
/// known yet, kept as evidence until it is. A folder or entry that cannot
/// be read may hold one, so it counts as holding one.
pub(super) fn requests(stage: &Path) -> bool {
    let Ok(entries) = fs::read_dir(stage) else {
        return true;
    };

    entries.into_iter().any(|entry| {
        entry.map_or(true, |entry| {
            entry.file_name().to_string_lossy().starts_with(CHANGE)
        })
    })
}

/// Atlas's own plan and request folders beside the shared command: named
/// with a valid ID, real folders owned by this account. Anything else there
/// is left alone.
pub fn leftovers(shared: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(shared) else {
        return Vec::new();
    };

    entries
        .flatten()
        .filter(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let id = name
                .strip_prefix("environment-plan-")
                .or_else(|| name.strip_prefix("environment-run-"));

            id.is_some_and(|id| validate_id(id).is_ok())
                && fs::symlink_metadata(entry.path()).is_ok_and(|metadata| {
                    metadata.file_type().is_dir()
                        && metadata.uid() == nix::unistd::getuid().as_raw()
                })
        })
        .map(|entry| entry.path())
        .collect()
}

/// What a folder being removed is first renamed to, so no command ever sees
/// it half removed; one left behind holds nothing anyone needs.
pub(super) const DISCARDED: &str = ".environment-discarded-";

/// What a stage is made under before it is locked and moved into place, so
/// no command sees it unlocked. A sweep never looks at it.
pub(super) const STAGING: &str = ".environment-staging-";

/// Remove a frozen plan the helper no longer needs, called under its lock:
/// it is moved to a discarded name first, so its own name is gone at once
/// and nothing is left there half removed. Where it cannot be moved it is
/// removed where it is.
pub(super) fn discard(stage: &Path) {
    let discarded = stage
        .parent()
        .map(|parent| parent.join(format!("{DISCARDED}{}", crate::files::token())));
    match discarded {
        Some(discarded) if fs::rename(stage, &discarded).is_ok() => {
            let _ = fs::remove_dir_all(discarded);
        }
        _ => {
            let _ = fs::remove_dir_all(stage);
        }
    }
}

/// Remove what a [`discard`] cut short left under a discarded name: Atlas's
/// own real folders, which no command uses once renamed.
pub(super) fn clear_discarded(shared: &Path) {
    let Ok(entries) = fs::read_dir(shared) else {
        return;
    };

    for entry in entries.flatten() {
        let ours = entry.file_name().to_string_lossy().starts_with(DISCARDED)
            && fs::symlink_metadata(entry.path()).is_ok_and(|metadata| {
                metadata.file_type().is_dir() && metadata.uid() == nix::unistd::getuid().as_raw()
            });
        if ours {
            let _ = fs::remove_dir_all(entry.path());
        }
    }
}

/// How long a frozen plan is kept before the helper is asked whether it is
/// still needed.
pub(super) const UNUSED: Duration = Duration::from_secs(7 * 24 * 60 * 60);

/// How old a plan folder with no plan and no request whose outcome is
/// unknown must be before it is taken for a preview cut short rather than
/// one still being made. A preview holds its folder's lock while it runs,
/// and none takes this long.
pub(super) const STARTING: Duration = Duration::from_secs(10 * 60);

/// Remove what earlier environment commands left behind that nothing can
/// use: [`prune_runs`], [`sweep`] and [`clear_discarded`].
pub(super) fn tidy(client: &Client) {
    if let Some(shared) = client.config.identity_probe.parent() {
        prune_runs(shared);
        sweep(shared);
        clear_discarded(shared);
    }
}

/// Remove previews cut short, a killed command's: plan folders older than
/// [`STARTING`] with no plan and no request whose outcome is unknown, whose
/// lock nothing holds. They hold only copies of the selected files, which
/// nothing can apply.
pub(super) fn sweep(shared: &Path) {
    for (_, stage) in stale(shared, STARTING) {
        // Looked at first, so a folder that is kept is never touched, then
        // again under the folder's own lock, which a preview being made holds.
        if !abandoned(&stage) {
            continue;
        }
        let Ok(lock) = lock_stage(&stage) else {
            continue;
        };
        if abandoned(&stage) {
            discard(&stage);
        }

        drop(lock);
    }
}

/// [`tidy`], then remove frozen plans older than UNUSED that the helper
/// never started (it answers that nothing was copied with their ID). Plans
/// applied or undone completely were removed when that finished; one with a
/// retained request (a step whose outcome is unknown), one the helper
/// started, or one it cannot answer for, stays. A plan removed is settled
/// gone wherever a task claimed it.
pub(super) async fn prune(client: &Client, helper: &str) {
    let Some(shared) = client.config.identity_probe.parent() else {
        return;
    };

    tidy(client);

    let mut removed = Vec::new();
    for (id, stage) in stale(shared, UNUSED) {
        // Everything below happens under the plan's own lock; a plan in use
        // is skipped.
        let Ok(lock) = lock_stage(&stage) else {
            continue;
        };

        if !prunable(&stage, &id, client.config.helper_uid) {
            continue;
        }

        // A read-only question: has the helper ever started this plan?
        let answer = dispatch(
            client,
            helper,
            &stage,
            Request::Rollback {
                id: id.clone(),
                apply: false,
            },
        )
        .await;
        let never = answer
            .as_ref()
            .is_ok_and(|answer| answer["ok"] == false && answer["error"] == worker::MISSING);
        if never && !requests(&stage) {
            discard(&stage);
            if fs::symlink_metadata(&stage).is_err()
                && let Some(id) = crate::files::Token::parse(&id)
            {
                removed.push(task::Owned::Plan { id });
            }
        }

        drop(lock);
    }
    task::settle_gone(client, &removed);
}

/// Remove the folders of commands run older than UNUSED. A command kept
/// its folder only when it was stopped or its reply was lost; no command
/// runs longer than ten minutes, so a week-old folder is no longer read.
pub(super) fn prune_runs(shared: &Path) {
    for stage in stale_runs(shared, UNUSED) {
        discard(&stage);
    }
}

/// The frozen plans beside the shared command older than `age`, with their IDs.
pub(super) fn stale(shared: &Path, age: Duration) -> Vec<(String, PathBuf)> {
    older(shared, "environment-plan-", age)
}

/// The command folders beside the shared command older than `age`.
pub(super) fn stale_runs(shared: &Path, age: Duration) -> Vec<PathBuf> {
    older(shared, "environment-run-", age)
        .into_iter()
        .map(|(_, stage)| stage)
        .collect()
}

/// Atlas's folders beside the shared command named with `prefix` and older
/// than `age`, with their IDs.
pub(super) fn older(shared: &Path, prefix: &str, age: Duration) -> Vec<(String, PathBuf)> {
    leftovers(shared)
        .into_iter()
        .filter_map(|stage| {
            let name = stage.file_name()?.to_string_lossy().into_owned();
            let id = name.strip_prefix(prefix)?.to_owned();
            let elapsed = fs::symlink_metadata(&stage)
                .ok()?
                .modified()
                .ok()?
                .elapsed()
                .ok()?;

            (elapsed > age).then_some((id, stage))
        })
        .collect()
}

/// Whether `stage` is a preview cut short: copies of the selected files but
/// no plan, so nothing can apply it, and no request whose outcome is
/// unknown, so the helper has nothing to recover from it.
pub(super) fn abandoned(stage: &Path) -> bool {
    !planned(stage) && !requests(stage)
}

/// Whether the helper may be asked about the plan in `stage`: a readable
/// plan for this ID and helper, with no request whose outcome is unknown.
pub(super) fn prunable(stage: &Path, id: &str, uid: u32) -> bool {
    let ours = read_limited(&stage.join("plan.json"), MAX_RECORD)
        .ok()
        .and_then(|bytes| decode::<Plan>(&bytes).ok())
        .is_some_and(|plan| plan.id == id && plan.uid == uid);

    ours && !requests(stage)
}

pub(super) fn stage_path(client: &Client, id: &str) -> Result<PathBuf> {
    validate_id(id)?;

    let parent = client
        .config
        .identity_probe
        .parent()
        .ok_or_else(|| problem("Missing shared directory"))?;
    let directory =
        directory(parent, false)?.ok_or_else(|| problem("Shared staging directory is missing"))?;
    installation_parent(&directory)?;

    if has_acl(parent)? {
        return Err(problem("Shared staging parent must not have an ACL"));
    }

    Ok(parent.join(format!("environment-plan-{id}")))
}

/// Make a private folder the helper may read, or nothing.
pub(super) fn create_stage(stage: &Path, helper: &str) -> Result<()> {
    fs::DirBuilder::new()
        .mode(0o700)
        .create(stage)
        .map_err(|error| Error::writing(stage, error))?;

    let shared = remove_inherited_acl(stage).and_then(|()| allow_helper(stage, helper, true));
    if shared.is_err() {
        let _ = fs::remove_dir(stage);
    }

    shared
}

/// Make a private folder the helper may read at `stage`, holding its lock.
/// It is made and locked under a name no sweep looks at, then moved into
/// place, so no other command ever finds it unlocked.
pub(super) fn create_locked_stage(stage: &Path, helper: &str) -> Result<LockFile> {
    create_locked_stage_with(stage, helper, |_| {})
}

/// [`create_locked_stage`], calling `made` with the folder once it is made
/// and before it is locked.
pub(super) fn create_locked_stage_with(
    stage: &Path,
    helper: &str,
    made: impl FnOnce(&Path),
) -> Result<LockFile> {
    let parent = stage
        .parent()
        .ok_or_else(|| problem("Missing shared directory"))?;
    let staging = parent.join(format!("{STAGING}{}", crate::files::token()));
    create_stage(&staging, helper)?;
    made(&staging);

    let placed = lock_stage(&staging).and_then(|lock| {
        fs::rename(&staging, stage)
            .map(|()| lock)
            .map_err(|error| Error::writing(stage, error))
    });
    if placed.is_err() {
        let _ = fs::remove_dir_all(&staging);
    }

    placed
}

pub(super) fn lock_stage(stage: &Path) -> Result<LockFile> {
    let directory = directory(stage, false)?.ok_or_else(|| problem("Frozen plan is missing"))?;
    installation_parent(&directory)?;

    if directory.metadata()?.mode() & 0o7777 != 0o700 {
        return Err(problem("Frozen plan directory is not private"));
    }

    let lock = open_regular(&stage.join("controller.lock"), true)?
        .ok_or_else(|| problem("Plan lock is missing"))?;
    validate_private(&lock, false)?;

    let lock = LockFile(lock);
    // A listing holds a shared lock for an instant.
    crate::files::lock_within(&lock, crate::files::Lock::Exclusive, STAGE_PATIENCE)
        .map_err(|_| problem("This environment plan is already running"))?;

    Ok(lock)
}

/// What a request folder that may change the helper is named with: one
/// left behind holds a step whose outcome is not known yet.
pub(super) const CHANGE: &str = "request-";

/// What a read-only request folder is named with: one left behind (its
/// command killed mid-read) holds nothing to recover.
pub(super) const READ: &str = "read-";

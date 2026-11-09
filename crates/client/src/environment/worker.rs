//! Runs inside the selected login. Every mutation is guarded and journaled.
use super::{
    model::*,
    reply::{TransactionSummary, Transactions},
    state::Store,
    *,
};
use atlas_core::files::Lock;
use atlas_core::lenient::Lenient;
use changing::{Changing, Installation, RollbackState};

mod changing;

/// The helper's answer for a plan it never applied.
pub(super) const MISSING: &str =
    "Nothing was copied into the helper with this ID, so there is nothing to undo";

/// Internal worker; invoked through the verified helper, never through sudo.
///
/// # Errors
///
/// Rejects malformed or stale requests and invalid helper identity, paths or ownership.
/// Partial filesystem effects remain recorded for recovery.
pub fn worker(path: &Path, apply: bool) -> Result<Value> {
    let packet: Packet = decode(&read_limited(path, MAX_RECORD)?)?;
    if packet.version != 1 {
        return Err(problem("Unsupported environment packet version"));
    }

    let guard = || {
        let current = Identity::from_proof(&atlas_native::native::session_info()?)?;
        if current != packet.expected {
            return Err(problem(
                "Expected helper session or display changed; no further mutation is allowed",
            ));
        }
        Ok(())
    };

    guard()?;
    let home = account_home(packet.expected.uid)?;
    if apply != packet.request.changes_helper() {
        return Err(problem("Worker apply flag does not match its request"));
    }

    match packet.request {
        Request::Preview { id, files } => preview(&home, packet.expected.uid, &id, files, &guard),
        Request::Apply { plan } => apply_plan(&home, &plan, &guard),
        Request::Rollback { id, apply } => rollback(&home, &id, apply, &guard),
        Request::Run { argv } => runner::execute(&home, &argv, &guard),
        Request::Transactions { all, ids } => transactions(&home, all, &ids, &guard),
        Request::InstallApp { id, source, name } => {
            installs::install(&home, &id, &source, &name, &guard)
        }
        Request::RemoveApp { id } => {
            installs::remove(&home, &id, &guard, &installs::Looks::system())
        }
        Request::Installs { ids } => {
            installs::states(&home, &ids, &guard, &installs::Looks::system())
        }
    }
}

/// The most transactions listed; `more` counts those left unread.
const LISTED: usize = 200;

/// The transactions in the helper, newest first, at most [`LISTED`]: each
/// one's ID, phase, whether it is the one that must be resumed or rolled
/// back first, when its record last changed, and each file's target and
/// operation. Those rolled back completely are counted as `hidden`, not
/// listed, unless `all`, so they never push older unfinished ones out. Each
/// of `ids` (the frozen plans the asker holds) is listed whatever its phase
/// and age, so the asker can tell which the helper started. File contents,
/// digests, fingerprints, sources and copies are never included, and a
/// record that does not read is counted, not listed. It opens existing
/// state only, creates nothing, and holds the store's shared lock only
/// while it reads one record, so an operation that wants the store waits
/// no longer than that.
pub(super) fn transactions(
    home: &Path,
    all: bool,
    ids: &[String],
    guard: &impl Fn() -> Result<()>,
) -> Result<Value> {
    guard()?;
    if ids.len() > LISTED {
        return Err(problem("Too many plans were asked about"));
    }
    for id in ids {
        validate_id(id)?;
    }
    let (mut recorded, active) = {
        let store = Store::open(home, Lock::Shared)?;
        let read = (recorded_transactions(&store.path)?, store.active()?);
        store.confirm_read()?;
        read
    };
    recorded
        .found
        .sort_by_key(|(_, updated)| std::cmp::Reverse(*updated));

    let asked = |id: &String| ids.contains(id);
    let summary = |id: &str, updated_at, transaction: Transaction| {
        Lenient::Known(TransactionSummary {
            plan_id: id.to_owned(),
            phase: transaction.phase.into(),
            active: active.as_deref() == Some(id),
            updated_at,
            files: file_report(&transaction.plan),
            retained_directories: transaction.retained_directories.len(),
        })
    };
    let (mut listed, mut hidden, mut unreadable, mut more) =
        (Vec::new(), 0, recorded.unreadable, 0);
    for (id, updated) in &recorded.found {
        let full = listed.len() >= LISTED;
        if full && !asked(id) {
            more += 1;
            continue;
        }
        match Store::open(home, Lock::Shared)?.transaction(id) {
            Ok(Some(transaction))
                if !all && !asked(id) && transaction.phase == Phase::RolledBack =>
            {
                hidden += 1;
            }
            Ok(Some(transaction)) => listed.push(summary(id, *updated, transaction)),
            Ok(None) | Err(_) => unreadable += 1,
        }
    }

    // An asked plan the listing did not come across is read by its own ID,
    // so one it missed is never taken for one the helper never started.
    for id in ids.iter().filter(|id| !recorded.seen.contains(id)) {
        let store = Store::open(home, Lock::Shared)?;
        match store.transaction(id) {
            Ok(Some(transaction)) => {
                let updated = changed_ms(&store.folder(id).join("transaction.json"));
                listed.push(summary(id, updated.unwrap_or(0), transaction));
            }
            Ok(None) => {}
            Err(_) => unreadable += 1,
        }
    }

    Ok(serde_json::to_value(Transactions {
        ok: true,
        transactions: listed,
        hidden,
        more,
        unreadable,
    })?)
}

/// What the store's folder holds of transactions, as listed.
struct Recorded {
    /// Each transaction whose record is there, by ID, with when its record
    /// last changed in milliseconds since 1970, or 0 when that cannot be
    /// told.
    found: Vec<(String, u64)>,
    /// The IDs listed, whether their record could be looked at or not.
    seen: Vec<String>,
    /// How many entries could not be looked at.
    unreadable: u64,
}

/// The transactions in the store at `path`. A store not made yet holds
/// none, and a folder without a record holds a preparation that never
/// started, and is left out; every other entry that cannot be looked at
/// is counted, never left out.
fn recorded_transactions(path: &Path) -> Result<Recorded> {
    let mut recorded = Recorded {
        found: Vec::new(),
        seen: Vec::new(),
        unreadable: 0,
    };
    let entries = match fs::read_dir(path) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(recorded),
        Err(error) => return Err(error.into()),
    };

    for entry in entries {
        let Ok(entry) = entry else {
            recorded.unreadable += 1;
            continue;
        };
        let name = entry.file_name();
        let Some(id) = name
            .to_str()
            .and_then(|name| name.strip_prefix("transaction-"))
            .filter(|id| validate_id(id).is_ok())
        else {
            continue;
        };
        recorded.seen.push(id.to_owned());
        let folder = fs::symlink_metadata(entry.path());
        if !folder.is_ok_and(|folder| folder.is_dir()) {
            recorded.unreadable += 1;
            continue;
        }
        match fs::symlink_metadata(entry.path().join("transaction.json")) {
            Ok(record) => recorded
                .found
                .push((id.to_owned(), changed_ms_of(&record).unwrap_or(0))),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => recorded.unreadable += 1,
        }
    }
    Ok(recorded)
}

fn account_home(uid: u32) -> Result<PathBuf> {
    let account = atlas_core::account::account_with_uid(uid)
        .map_err(|cause| problem("Cannot resolve the helper account home").with_source(cause))?
        .ok_or_else(|| problem("Cannot resolve the helper account home"))?;
    if account.uid != uid {
        return Err(problem("Invalid helper account record"));
    }

    let home = fs::canonicalize(&account.home)?;
    let directory = directory(&home, false)?.ok_or_else(|| problem("Helper home is missing"))?;
    installation_parent(&directory)?;

    if directory.metadata()?.uid() != uid {
        return Err(problem("Helper home ownership mismatch"));
    }

    Ok(home)
}

pub(super) fn preview(
    home: &Path,
    uid: u32,
    id: &str,
    files: Vec<Payload>,
    guard: &impl Fn() -> Result<()>,
) -> Result<Value> {
    validate_id(id)?;
    guard()?;

    let store = Store::open(home, Lock::Shared)?;
    store.require_available(id)?;
    let ledger = store.ledger()?;
    validate_files(files.iter())?;

    let mut planned = Vec::new();
    let mut total = 0;

    for payload in files {
        guard()?;
        separate_from_ledger(&ledger, &payload.target)?;
        installable_parents(home, &payload.target)?;

        let bytes = checked_bytes(&payload.source, &payload.sha256, "Frozen payload changed")?;
        let current = snapshot(&destination(home, &payload.target)?)?;
        total += bytes.len() as u64 + current.as_ref().map_or(0, |(_, bytes)| bytes.len() as u64);
        memory_limit(total)?;

        let before = current.map(|(fingerprint, _)| fingerprint);
        let previous = ledger.files.get(&payload.target).cloned();
        let operation = operation(&payload, before.as_ref(), previous.as_ref());
        planned.push(PlannedFile {
            payload,
            before,
            previous,
            operation,
        });
    }

    guard()?;
    store.confirm_read()?;
    let plan = Plan {
        version: 1,
        id: id.into(),
        uid,
        home: home.into(),
        files: planned,
    };

    let ok = plan
        .files
        .iter()
        .all(|file| file.operation != Operation::Conflict);

    Ok(json!({
        "ok": ok,
        "applied": false,
        "planId": id,
        "files": file_report(&plan),
        "plan": plan,
    }))
}

fn operation(
    payload: &Payload,
    current: Option<&Fingerprint>,
    previous: Option<&Entry>,
) -> Operation {
    let managed_matches = match (previous, current) {
        (Some(previous), Some(current)) => previous.fingerprint == *current,
        (None, _) => true,
        (Some(_), None) => false,
    };

    if !managed_matches {
        return Operation::Conflict;
    }

    match current {
        None => Operation::Create,
        Some(current) if current.sha256 == payload.sha256 => {
            if current.mode == desired_mode(payload) {
                Operation::Unchanged
            } else {
                Operation::Permissions
            }
        }
        Some(_) if previous.is_some() => Operation::Update,
        Some(_) => Operation::Conflict,
    }
}

fn validate_files<'a>(files: impl ExactSizeIterator<Item = &'a Payload> + Clone) -> Result<()> {
    if files.len() == 0 || files.len() > MAX_SELECTIONS {
        return Err(problem("Select 1–32 files"));
    }

    unique_targets(files.clone().map(|file| file.target.as_path()))?;

    for file in files {
        if !file.source.is_absolute() || !atlas_core::name::lower_hex(&file.sha256, 64) {
            return Err(problem("Invalid frozen file selection"));
        }
    }

    Ok(())
}

fn checked_bytes(path: &Path, sha256: &str, changed: &str) -> Result<Vec<u8>> {
    let bytes = read_limited(path, MAX_FILE)?;
    if digest(&bytes) != sha256 {
        return Err(problem(changed));
    }
    Ok(bytes)
}

fn memory_limit(total: u64) -> Result<()> {
    if total > MAX_FILE {
        return Err(problem("Setup exceeds the aggregate 128 MiB budget"));
    }
    Ok(())
}

fn desired_mode(payload: &Payload) -> u32 {
    if payload.executable { 0o700 } else { 0o600 }
}

fn validate_plan(home: &Path, plan: &Plan) -> Result<()> {
    validate_id(&plan.id)?;

    if plan.version != 1 || plan.home != home || plan.uid != nix::unistd::getuid().as_raw() {
        return Err(problem(
            "Frozen plan does not belong to this helper home and UID",
        ));
    }

    validate_files(plan.files.iter().map(|file| &file.payload))?;

    for file in &plan.files {
        if operation(&file.payload, file.before.as_ref(), file.previous.as_ref()) != file.operation
            || file.operation == Operation::Conflict
        {
            return Err(problem(
                "Frozen plan contains a conflict or inconsistent operation",
            ));
        }
    }

    Ok(())
}

pub(super) fn apply_plan(
    home: &Path,
    plan: &Plan,
    guard: &impl Fn() -> Result<()>,
) -> Result<Value> {
    validate_plan(home, plan)?;
    guard()?;

    {
        let store = Store::open(home, Lock::Shared)?;
        store.require_available(&plan.id)?;

        if store.transaction(&plan.id)?.is_none() {
            preconditions(home, plan, &store.ledger()?)?;
        }
    }

    guard()?;
    let store = Store::open(home, Lock::Exclusive)?;
    store.require_available(&plan.id)?;
    let ledger = store.ledger()?;

    let transaction = match store.transaction(&plan.id)? {
        Some(transaction) => {
            if transaction.plan != *plan {
                return Err(problem("Transaction does not match this frozen plan"));
            }
            transaction
        }
        None => prepare(home, plan, &store, &ledger, guard)?,
    };

    match transaction.phase {
        Phase::Applying | Phase::Applied => {}
        Phase::RollingBack | Phase::RolledBack => {
            return Err(problem(
                "This transaction was rolled back or is being rolled back",
            ));
        }
    }

    let mut changing = Changing {
        home,
        store: &store,
        guard,
        transaction,
        ledger,
    };
    let result = changing.apply();

    // A file that changed in the helper some other way can never be copied
    // by this plan: only undoing it finishes.
    let stuck = result.is_err()
        && (0..plan.files.len())
            .any(|index| matches!(changing.install_state(index), Ok(Installation::Changed)));

    let mut value = report(&changing.transaction, result, false);
    if stuck {
        value["rollbackRequired"] = json!(true);
    }

    Ok(value)
}

/// Refuse a target that differs only by case from a file Atlas copied
/// earlier, or lies inside or around one: the ledger could not hold both.
fn separate_from_ledger(ledger: &Ledger, target: &Path) -> Result<()> {
    let Some(other) = ledger
        .files
        .keys()
        .find(|other| other.as_path() != target && overlap(other, target))
    else {
        return Ok(());
    };

    Err(problem(format!(
        "{} overlaps {}, which Atlas copied earlier; roll that back or pick another target",
        target.display(),
        other.display(),
    )))
}

fn preconditions(home: &Path, plan: &Plan, ledger: &Ledger) -> Result<()> {
    for file in &plan.files {
        separate_from_ledger(ledger, &file.payload.target)?;
        installable_parents(home, &file.payload.target)?;

        if fingerprint(&destination(home, &file.payload.target)?)? != file.before
            || ledger.files.get(&file.payload.target) != file.previous.as_ref()
        {
            return Err(problem(
                "Destination or provisioning ownership changed since preview; create a new plan",
            ));
        }
    }

    Ok(())
}

fn prepare(
    home: &Path,
    plan: &Plan,
    store: &Store,
    ledger: &Ledger,
    guard: &impl Fn() -> Result<()>,
) -> Result<Transaction> {
    preconditions(home, plan, ledger)?;
    let mut contents = Vec::new();
    let mut total = 0;

    for file in &plan.files {
        let after = checked_bytes(
            &file.payload.source,
            &file.payload.sha256,
            "Frozen payload changed",
        )?;
        let before = snapshot(&destination(home, &file.payload.target)?)?;
        if before.as_ref().map(|(fp, _)| fp) != file.before.as_ref() {
            return Err(problem("Destination changed during preparation"));
        }
        total += after.len() as u64 + before.as_ref().map_or(0, |(_, bytes)| bytes.len() as u64);
        memory_limit(total)?;
        contents.push((before.map(|(_, bytes)| bytes), after));
    }

    guard()?;
    store.discard_unstarted();

    let transaction = Transaction {
        version: 1,
        plan: plan.clone(),
        phase: Phase::Applying,
        progress: vec![Progress::default(); plan.files.len()],
        directories: Vec::new(),
        retained_directories: Vec::new(),
    };

    let written = (|| -> Result<()> {
        private_directory(&store.folder(&plan.id))?;

        for (index, (before, after)) in contents.iter().enumerate() {
            guard()?;
            if let Some(before) = before {
                atomic_write(&store.blob(&plan.id, index, true), before, 0o600)?;
            }
            atomic_write(&store.blob(&plan.id, index, false), after, 0o600)?;
        }

        guard()?;
        store.save(&transaction)
    })();

    // Copies of a transaction that never started are never read.
    if written.is_err() {
        store.discard_unstarted();
    }

    written.map(|()| transaction)
}

/// Write `bytes` beside their destination with the group and attributes of
/// the file they replace. Contents that are new to the destination
/// (`new_contents`) do not take over its download mark.
fn prepare_inode(
    path: &Path,
    bytes: &[u8],
    mode: u32,
    previous: Option<&Fingerprint>,
    new_contents: bool,
) -> Result<Fingerprint> {
    if fingerprint(path)?.is_none() {
        atomic_install(path, bytes, mode, true)?;
    }

    let current = fingerprint(path)?.ok_or_else(|| problem("Prepared file disappeared"))?;
    if current.sha256 != digest(bytes) || current.mode != mode {
        return Err(problem("Prepared file contents or permissions changed"));
    }

    if let Some(previous) = previous {
        let mut attributes = previous.xattrs.clone();
        if new_contents {
            attributes.remove(QUARANTINE);
        }

        set_group(path, previous.gid)?;
        restore_attributes(path, &attributes)?;
    }

    fingerprint(path)?.ok_or_else(|| problem("Prepared file disappeared"))
}

pub(super) fn rollback(
    home: &Path,
    id: &str,
    apply: bool,
    guard: &impl Fn() -> Result<()>,
) -> Result<Value> {
    validate_id(id)?;
    guard()?;

    let store = Store::open(home, if apply { Lock::Exclusive } else { Lock::Shared })?;
    store.require_available(id)?;
    let ledger = store.ledger()?;
    let transaction = store.transaction(id)?.ok_or_else(|| problem(MISSING))?;
    validate_plan(home, &transaction.plan)?;

    if transaction.phase == Phase::RolledBack {
        if apply {
            guard()?;
            store.finish(id)?;
            store.discard_copies(&transaction);
        }
        return Ok(report(&transaction, Ok(()), true));
    }

    let mut changing = Changing {
        home,
        store: &store,
        guard,
        transaction,
        ledger,
    };
    let mut states = Vec::new();
    let mut result = changing.check_rollback(&mut states);
    if apply && result.is_ok() {
        result = changing.roll_back();
    }

    if !apply {
        store.confirm_read()?;
    }

    let transaction = &changing.transaction;
    let mut value = report(transaction, result, true);
    value["preview"] = json!(!apply);

    // A preview names only the files undoing would put back or remove, and
    // those it would leave as the helper changed them.
    if !apply && states.len() == transaction.plan.files.len() {
        let targets = |wanted: &dyn Fn(&PlannedFile, RollbackState) -> bool| {
            transaction
                .plan
                .files
                .iter()
                .zip(&states)
                .filter(|(file, state)| wanted(file, **state))
                .map(|(file, _)| &file.payload.target)
                .collect::<Vec<_>>()
        };

        value["pendingTargets"] = json!(targets(&|file, state| {
            state == RollbackState::Installed && file.operation.places_a_copy()
        }));
        value["keptTargets"] = json!(targets(&|_, state| state == RollbackState::Changed));
    }

    Ok(value)
}

fn previous_entry(file: &PlannedFile, restored: Option<&Fingerprint>) -> Option<Entry> {
    match (&file.previous, restored) {
        (Some(entry), Some(fingerprint)) => Some(Entry {
            generation: entry.generation.clone(),
            fingerprint: fingerprint.clone(),
        }),
        (None, _) | (Some(_), None) => file.previous.clone(),
    }
}

fn report(transaction: &Transaction, result: Result<()>, rollback: bool) -> Value {
    let (mut completed, mut pending, mut kept) = (Vec::new(), Vec::new(), Vec::new());
    for (file, progress) in transaction.plan.files.iter().zip(&transaction.progress) {
        let finished = if rollback {
            progress.restored
        } else {
            progress.applied
        };
        let targets = if !finished {
            &mut pending
        } else if rollback && progress.kept {
            &mut kept
        } else {
            &mut completed
        };
        targets.push(&file.payload.target);
    }
    let mut value = json!({
        "ok": result.is_ok(),
        "planId": transaction.plan.id,
        "applied": transaction.phase == Phase::Applied,
        "rolledBack": transaction.phase == Phase::RolledBack,
        "phase": transaction.phase,
        "resumeRequired": result.is_err(),
        "files": file_report(&transaction.plan),
        "retainedDirectories": transaction.retained_directories,
        "completedTargets": completed,
        "pendingTargets": pending,
    });

    if rollback {
        value["keptTargets"] = json!(kept);
    }

    if let Err(error) = result {
        value["error"] = json!(error.error);
    }

    value
}

#[cfg(test)]
#[path = "worker_tests.rs"]
mod tests;

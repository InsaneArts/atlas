//! Runs inside the selected login. Every mutation is guarded and journaled.
use super::{model::*, state::Store, *};

/// The helper's answer for a plan it never applied.
pub(super) const MISSING: &str =
    "Nothing was copied into the helper with this ID, so there is nothing to undo";

/// Internal worker; invoked through the verified helper, never through sudo.
pub fn worker(path: &Path, apply: bool) -> Result<Value> {
    let packet: Packet = decode(&read_limited(path, MAX_RECORD)?)?;
    if packet.version != 1 {
        return Err(problem("Unsupported environment packet version"));
    }

    let guard = || {
        let current = Identity::from_proof(&crate::native::session_info()?)?;
        if current != packet.expected {
            return Err(problem(
                "Expected helper session or display changed; no further mutation is allowed",
            ));
        }
        Ok(())
    };

    guard()?;
    let home = account_home(packet.expected.uid)?;
    let mutating = matches!(
        &packet.request,
        Request::Apply { .. } | Request::Rollback { apply: true, .. } | Request::Run { .. }
    );
    if apply != mutating {
        return Err(problem("Worker apply flag does not match its request"));
    }

    match packet.request {
        Request::Preview { id, files } => preview(&home, packet.expected.uid, &id, files, &guard),
        Request::Apply { plan } => apply_plan(&home, &plan, &guard),
        Request::Rollback { id, apply } => rollback(&home, &id, apply, &guard),
        Request::Run { argv } => runner::execute(&home, &argv, &guard),
    }
}

fn account_home(uid: u32) -> Result<PathBuf> {
    let account = crate::setup::account_with_uid(uid)
        .ok()
        .flatten()
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

    let store = Store::open(home, false)?;
    store.require_available(id)?;
    let ledger = store.ledger()?;
    validate_files(files.iter())?;

    let mut planned = Vec::new();
    let mut total = 0;

    for payload in files {
        guard()?;
        separate_from_ledger(&ledger, &payload.target)?;
        installable_parents(home, &payload.target)?;

        let bytes = payload_bytes(&payload)?;
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
        (Some(Entry::Managed { fingerprint, .. }), Some(current)) => fingerprint == current,
        (Some(Entry::Legacy(hash)), Some(current)) => hash == &current.sha256,
        (None, _) => true,
        _ => false,
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
        _ => Operation::Conflict,
    }
}

fn validate_files<'a>(files: impl ExactSizeIterator<Item = &'a Payload> + Clone) -> Result<()> {
    if files.len() == 0 || files.len() > MAX_SELECTIONS {
        return Err(problem("Select 1–32 files"));
    }

    unique_targets(files.clone().map(|file| file.target.as_path()))?;

    for file in files {
        if !file.source.is_absolute()
            || file.sha256.len() != 64
            || !file
                .sha256
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        {
            return Err(problem("Invalid frozen file selection"));
        }
    }

    Ok(())
}

fn payload_bytes(payload: &Payload) -> Result<Vec<u8>> {
    let bytes = read_limited(&payload.source, MAX_FILE)?;
    if digest(&bytes) != payload.sha256 {
        return Err(problem("Frozen payload changed"));
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
        let store = Store::open(home, false)?;
        store.require_available(&plan.id)?;

        if store.transaction(&plan.id)?.is_none() {
            preconditions(home, plan, &store.ledger()?)?;
        }
    }

    guard()?;
    let store = Store::open(home, true)?;
    store.require_available(&plan.id)?;
    let mut ledger = store.ledger()?;

    let mut transaction = match store.transaction(&plan.id)? {
        Some(transaction) => {
            if transaction.plan != *plan {
                return Err(problem("Transaction does not match this frozen plan"));
            }
            transaction
        }
        None => prepare(home, plan, &store, &ledger, guard)?,
    };

    if matches!(transaction.phase, Phase::RollingBack | Phase::RolledBack) {
        return Err(problem(
            "This transaction was rolled back or is being rolled back",
        ));
    }

    let result = (|| -> Result<()> {
        for index in 0..plan.files.len() {
            apply_state(home, &transaction, index, &ledger)?;
        }

        guard()?;
        store.activate(&plan.id)?;

        for index in 0..plan.files.len() {
            apply_file(home, &mut transaction, index, &store, &mut ledger, guard)?;
        }

        guard()?;
        transaction.phase = Phase::Applied;
        store.save(&transaction)?;
        store.finish(&plan.id)?;

        Ok(())
    })();

    // A file that changed in the helper some other way can never be copied
    // by this plan: only undoing it finishes.
    let stuck = result.is_err()
        && (0..plan.files.len())
            .any(|index| matches!(install_state(home, &transaction, index, &ledger), Ok(None)));

    let mut value = report(&transaction, result, false);
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
        let after = payload_bytes(&file.payload)?;
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

/// Whether this transaction installed the file, recognized by its owned
/// generation or its journaled inode: Some(true) installed, Some(false) not
/// yet, None when it changed some other way.
fn install_state(
    home: &Path,
    transaction: &Transaction,
    index: usize,
    ledger: &Ledger,
) -> Result<Option<bool>> {
    let file = &transaction.plan.files[index];
    let progress = &transaction.progress[index];
    let current = fingerprint(&destination(home, &file.payload.target)?)?;
    let entry = ledger.files.get(&file.payload.target);
    if let (
        Some(Entry::Managed {
            generation,
            fingerprint,
        }),
        Some(current),
    ) = (entry, &current)
        && generation == &transaction.plan.id
        && fingerprint == current
        && current.sha256 == file.payload.sha256
        && current.mode == desired_mode(&file.payload)
    {
        return Ok(Some(true));
    }

    if entry == file.previous.as_ref() {
        if current == file.before && !progress.applied {
            return Ok(Some(false));
        }
        if progress.prepared.is_some() && current == progress.prepared {
            return Ok(Some(true));
        }
    }

    Ok(None)
}

/// Whether this transaction installed the file. One that changed some other
/// way is refused, saying how to finish when the plan never put it in place.
fn apply_state(
    home: &Path,
    transaction: &Transaction,
    index: usize,
    ledger: &Ledger,
) -> Result<bool> {
    if let Some(installed) = install_state(home, transaction, index, ledger)? {
        return Ok(installed);
    }

    let file = &transaction.plan.files[index];
    let when = if never_installed(home, transaction, index, ledger)? {
        "before"
    } else if undoable(transaction, index, ledger) {
        "after"
    } else {
        return Err(problem(
            "Destination or ownership changed during this transaction; it was preserved",
        ));
    };

    Err(problem(format!(
        "{} changed in the helper {when} this plan copied it; it was kept. \
         To finish, run atlas environment rollback {} --apply",
        file.payload.target.display(),
        transaction.plan.id,
    )))
}

/// Whether the ledger names this transaction's own copy of the file.
fn recorded(transaction: &Transaction, index: usize, ledger: &Ledger) -> bool {
    matches!(
        ledger.files.get(&transaction.plan.files[index].payload.target),
        Some(Entry::Managed { generation, .. }) if generation == &transaction.plan.id
    )
}

/// Whether undoing this transaction may still decide the file: the ledger
/// names this transaction's copy, or still the entry it started from. One
/// another plan copied since is that plan's to undo first.
fn undoable(transaction: &Transaction, index: usize, ledger: &Ledger) -> bool {
    let file = &transaction.plan.files[index];

    recorded(transaction, index, ledger)
        || ledger.files.get(&file.payload.target) == file.previous.as_ref()
}

/// Whether this transaction provably never put its file in place: it was
/// never acknowledged, the ledger does not name its copy, and either
/// nothing was prepared or the prepared copy still waits beside it. A plan
/// stopped before it was activated may find another plan copied the same
/// file since; that file is the other plan's, and this one never touched it.
fn never_installed(
    home: &Path,
    transaction: &Transaction,
    index: usize,
    ledger: &Ledger,
) -> Result<bool> {
    let file = &transaction.plan.files[index];
    let progress = &transaction.progress[index];
    if progress.applied || recorded(transaction, index, ledger) {
        return Ok(false);
    }

    let Some(prepared) = &progress.prepared else {
        return Ok(true);
    };

    let path = destination(home, &file.payload.target)?;
    let waiting = candidate(&path, &transaction.plan.id, index, false);

    Ok(fingerprint(&path)?.as_ref() != Some(prepared)
        && fingerprint(&waiting)?.as_ref() == Some(prepared))
}

fn apply_file(
    home: &Path,
    transaction: &mut Transaction,
    index: usize,
    store: &Store,
    ledger: &mut Ledger,
    guard: &impl Fn() -> Result<()>,
) -> Result<()> {
    guard()?;

    let file = transaction.plan.files[index].clone();
    let path = destination(home, &file.payload.target)?;
    let installed = apply_state(home, transaction, index, ledger)?;
    if !installed && file.operation != Operation::Unchanged {
        ensure_parents(home, transaction, &file.payload.target, store, guard)?;
        let candidate = candidate(&path, &transaction.plan.id, index, false);

        if transaction.progress[index].prepared.is_none() {
            guard()?;
            let bytes = read_limited(&store.blob(&transaction.plan.id, index, false), MAX_FILE)?;
            if digest(&bytes) != file.payload.sha256 {
                return Err(problem("Transaction payload changed"));
            }

            let prepared = prepare_inode(
                &candidate,
                &bytes,
                desired_mode(&file.payload),
                file.before.as_ref(),
                file.operation == Operation::Update,
            )?;
            transaction.progress[index].prepared = Some(prepared);
            store.save(transaction)?;
        }

        guard()?;
        if apply_state(home, transaction, index, ledger)? {
            return finish_file(home, transaction, index, store, ledger);
        }

        if fingerprint(&candidate)? != transaction.progress[index].prepared {
            return Err(problem("Prepared replacement changed or disappeared"));
        }

        install_prepared(&candidate, &path, file.before.is_none())?;

        if fingerprint(&path)? != transaction.progress[index].prepared {
            return Err(problem("Installed file did not match its prepared inode"));
        }
    }

    guard()?;
    finish_file(home, transaction, index, store, ledger)
}

fn finish_file(
    home: &Path,
    transaction: &mut Transaction,
    index: usize,
    store: &Store,
    ledger: &mut Ledger,
) -> Result<()> {
    let file = &transaction.plan.files[index];
    let current = fingerprint(&destination(home, &file.payload.target)?)?
        .ok_or_else(|| problem("Installed file disappeared"))?;

    let expected = if file.operation == Operation::Unchanged {
        &file.before
    } else {
        &transaction.progress[index].prepared
    };

    if expected
        .as_ref()
        .is_some_and(|expected| !current.same_contents_and_metadata(expected))
    {
        return Err(problem("Installed file metadata changed"));
    }

    ledger.version = 1;
    ledger.files.insert(
        file.payload.target.clone(),
        Entry::Managed {
            generation: transaction.plan.id.clone(),
            fingerprint: current,
        },
    );

    store.save_ledger(ledger)?;
    transaction.progress[index].applied = true;

    store.save(transaction)
}

fn candidate(path: &Path, id: &str, index: usize, rollback: bool) -> PathBuf {
    path.parent()
        .expect("a checked destination lies inside the helper home")
        .join(format!(
            ".atlas-{id}-{index}-{}",
            if rollback { "restore" } else { "install" }
        ))
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

fn ensure_parents(
    home: &Path,
    transaction: &mut Transaction,
    target: &Path,
    store: &Store,
    guard: &impl Fn() -> Result<()>,
) -> Result<()> {
    let mut relative = PathBuf::new();
    let parent = target
        .parent()
        .ok_or_else(|| problem("Missing target parent"))?;

    for component in parent.components() {
        relative.push(component);
        let path = directory_destination(home, &relative)?;
        if let Some(dir) = directory(&path, false)? {
            installation_parent(&dir)?;
            continue;
        }

        guard()?;
        let index = transaction
            .directories
            .iter()
            .position(|dir| dir.target == relative)
            .unwrap_or_else(|| {
                transaction.directories.push(Directory {
                    target: relative.clone(),
                    node: None,
                });
                transaction.directories.len() - 1
            });

        store.save(transaction)?;
        guard()?;
        let dir = create_private_directory(&path)?;
        transaction.directories[index].node = Some(node(&dir.metadata()?));
        store.save(transaction)?;
    }

    Ok(())
}

pub(super) fn rollback(
    home: &Path,
    id: &str,
    apply: bool,
    guard: &impl Fn() -> Result<()>,
) -> Result<Value> {
    validate_id(id)?;
    guard()?;

    let store = Store::open(home, apply)?;
    store.require_available(id)?;
    let mut ledger = store.ledger()?;
    let mut transaction = store.transaction(id)?.ok_or_else(|| problem(MISSING))?;
    validate_plan(home, &transaction.plan)?;

    if transaction.phase == Phase::RolledBack {
        if apply {
            store.discard_copies(&transaction);
        }
        return Ok(report(&transaction, Ok(()), true));
    }

    let mut states = Vec::new();
    let result = (|| -> Result<()> {
        for index in 0..transaction.plan.files.len() {
            let status = rollback_state(home, &transaction, index, &ledger)?;
            states.push(status);
            let file = &transaction.plan.files[index];
            if status == RollbackState::Installed
                && file.operation != Operation::Unchanged
                && let Some(before) = &file.before
            {
                if let Some(restore) = &transaction.progress[index].restore {
                    let path = destination(home, &file.payload.target)?;
                    if fingerprint(&candidate(&path, id, index, true))?.as_ref() != Some(restore) {
                        return Err(problem("A prepared restore changed or disappeared"));
                    }
                } else {
                    let backup = read_limited(&store.blob(id, index, true), MAX_FILE)?;
                    if digest(&backup) != before.sha256 {
                        return Err(problem(
                            "A transaction backup changed; rollback did not start",
                        ));
                    }
                }
            }
        }

        if !apply {
            return Ok(());
        }

        guard()?;
        store.activate(id)?;
        transaction.phase = Phase::RollingBack;
        store.save(&transaction)?;

        for index in (0..transaction.plan.files.len()).rev() {
            rollback_file(home, &mut transaction, index, &store, &mut ledger, guard)?;
        }

        transaction.retained_directories = cleanup_directories(home, &transaction, guard)?;
        guard()?;
        transaction.phase = Phase::RolledBack;
        store.save(&transaction)?;
        store.finish(id)?;

        // Nothing reads a rolled-back transaction's copies again.
        store.discard_copies(&transaction);

        Ok(())
    })();

    let mut value = report(&transaction, result, true);
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
            state == RollbackState::Installed && file.operation != Operation::Unchanged
        }));
        value["keptTargets"] = json!(targets(&|_, state| state == RollbackState::Changed));
    }

    Ok(value)
}

#[derive(Clone, Copy, PartialEq)]
enum RollbackState {
    /// Undone already: put back, left untouched or kept.
    Done,
    /// Never put in place by this transaction.
    Untouched,
    /// As it was before this transaction.
    Before,
    /// This transaction's copy, unchanged.
    Installed,
    /// The earlier file is back, not yet recorded.
    Restored,
    /// Changed in the helper since this transaction copied it.
    Changed,
}

fn previous_entry(file: &PlannedFile, restored: Option<&Fingerprint>) -> Option<Entry> {
    match (&file.previous, restored) {
        (Some(Entry::Managed { generation, .. }), Some(fingerprint)) => Some(Entry::Managed {
            generation: generation.clone(),
            fingerprint: fingerprint.clone(),
        }),
        _ => file.previous.clone(),
    }
}

fn rollback_state(
    home: &Path,
    transaction: &Transaction,
    index: usize,
    ledger: &Ledger,
) -> Result<RollbackState> {
    let file = &transaction.plan.files[index];
    let progress = &transaction.progress[index];

    // What happens to the file once it is undone is the helper's business.
    if progress.restored {
        return Ok(RollbackState::Done);
    }

    // A file this transaction never put in place is the helper's, whatever
    // it holds now; rollback leaves it alone.
    if never_installed(home, transaction, index, ledger)? {
        return Ok(RollbackState::Untouched);
    }

    let current = fingerprint(&destination(home, &file.payload.target)?)?;
    let entry = ledger.files.get(&file.payload.target);
    let ours = recorded(transaction, index, ledger);

    let restored = if file.operation == Operation::Unchanged {
        file.before.as_ref()
    } else {
        progress.restore.as_ref()
    };

    let previous = previous_entry(file, restored);
    if progress.reverting && current.as_ref() == restored && (entry == previous.as_ref() || ours) {
        return Ok(RollbackState::Restored);
    }

    if current == file.before && (entry == file.previous.as_ref() || ours) {
        return Ok(RollbackState::Before);
    }

    if install_state(home, transaction, index, ledger)? == Some(true) {
        return Ok(RollbackState::Installed);
    }

    if undoable(transaction, index, ledger) {
        return Ok(RollbackState::Changed);
    }

    Err(problem(format!(
        "{} was copied again by a later plan; roll that back first",
        file.payload.target.display()
    )))
}

fn rollback_file(
    home: &Path,
    transaction: &mut Transaction,
    index: usize,
    store: &Store,
    ledger: &mut Ledger,
    guard: &impl Fn() -> Result<()>,
) -> Result<()> {
    guard()?;

    let file = transaction.plan.files[index].clone();
    let path = destination(home, &file.payload.target)?;
    let status = rollback_state(home, transaction, index, ledger)?;

    // The randomized candidate name is reserved in this durable transaction.
    // A crash can leave its inode prepared before the fingerprint was recorded.
    if transaction.progress[index].prepared.is_none() {
        let candidate = candidate(&path, &transaction.plan.id, index, false);
        if let Some(prepared) = fingerprint(&candidate)? {
            if prepared.sha256 != file.payload.sha256
                || prepared.mode != desired_mode(&file.payload)
            {
                return Err(problem(
                    "Unrecorded transaction candidate changed; it was preserved",
                ));
            }

            transaction.progress[index].prepared = Some(prepared);
            guard()?;
            store.save(transaction)?;
        }
    }

    match status {
        RollbackState::Done => return discard_candidates(&path, transaction, index, guard),
        RollbackState::Untouched => {
            return release_untouched(&path, transaction, index, store, guard);
        }
        RollbackState::Changed => {
            return keep_changed(&path, transaction, index, store, ledger, guard);
        }
        _ => {}
    }

    if status == RollbackState::Installed && file.operation != Operation::Unchanged {
        if let Some(before) = &file.before
            && transaction.progress[index].restore.is_none()
        {
            let bytes = read_limited(&store.blob(&transaction.plan.id, index, true), MAX_FILE)?;
            if digest(&bytes) != before.sha256 {
                return Err(problem("Transaction backup changed"));
            }

            let candidate = candidate(&path, &transaction.plan.id, index, true);
            guard()?;
            transaction.progress[index].restore = Some(prepare_inode(
                &candidate,
                &bytes,
                before.mode,
                Some(before),
                false,
            )?);
            store.save(transaction)?;
        }

        transaction.progress[index].reverting = true;
        guard()?;
        store.save(transaction)?;
        guard()?;

        if rollback_state(home, transaction, index, ledger)? != RollbackState::Installed {
            return Err(problem("Destination changed before rollback"));
        }

        if file.before.is_some() {
            let candidate = candidate(&path, &transaction.plan.id, index, true);
            if fingerprint(&candidate)? != transaction.progress[index].restore {
                return Err(problem("Prepared restore changed"));
            }
            install_prepared(&candidate, &path, false)?;
        } else {
            unlink(&path, false)?;
        }
    } else if status == RollbackState::Before || file.operation == Operation::Unchanged {
        transaction.progress[index].restore = file.before.clone();
        transaction.progress[index].reverting = true;
        store.save(transaction)?;
    }

    guard()?;
    let restored = fingerprint(&path)?;

    let expected = if file.operation == Operation::Unchanged {
        &file.before
    } else {
        &transaction.progress[index].restore
    };

    if &restored != expected {
        return Err(problem("Restored file does not match its recorded inode"));
    }

    let previous = previous_entry(&file, restored.as_ref());
    if let Some(previous) = previous {
        ledger.files.insert(file.payload.target.clone(), previous);
    } else {
        ledger.files.remove(&file.payload.target);
    }

    ledger.version = 1;
    store.save_ledger(ledger)?;
    transaction.progress[index].restored = true;
    store.save(transaction)?;

    discard_candidates(&path, transaction, index, guard)
}

/// Leave a file the helper changed since this transaction copied it as the
/// helper has it. The ledger goes back to the entry the transaction started
/// from, so the file is no longer this transaction's copy, and only the
/// transaction's own waiting copies go.
fn keep_changed(
    path: &Path,
    transaction: &mut Transaction,
    index: usize,
    store: &Store,
    ledger: &mut Ledger,
    guard: &impl Fn() -> Result<()>,
) -> Result<()> {
    let file = &transaction.plan.files[index];
    match &file.previous {
        Some(previous) => ledger
            .files
            .insert(file.payload.target.clone(), previous.clone()),
        None => ledger.files.remove(&file.payload.target),
    };

    guard()?;
    ledger.version = 1;
    store.save_ledger(ledger)?;

    transaction.progress[index].kept = true;
    transaction.progress[index].restored = true;
    store.save(transaction)?;

    discard_candidates(path, transaction, index, guard)
}

/// Remove this transaction's waiting copies of one file, and anything an
/// interrupted write of them left. A copy the transaction did not record
/// is kept for inspection.
fn discard_candidates(
    path: &Path,
    transaction: &Transaction,
    index: usize,
    guard: &impl Fn() -> Result<()>,
) -> Result<()> {
    for restore in [false, true] {
        let candidate = candidate(path, &transaction.plan.id, index, restore);
        discard_partial(&candidate)?;

        if let Some(current) = fingerprint(&candidate)? {
            let expected = if restore {
                &transaction.progress[index].restore
            } else {
                &transaction.progress[index].prepared
            };

            if expected.as_ref() != Some(&current) {
                return Err(problem(
                    "An unrecorded prepared file remains; resume or inspect before cleanup",
                ));
            }

            guard()?;
            unlink(&candidate, false)?;
        }
    }

    Ok(())
}

/// Finish a file this transaction never put in place: its destination and
/// ledger entry stay as they are and only its waiting copy goes. The copy is
/// forgotten before it is removed, so an interruption leaves an unrecorded
/// copy the next rollback records and removes again.
fn release_untouched(
    path: &Path,
    transaction: &mut Transaction,
    index: usize,
    store: &Store,
    guard: &impl Fn() -> Result<()>,
) -> Result<()> {
    let waiting = candidate(path, &transaction.plan.id, index, false);
    discard_partial(&waiting)?;

    let current = fingerprint(&waiting)?;
    if current.is_some() && current != transaction.progress[index].prepared {
        return Err(problem(
            "An unrecorded prepared file remains; resume or inspect before cleanup",
        ));
    }

    transaction.progress[index].prepared = None;
    transaction.progress[index].restored = true;
    guard()?;
    store.save(transaction)?;

    if current.is_some() {
        guard()?;
        unlink(&waiting, false)?;
    }

    Ok(())
}

fn cleanup_directories(
    home: &Path,
    transaction: &Transaction,
    guard: &impl Fn() -> Result<()>,
) -> Result<Vec<PathBuf>> {
    // Files are already restored. A directory that cannot be proven to be our
    // unchanged, empty creation is kept and reported; an inspection error must
    // not wedge the transaction in RollingBack forever.
    // None: already absent. Some(removable) otherwise.
    let inspect = |record: &Directory| -> Result<Option<(PathBuf, bool)>> {
        let path = directory_destination(home, &record.target)?;
        let Some(dir) = directory(&path, false)? else {
            return Ok(None);
        };
        let removable = validate_private(&dir, true).is_ok()
            && record.node.as_ref() == Some(&node(&dir.metadata()?))
            && fs::read_dir(&path)?.next().is_none();
        Ok(Some((path, removable)))
    };

    let mut retained = Vec::new();

    for record in transaction.directories.iter().rev() {
        let path = match inspect(record) {
            Ok(None) => continue,
            Ok(Some((path, true))) => path,
            Ok(Some((_, false))) | Err(_) => {
                retained.push(record.target.clone());
                continue;
            }
        };

        guard()?;
        if unlink(&path, true).is_err() {
            retained.push(record.target.clone());
        }
    }

    Ok(retained)
}

fn report(transaction: &Transaction, result: Result<()>, rollback: bool) -> Value {
    let (completed, pending): (Vec<_>, Vec<_>) = transaction
        .plan
        .files
        .iter()
        .zip(&transaction.progress)
        .partition(|(_, progress)| {
            if rollback {
                progress.restored
            } else {
                progress.applied
            }
        });
    let (kept, completed): (Vec<_>, Vec<_>) = completed
        .into_iter()
        .partition(|(_, progress)| rollback && progress.kept);

    fn targets<'a>(files: Vec<(&'a PlannedFile, &'a Progress)>) -> Vec<&'a PathBuf> {
        files
            .into_iter()
            .map(|(file, _)| &file.payload.target)
            .collect()
    }

    let (completed, pending, kept) = (targets(completed), targets(pending), targets(kept));
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

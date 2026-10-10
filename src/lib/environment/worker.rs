//! Runs inside the selected login. Every mutation is guarded and journaled.
use super::{model::*, state::Store, *};

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
    validate_files(&files)?;
    let mut planned = Vec::new();
    let mut total = 0;
    for payload in files {
        guard()?;
        let key = payload.target.to_string_lossy().to_ascii_lowercase();
        if ledger.files.keys().any(|target| {
            target != &payload.target && target.to_string_lossy().to_ascii_lowercase() == key
        }) {
            return Err(problem(
                "Selected target is a case alias of a previously managed target",
            ));
        }
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
            operation: operation.into(),
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
    let ok = plan.files.iter().all(|file| file.operation != "conflict");
    Ok(json!({
        "ok": ok,
        "applied": false,
        "planId": id,
        "transactionId": id,
        "files": file_report(&plan),
        "plan": plan,
    }))
}

fn operation(
    payload: &Payload,
    current: Option<&Fingerprint>,
    previous: Option<&Entry>,
) -> &'static str {
    let managed_matches = match (previous, current) {
        (Some(Entry::Managed { fingerprint, .. }), Some(current)) => fingerprint == current,
        (Some(Entry::Legacy(hash)), Some(current)) => hash == &current.sha256,
        (None, _) => true,
        _ => false,
    };
    if !managed_matches {
        return "conflict";
    }
    match current {
        None => "create",
        Some(current) if current.sha256 == payload.sha256 => {
            if current.mode == desired_mode(payload) {
                "unchanged"
            } else {
                "permissions"
            }
        }
        Some(_) if previous.is_some() => "update",
        _ => "conflict",
    }
}

fn validate_files(files: &[Payload]) -> Result<()> {
    if files.is_empty() || files.len() > 32 {
        return Err(problem("Select 1–32 files"));
    }
    unique_targets(files.iter().map(|file| file.target.as_path()))?;
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
    validate_files(
        &plan
            .files
            .iter()
            .map(|file| file.payload.clone())
            .collect::<Vec<_>>(),
    )?;
    for file in &plan.files {
        if operation(&file.payload, file.before.as_ref(), file.previous.as_ref()) != file.operation
            || file.operation == "conflict"
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
    report(&transaction, result, false)
}

fn preconditions(home: &Path, plan: &Plan, ledger: &Ledger) -> Result<()> {
    for file in &plan.files {
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
    private_directory(&store.folder(&plan.id))?;
    for (index, (before, after)) in contents.iter().enumerate() {
        guard()?;
        if let Some(before) = before {
            atomic_write(&store.blob(&plan.id, index, true), before, 0o600)?;
        }
        atomic_write(&store.blob(&plan.id, index, false), after, 0o600)?;
    }
    let transaction = Transaction {
        version: 1,
        plan: plan.clone(),
        phase: Phase::Applying,
        progress: vec![Progress::default(); plan.files.len()],
        directories: Vec::new(),
        retained_directories: Vec::new(),
    };
    guard()?;
    store.save(&transaction)?;
    Ok(transaction)
}

/// An installed file is recognized by its owned generation or its journaled inode.
fn apply_state(
    home: &Path,
    transaction: &Transaction,
    index: usize,
    ledger: &Ledger,
) -> Result<bool> {
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
        return Ok(true);
    }
    if entry == file.previous.as_ref() {
        if current == file.before && !progress.applied {
            return Ok(false);
        }
        if progress.prepared.is_some() && current == progress.prepared {
            return Ok(true);
        }
    }
    Err(problem(
        "Destination or ownership changed during this transaction; it was preserved",
    ))
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
    if !installed && file.operation != "unchanged" {
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
    let expected = if file.operation == "unchanged" {
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
    path.parent().unwrap().join(format!(
        ".understudy-{id}-{index}-{}",
        if rollback { "restore" } else { "install" }
    ))
}

fn prepare_inode(
    path: &Path,
    bytes: &[u8],
    mode: u32,
    previous: Option<&Fingerprint>,
) -> Result<Fingerprint> {
    if fingerprint(path)?.is_none() {
        atomic_install(path, bytes, mode, true)?;
    }
    let current = fingerprint(path)?.ok_or_else(|| problem("Prepared file disappeared"))?;
    if current.sha256 != digest(bytes) || current.mode != mode {
        return Err(problem("Prepared file contents or permissions changed"));
    }
    if let Some(previous) = previous {
        set_group(path, previous.gid)?;
        restore_attributes(path, &previous.xattrs)?;
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
    for component in target.parent().unwrap().components() {
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
    let mut transaction = store.transaction(id)?.ok_or_else(|| {
        problem("Transaction is missing; legacy ledgers do not contain rollback history")
    })?;
    validate_plan(home, &transaction.plan)?;
    if transaction.phase == Phase::RolledBack {
        return report(&transaction, Ok(()), true);
    }
    let result = (|| -> Result<()> {
        for index in 0..transaction.plan.files.len() {
            let status = rollback_state(home, &transaction, index, &ledger)?;
            let file = &transaction.plan.files[index];
            if status == RollbackState::Installed
                && file.operation != "unchanged"
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
        Ok(())
    })();
    let mut value = report(&transaction, result, true)?;
    value["preview"] = json!(!apply);
    Ok(value)
}

#[derive(PartialEq)]
enum RollbackState {
    Before,
    Installed,
    Restored,
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
    let current = fingerprint(&destination(home, &file.payload.target)?)?;
    let entry = ledger.files.get(&file.payload.target);
    let restored = if file.operation == "unchanged" {
        file.before.as_ref()
    } else {
        progress.restore.as_ref()
    };
    let previous = previous_entry(file, restored);
    if progress.reverting
        && current.as_ref() == restored
        && (entry == previous.as_ref()
            || matches!(entry, Some(Entry::Managed { generation, .. }) if generation == &transaction.plan.id))
    {
        return Ok(RollbackState::Restored);
    }
    if progress.restored {
        return Err(problem(
            "A rolled-back destination changed; it was preserved",
        ));
    }
    if current == file.before && entry == file.previous.as_ref() {
        return Ok(RollbackState::Before);
    }
    if apply_state(home, transaction, index, ledger)? {
        return Ok(RollbackState::Installed);
    }
    Err(problem(
        "Rollback requires this transaction's latest unchanged file generation",
    ))
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
    if status == RollbackState::Installed && file.operation != "unchanged" {
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
    } else if status == RollbackState::Before || file.operation == "unchanged" {
        transaction.progress[index].restore = file.before.clone();
        transaction.progress[index].reverting = true;
        store.save(transaction)?;
    }
    guard()?;
    let restored = fingerprint(&path)?;
    let expected = if file.operation == "unchanged" {
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
    for restore in [false, true] {
        let candidate = candidate(&path, &transaction.plan.id, index, restore);
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

fn report(transaction: &Transaction, result: Result<()>, rollback: bool) -> Result<Value> {
    let completed = transaction
        .plan
        .files
        .iter()
        .zip(&transaction.progress)
        .filter(|(_, progress)| {
            if rollback {
                progress.restored
            } else {
                progress.applied
            }
        })
        .map(|(file, _)| &file.payload.target)
        .collect::<Vec<_>>();
    let pending = transaction
        .plan
        .files
        .iter()
        .zip(&transaction.progress)
        .filter(|(_, progress)| {
            if rollback {
                !progress.restored
            } else {
                !progress.applied
            }
        })
        .map(|(file, _)| &file.payload.target)
        .collect::<Vec<_>>();
    let mut value = json!({
        "ok": result.is_ok(),
        "planId": transaction.plan.id,
        "transactionId": transaction.plan.id,
        "applied": transaction.phase == Phase::Applied,
        "rolledBack": transaction.phase == Phase::RolledBack,
        "phase": transaction.phase,
        "resumeRequired": result.is_err(),
        "files": file_report(&transaction.plan),
        "retainedDirectories": transaction.retained_directories,
        "completedTargets": completed,
        "pendingTargets": pending,
    });
    if let Err(error) = result {
        value["error"] = json!(error.error);
    }
    Ok(value)
}

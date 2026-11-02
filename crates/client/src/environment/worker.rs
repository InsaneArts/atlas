//! Runs inside the selected login. Every mutation is guarded and journaled.
use super::{
    model::*,
    reply::{TransactionSummary, Transactions},
    state::Store,
    *,
};
use atlas_core::files::Lock;
use atlas_core::lenient::Lenient;

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
        let read = (recorded_transactions(&store.path), store.active()?);
        store.confirm_read()?;
        read
    };
    recorded.sort_by_key(|(_, updated)| std::cmp::Reverse(*updated));

    let asked = |id: &String| ids.contains(id);
    let (mut listed, mut hidden, mut unreadable, mut more) = (Vec::new(), 0, 0, 0);
    for (id, updated) in &recorded {
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
            Ok(Some(transaction)) => listed.push(Lenient::Known(TransactionSummary {
                plan_id: id.clone(),
                phase: transaction.phase.into(),
                active: active.as_deref() == Some(id.as_str()),
                updated_at: *updated,
                files: file_report(&transaction.plan),
                retained_directories: transaction.retained_directories.len(),
            })),
            Ok(None) | Err(_) => unreadable += 1,
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

/// The transactions whose record is in the store at `path`, by ID, with
/// when each record last changed, in milliseconds since 1970. A folder
/// without a record holds a preparation that never started, and is left
/// out.
fn recorded_transactions(path: &Path) -> Vec<(String, u64)> {
    let Ok(entries) = fs::read_dir(path) else {
        return Vec::new();
    };

    entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_str()?.to_owned();
            let id = name.strip_prefix("transaction-")?;
            validate_id(id).ok()?;
            if !fs::symlink_metadata(entry.path()).ok()?.is_dir() {
                return None;
            }
            let at = changed_ms(&entry.path().join("transaction.json"))?;
            Some((id.to_owned(), at))
        })
        .collect()
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

/// Whether this transaction installed the file, recognized by its owned
/// generation or its journaled inode; a different file remains changed.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Installation {
    Present,
    Waiting,
    Changed,
}

/// Where a file stands against the transaction undoing it.
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

/// One transaction being applied or undone in the helper's `home`, journaled
/// in `store` under `guard`, with the ledger as that store holds it.
struct Changing<'a, G> {
    home: &'a Path,
    store: &'a Store,
    guard: &'a G,
    transaction: Transaction,
    ledger: Ledger,
}

impl<G: Fn() -> Result<()>> Changing<'_, G> {
    fn file(&self, index: usize) -> &PlannedFile {
        &self.transaction.plan.files[index]
    }

    fn path(&self, index: usize) -> Result<PathBuf> {
        destination(self.home, &self.file(index).payload.target)
    }

    fn entry(&self, index: usize) -> Option<&Entry> {
        self.ledger.files.get(&self.file(index).payload.target)
    }

    /// The copy the ledger names as this transaction's own, when it does.
    fn own_copy(&self, index: usize) -> Option<&Fingerprint> {
        self.entry(index)
            .filter(|entry| entry.generation == self.transaction.plan.id)
            .map(|entry| &entry.fingerprint)
    }

    /// Where a file is prepared beside `path` before it is moved over it. A
    /// checked destination lies inside the helper home, so it has a folder.
    fn candidate(&self, path: &Path, index: usize, rollback: bool) -> Result<PathBuf> {
        let folder = path
            .parent()
            .ok_or_else(|| problem("A destination needs a folder"))?;
        Ok(folder.join(format!(
            ".atlas-{}-{index}-{}",
            self.transaction.plan.id,
            if rollback { "restore" } else { "install" }
        )))
    }

    fn install_state(&self, index: usize) -> Result<Installation> {
        let file = self.file(index);
        let progress = &self.transaction.progress[index];
        let current = fingerprint(&self.path(index)?)?;
        if let (Some(recorded), Some(current)) = (self.own_copy(index), &current)
            && recorded == current
            && current.sha256 == file.payload.sha256
            && current.mode == desired_mode(&file.payload)
        {
            return Ok(Installation::Present);
        }

        if self.entry(index) == file.previous.as_ref() {
            if current == file.before && !progress.applied {
                return Ok(Installation::Waiting);
            }
            if progress.prepared.is_some() && current == progress.prepared {
                return Ok(Installation::Present);
            }
        }

        Ok(Installation::Changed)
    }

    /// Whether this transaction installed the file. One that changed some other
    /// way is refused, saying how to finish when the plan never put it in place.
    fn apply_state(&self, index: usize) -> Result<bool> {
        match self.install_state(index)? {
            Installation::Present => return Ok(true),
            Installation::Waiting => return Ok(false),
            Installation::Changed => {}
        }

        let when = if self.never_installed(index)? {
            "before"
        } else if self.undoable(index) {
            "after"
        } else {
            return Err(problem(
                "Destination or ownership changed during this transaction; it was preserved",
            ));
        };

        Err(problem(format!(
            "{} changed in the helper {when} this plan copied it; it was kept. \
             To finish, run atlas environment rollback {} --apply",
            self.file(index).payload.target.display(),
            self.transaction.plan.id,
        )))
    }

    /// Whether undoing this transaction may still decide the file: the ledger
    /// names this transaction's copy, or still the entry it started from. One
    /// another plan copied since is that plan's to undo first.
    fn undoable(&self, index: usize) -> bool {
        self.own_copy(index).is_some() || self.entry(index) == self.file(index).previous.as_ref()
    }

    /// Whether this transaction provably never put its file in place: it was
    /// never acknowledged, the ledger does not name its copy, and either
    /// nothing was prepared or the prepared copy still waits beside it. A plan
    /// stopped before it was activated may find another plan copied the same
    /// file since; that file is the other plan's, and this one never touched it.
    fn never_installed(&self, index: usize) -> Result<bool> {
        let progress = &self.transaction.progress[index];
        if progress.applied || self.own_copy(index).is_some() {
            return Ok(false);
        }

        let Some(prepared) = &progress.prepared else {
            return Ok(true);
        };

        let path = self.path(index)?;
        let waiting = self.candidate(&path, index, false)?;

        Ok(fingerprint(&path)?.as_ref() != Some(prepared)
            && fingerprint(&waiting)?.as_ref() == Some(prepared))
    }

    fn apply(&mut self) -> Result<()> {
        let files = self.transaction.plan.files.len();
        for index in 0..files {
            self.apply_state(index)?;
        }

        (self.guard)()?;
        self.store.activate(&self.transaction.plan.id)?;

        for index in 0..files {
            self.apply_file(index)?;
        }

        (self.guard)()?;
        self.transaction.phase = Phase::Applied;
        self.store.save(&self.transaction)?;
        self.store.finish(&self.transaction.plan.id)
    }

    fn apply_file(&mut self, index: usize) -> Result<()> {
        (self.guard)()?;

        let file = self.file(index).clone();
        let path = self.path(index)?;
        let installed = self.apply_state(index)?;
        if !installed && file.operation.places_a_copy() {
            self.ensure_parents(&file.payload.target)?;
            let candidate = self.candidate(&path, index, false)?;

            if self.transaction.progress[index].prepared.is_none() {
                (self.guard)()?;
                let bytes = checked_bytes(
                    &self.store.blob(&self.transaction.plan.id, index, false),
                    &file.payload.sha256,
                    "Transaction payload changed",
                )?;

                let prepared = prepare_inode(
                    &candidate,
                    &bytes,
                    desired_mode(&file.payload),
                    file.before.as_ref(),
                    file.operation.new_contents(),
                )?;
                self.transaction.progress[index].prepared = Some(prepared);
                self.store.save(&self.transaction)?;
            }

            (self.guard)()?;
            if self.apply_state(index)? {
                return self.finish_file(index);
            }

            if fingerprint(&candidate)? != self.transaction.progress[index].prepared {
                return Err(problem("Prepared replacement changed or disappeared"));
            }

            install_prepared(&candidate, &path, file.before.is_none())?;

            if fingerprint(&path)? != self.transaction.progress[index].prepared {
                return Err(problem("Installed file did not match its prepared inode"));
            }
        }

        (self.guard)()?;
        self.finish_file(index)
    }

    fn finish_file(&mut self, index: usize) -> Result<()> {
        let current = fingerprint(&self.path(index)?)?
            .ok_or_else(|| problem("Installed file disappeared"))?;
        let file = &self.transaction.plan.files[index];

        let expected = if !file.operation.places_a_copy() {
            &file.before
        } else {
            &self.transaction.progress[index].prepared
        };

        if expected
            .as_ref()
            .is_some_and(|expected| !current.same_contents_and_metadata(expected))
        {
            return Err(problem("Installed file metadata changed"));
        }

        self.ledger.files.insert(
            file.payload.target.clone(),
            Entry {
                generation: self.transaction.plan.id.clone(),
                fingerprint: current,
            },
        );

        self.store.save_ledger(&self.ledger)?;
        self.transaction.progress[index].applied = true;

        self.store.save(&self.transaction)
    }

    fn ensure_parents(&mut self, target: &Path) -> Result<()> {
        let mut relative = PathBuf::new();
        let parent = target
            .parent()
            .ok_or_else(|| problem("Missing target parent"))?;

        for component in parent.components() {
            relative.push(component);
            let path = directory_destination(self.home, &relative)?;
            if let Some(dir) = directory(&path, false)? {
                installation_parent(&dir)?;
                continue;
            }

            (self.guard)()?;
            let index = self
                .transaction
                .directories
                .iter()
                .position(|dir| dir.target == relative)
                .unwrap_or_else(|| {
                    self.transaction.directories.push(Directory {
                        target: relative.clone(),
                        node: None,
                    });
                    self.transaction.directories.len() - 1
                });

            self.store.save(&self.transaction)?;
            (self.guard)()?;
            let dir = create_private_directory(&path)?;
            self.transaction.directories[index].node = Some(node(&dir.metadata()?));
            self.store.save(&self.transaction)?;
        }

        Ok(())
    }

    fn rollback_state(&self, index: usize) -> Result<RollbackState> {
        let file = self.file(index);
        let progress = &self.transaction.progress[index];

        // What happens to the file once it is undone is the helper's business.
        if progress.restored {
            return Ok(RollbackState::Done);
        }

        // A file this transaction never put in place is the helper's, whatever
        // it holds now; rollback leaves it alone.
        if self.never_installed(index)? {
            return Ok(RollbackState::Untouched);
        }

        let current = fingerprint(&self.path(index)?)?;
        let entry = self.entry(index);
        let ours = self.own_copy(index).is_some();

        // An unchanged file stays as it is when undone: the helper's original,
        // or the copy a later plan's undo put back in its place.
        let restored = if !file.operation.places_a_copy() {
            progress.restore.as_ref().or(file.before.as_ref())
        } else {
            progress.restore.as_ref()
        };

        let previous = previous_entry(file, restored);
        if progress.reverting
            && current.as_ref() == restored
            && (entry == previous.as_ref() || ours)
        {
            return Ok(RollbackState::Restored);
        }

        if current == file.before && (entry == file.previous.as_ref() || ours) {
            return Ok(RollbackState::Before);
        }

        if self.install_state(index)? == Installation::Present {
            return Ok(RollbackState::Installed);
        }

        if self.undoable(index) {
            return Ok(RollbackState::Changed);
        }

        Err(problem(format!(
            "{} was copied again by a later plan; roll that back first",
            file.payload.target.display()
        )))
    }

    /// Judge every file in `states` before anything changes, and refuse when
    /// what would put a file back is no longer as it was kept.
    fn check_rollback(&self, states: &mut Vec<RollbackState>) -> Result<()> {
        for index in 0..self.transaction.plan.files.len() {
            let status = self.rollback_state(index)?;
            states.push(status);
            let file = self.file(index);
            if status == RollbackState::Installed
                && file.operation.places_a_copy()
                && let Some(before) = &file.before
            {
                if let Some(restore) = &self.transaction.progress[index].restore {
                    let prepared = self.candidate(&self.path(index)?, index, true)?;
                    if fingerprint(&prepared)?.as_ref() != Some(restore) {
                        return Err(problem("A prepared restore changed or disappeared"));
                    }
                } else {
                    checked_bytes(
                        &self.store.blob(&self.transaction.plan.id, index, true),
                        &before.sha256,
                        "A transaction backup changed; rollback did not start",
                    )?;
                }
            }
        }

        Ok(())
    }

    fn roll_back(&mut self) -> Result<()> {
        (self.guard)()?;
        self.store.activate(&self.transaction.plan.id)?;
        self.transaction.phase = Phase::RollingBack;
        self.store.save(&self.transaction)?;

        for index in (0..self.transaction.plan.files.len()).rev() {
            self.rollback_file(index)?;
        }

        self.transaction.retained_directories = self.cleanup_directories()?;
        (self.guard)()?;
        self.transaction.phase = Phase::RolledBack;
        self.store.save(&self.transaction)?;
        self.store.finish(&self.transaction.plan.id)?;

        // Nothing reads a rolled-back transaction's copies again.
        self.store.discard_copies(&self.transaction);

        Ok(())
    }

    fn rollback_file(&mut self, index: usize) -> Result<()> {
        (self.guard)()?;

        let file = self.file(index).clone();
        let path = self.path(index)?;
        let status = self.rollback_state(index)?;
        let unchanged = !file.operation.places_a_copy();
        self.record_candidate(&path, &file, index)?;

        // Nothing is put back when the file is already as it was, or this plan
        // only adopted it. What is recorded is the file the state was judged
        // on (an adopted file a later plan's undo put back is a new inode,
        // known from the ledger), never a fresh read: a helper edit made
        // since fails the check below and is kept on the next try.
        let judged = match status {
            RollbackState::Done => return self.discard_candidates(&path, index),
            RollbackState::Untouched => return self.release_untouched(&path, index),
            RollbackState::Changed => return self.keep_changed(&path, index),
            RollbackState::Installed if !unchanged => {
                self.restore_file(&path, &file, index)?;
                None
            }
            RollbackState::Restored if !unchanged => None,
            RollbackState::Before => Some(file.before.clone()),
            RollbackState::Installed => Some(
                self.own_copy(index)
                    .cloned()
                    .or_else(|| self.transaction.progress[index].prepared.clone()),
            ),
            RollbackState::Restored => Some(
                self.transaction.progress[index]
                    .restore
                    .clone()
                    .or_else(|| file.before.clone()),
            ),
        };
        if let Some(restore) = judged {
            self.transaction.progress[index].restore = restore;
            self.transaction.progress[index].reverting = true;
            self.store.save(&self.transaction)?;
        }

        (self.guard)()?;
        let restored = fingerprint(&path)?;

        if restored != self.transaction.progress[index].restore {
            return Err(problem("Restored file does not match its recorded inode"));
        }

        let previous = previous_entry(&file, restored.as_ref());
        self.ledger.restore(&file.payload.target, previous);

        self.store.save_ledger(&self.ledger)?;
        self.transaction.progress[index].restored = true;
        self.store.save(&self.transaction)?;

        self.discard_candidates(&path, index)
    }

    /// The randomized candidate name is reserved in this durable transaction.
    /// A crash can leave its inode prepared before the fingerprint was recorded.
    fn record_candidate(&mut self, path: &Path, file: &PlannedFile, index: usize) -> Result<()> {
        if self.transaction.progress[index].prepared.is_some() {
            return Ok(());
        }

        let Some(prepared) = fingerprint(&self.candidate(path, index, false)?)? else {
            return Ok(());
        };
        if prepared.sha256 != file.payload.sha256 || prepared.mode != desired_mode(&file.payload) {
            return Err(problem(
                "Unrecorded transaction candidate changed; it was preserved",
            ));
        }

        self.transaction.progress[index].prepared = Some(prepared);
        (self.guard)()?;
        self.store.save(&self.transaction)
    }

    /// Prepare and journal a restore before replacing or removing the installed inode.
    fn restore_file(&mut self, path: &Path, file: &PlannedFile, index: usize) -> Result<()> {
        if let Some(before) = &file.before
            && self.transaction.progress[index].restore.is_none()
        {
            let bytes = checked_bytes(
                &self.store.blob(&self.transaction.plan.id, index, true),
                &before.sha256,
                "Transaction backup changed",
            )?;

            let candidate = self.candidate(path, index, true)?;
            (self.guard)()?;
            self.transaction.progress[index].restore = Some(prepare_inode(
                &candidate,
                &bytes,
                before.mode,
                Some(before),
                false,
            )?);
            self.store.save(&self.transaction)?;
        }

        self.transaction.progress[index].reverting = true;
        (self.guard)()?;
        self.store.save(&self.transaction)?;
        (self.guard)()?;

        if self.rollback_state(index)? != RollbackState::Installed {
            return Err(problem("Destination changed before rollback"));
        }

        if file.before.is_some() {
            let candidate = self.candidate(path, index, true)?;
            if fingerprint(&candidate)? != self.transaction.progress[index].restore {
                return Err(problem("Prepared restore changed"));
            }
            install_prepared(&candidate, path, false)
        } else {
            unlink(path, false)
        }
    }

    /// Leave a file the helper changed since this transaction copied it as the
    /// helper has it. The ledger goes back to the entry the transaction started
    /// from, so the file is no longer this transaction's copy, and only the
    /// transaction's own waiting copies go.
    fn keep_changed(&mut self, path: &Path, index: usize) -> Result<()> {
        let file = &self.transaction.plan.files[index];
        self.ledger
            .restore(&file.payload.target, file.previous.clone());

        (self.guard)()?;
        self.store.save_ledger(&self.ledger)?;

        self.transaction.progress[index].complete_kept();
        self.store.save(&self.transaction)?;

        self.discard_candidates(path, index)
    }

    /// Remove this transaction's waiting copies of one file, and anything an
    /// interrupted write of them left. A restore copy written before it was
    /// recorded holds the backed-up file and goes too; any other copy the
    /// transaction did not record is kept for inspection.
    fn discard_candidates(&self, path: &Path, index: usize) -> Result<()> {
        for restore in [false, true] {
            let candidate = self.candidate(path, index, restore)?;
            discard_partial(&candidate)?;

            if let Some(current) = fingerprint(&candidate)? {
                let progress = &self.transaction.progress[index];
                let expected = if restore {
                    &progress.restore
                } else {
                    &progress.prepared
                };

                let unrecorded_restore = restore
                    && expected.is_none()
                    && self.file(index).before.as_ref().is_some_and(|backup| {
                        current.sha256 == backup.sha256 && current.mode == backup.mode
                    });

                if expected.as_ref() != Some(&current) && !unrecorded_restore {
                    return Err(problem(
                        "An unrecorded prepared file remains; resume or inspect before cleanup",
                    ));
                }

                (self.guard)()?;
                unlink(&candidate, false)?;
            }
        }

        Ok(())
    }

    /// Finish a file this transaction never put in place: its destination and
    /// ledger entry stay as they are and only its waiting copy goes. The copy is
    /// forgotten before it is removed, so an interruption leaves an unrecorded
    /// copy the next rollback records and removes again.
    fn release_untouched(&mut self, path: &Path, index: usize) -> Result<()> {
        let waiting = self.candidate(path, index, false)?;
        discard_partial(&waiting)?;

        let current = fingerprint(&waiting)?;
        if current.is_some() && current != self.transaction.progress[index].prepared {
            return Err(problem(
                "An unrecorded prepared file remains; resume or inspect before cleanup",
            ));
        }

        self.transaction.progress[index].release_untouched();
        (self.guard)()?;
        self.store.save(&self.transaction)?;

        if current.is_some() {
            (self.guard)()?;
            unlink(&waiting, false)?;
        }

        Ok(())
    }

    fn cleanup_directories(&self) -> Result<Vec<PathBuf>> {
        // Files are already restored. A directory that cannot be proven to be our
        // unchanged, empty creation is kept and reported; an inspection error must
        // not wedge the transaction in RollingBack forever.
        // None: already absent. Some(removable) otherwise.
        let inspect = |record: &Directory| -> Result<Option<(PathBuf, bool)>> {
            let path = directory_destination(self.home, &record.target)?;
            let Some(dir) = directory(&path, false)? else {
                return Ok(None);
            };
            let removable = validate_private(&dir, true).is_ok()
                && record.node.as_ref() == Some(&node(&dir.metadata()?))
                && fs::read_dir(&path)?.next().is_none();
            Ok(Some((path, removable)))
        };

        let mut retained = Vec::new();

        for record in self.transaction.directories.iter().rev() {
            let path = match inspect(record) {
                Ok(None) => continue,
                Ok(Some((path, true))) => path,
                Ok(Some((_, false))) | Err(_) => {
                    retained.push(record.target.clone());
                    continue;
                }
            };

            (self.guard)()?;
            if unlink(&path, true).is_err() {
                retained.push(record.target.clone());
            }
        }

        Ok(retained)
    }
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

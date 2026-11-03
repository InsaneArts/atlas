//! One transaction's files being applied or undone in the helper, each step
//! guarded and journaled.
use super::*;

/// Whether this transaction installed the file, recognized by its owned
/// generation or its journaled inode; a different file remains changed.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Installation {
    Present,
    Waiting,
    Changed,
}

/// Where a file stands against the transaction undoing it.
#[derive(Clone, Copy, PartialEq)]
pub(super) enum RollbackState {
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
pub(super) struct Changing<'a, G> {
    pub(super) home: &'a Path,
    pub(super) store: &'a Store,
    pub(super) guard: &'a G,
    pub(super) transaction: Transaction,
    pub(super) ledger: Ledger,
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

    pub(super) fn install_state(&self, index: usize) -> Result<Installation> {
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

    pub(super) fn apply(&mut self) -> Result<()> {
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
    pub(super) fn check_rollback(&self, states: &mut Vec<RollbackState>) -> Result<()> {
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

    pub(super) fn roll_back(&mut self) -> Result<()> {
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

//! Each update as one transaction: copied in, verified, swapped into place
//! whole, and then released by a daemon of the new build or rolled back,
//! every step kept before it is done and settled from its record after a
//! crash, by the identities found and never blind.
use super::{
    Layout, System,
    copy::{BUNDLE, copy_in},
    record::{Asked, Identity, Phase, Transaction},
};
use atlas_core::{
    error::{Code, Error, Result},
    files::Token,
    fs_move::{FileName, Moved, exchange_between},
    stage::Snapshot,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, PoisonError},
};

/// How long a placed bundle waits for a daemon of its build to release it
/// before it is rolled back.
pub(super) const DEADLINE_MS: u64 = 10 * 60 * 1000;

/// The previous bundle's name in its transaction's folder once swapped out,
/// and the new one's before it is swapped in.
const HELD: &str = "Atlas.app";
/// The installer's own next copy, frozen at verification.
const NEXT: &str = "Installer.app";

/// The installer: one mutation at a time, every transaction and request
/// kept under its layout.
pub(super) struct Installer {
    layout: Layout,
    system: Arc<dyn System>,
    mutating: Mutex<()>,
}

/// A reply refusing the request, for `why`.
fn refusal(why: &str) -> Value {
    json!({"ok": false, "error": why})
}

/// A request's reply, saying where its transaction is.
fn phase_reply(transaction: &Transaction) -> Value {
    json!({"ok": true, "transaction": transaction.id, "phase": transaction.phase, "why": transaction.why})
}

fn name(text: &str) -> FileName {
    FileName::new(text).expect("a constant plain name")
}

fn io(error: impl std::error::Error + Send + Sync + 'static) -> Error {
    Error::new(Code::Io, "An installer step failed.").with_source(error)
}

fn synced(moved: Moved) -> Result<()> {
    match moved {
        Moved::Synced => Ok(()),
        Moved::Unsynced(error) => Err(io(error)),
    }
}

/// How a step that may find something unexpected ended.
enum Step {
    Done,
    /// It found what it did not expect, and the transaction now pends.
    Pended,
}

impl Installer {
    /// The installer of `layout`, its folders made and private.
    pub(super) fn new(layout: Layout, system: Arc<dyn System>) -> Result<Self> {
        for folder in [layout.home(), layout.transactions(), layout.requests()] {
            std::fs::create_dir_all(&folder).map_err(io)?;
        }
        for private in [layout.transactions(), layout.requests()] {
            std::fs::set_permissions(
                &private,
                std::os::unix::fs::PermissionsExt::from_mode(0o700),
            )
            .map_err(io)?;
        }
        Ok(Self {
            layout,
            system,
            mutating: Mutex::new(()),
        })
    }

    pub(super) fn record(&self, id: &Token) -> PathBuf {
        self.layout.transactions().join(format!("{id}.json"))
    }

    pub(super) fn folder(&self, id: &Token) -> PathBuf {
        self.layout.transactions().join(id.as_str())
    }

    fn request_record(&self, request: &Token) -> PathBuf {
        self.layout.requests().join(format!("{request}.json"))
    }

    fn keep(&self, transaction: &Transaction) -> Result<()> {
        transaction.write(&self.record(&transaction.id))
    }

    /// Every transaction kept.
    fn all(&self) -> Result<Vec<Transaction>> {
        let mut found = Vec::new();
        for entry in std::fs::read_dir(self.layout.transactions()).map_err(io)? {
            let path = entry.map_err(io)?.path();
            if path
                .extension()
                .is_some_and(|extension| extension == "json")
            {
                found.extend(Transaction::read(&path)?);
            }
        }
        Ok(found)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, ()> {
        self.mutating.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Begin `request`, asking `op` with `args`: kept before anything is
    /// done. A request answered before is answered from its record; one
    /// kept but cut short before its answer goes on; one whose ID was used
    /// for anything else is refused. A refusal or an old answer is the reply.
    fn begin(
        &self,
        request: &Token,
        op: &str,
        args: &str,
        transaction: Option<&Token>,
    ) -> Result<std::result::Result<Asked, Value>> {
        if let Some(kept) = Asked::read(&self.request_record(request))? {
            if kept.op != op || kept.args != args {
                return Ok(Err(refusal(
                    "This request ID was used for another request.",
                )));
            }
            return Ok(match kept.reply {
                Some(_) => Err(self.reply_of(&kept)?),
                // Cut short before it was answered: what it began goes on.
                None => Ok(kept),
            });
        }
        let asked = Asked {
            request: request.clone(),
            op: op.into(),
            args: args.into(),
            transaction: transaction.cloned(),
            reply: None,
        };
        asked.write(&self.request_record(request))?;
        Ok(Ok(asked))
    }

    /// What a kept request came to: its reply, or where its transaction is.
    fn reply_of(&self, asked: &Asked) -> Result<Value> {
        if let Some(reply) = &asked.reply {
            return Ok(reply.clone());
        }
        let transaction = match &asked.transaction {
            Some(id) => Transaction::read(&self.record(id))?,
            None => None,
        };
        Ok(transaction.map_or_else(
            || json!({"ok": true, "pending": true}),
            |kept| phase_reply(&kept),
        ))
    }

    /// Answer `asked` with `reply`, kept with it.
    fn answer(&self, mut asked: Asked, reply: Value) -> Result<Value> {
        asked.reply = Some(reply.clone());
        asked.write(&self.request_record(&asked.request))?;
        Ok(reply)
    }

    /// Install the bundle `manifest` describes, staged at `source` by the
    /// machine job of account `uid`, as the transaction `transaction`.
    pub(super) fn install(
        &self,
        request: &Token,
        transaction: &Token,
        uid: u32,
        source: &Path,
        manifest: &Snapshot,
    ) -> Result<Value> {
        let _one = self.lock();
        // Bound to the whole manifest as sent, not only the digest it claims.
        let sent = Sha256::digest(serde_json::to_vec(manifest)?);
        let args = format!("{transaction} {} {} {uid}", source.display(), hex(&sent));
        let asked = match self.begin(request, "install", &args, Some(transaction))? {
            Ok(asked) => asked,
            Err(reply) => return Ok(reply),
        };
        if let Some(kept) = Transaction::read(&self.record(transaction))? {
            // This very request, cut short after its transaction began: where
            // that went is its answer, recovery having settled it.
            let reply = if asked.transaction.as_ref() == Some(transaction)
                && kept.manifest == manifest.digest
            {
                phase_reply(&kept)
            } else {
                refusal("This transaction was begun already.")
            };
            return self.answer(asked, reply);
        }
        if self.all()?.iter().any(|kept| !kept.phase.ended()) {
            return self.answer(asked, refusal("Another update is open."));
        }
        let applications = File::open(self.layout.applications()).map_err(io)?;
        let Some(installed) =
            Identity::of(&applications, &name(BUNDLE))?.filter(|seen| seen.folder)
        else {
            return self.answer(asked, refusal("No Atlas is installed."));
        };
        let mut kept = Transaction {
            id: transaction.clone(),
            uid,
            manifest: manifest.digest.clone(),
            phase: Phase::Copying,
            why: None,
            installed,
            new: None,
            cdhash: None,
            placed_at: None,
        };
        self.keep(&kept)?;
        let folder = self.folder(transaction);
        std::fs::create_dir(&folder).map_err(io)?;
        let into = File::open(&folder).map_err(io)?;
        let root = nix::unistd::geteuid().is_root();
        if let Err(error) = copy_in(manifest, source, &into, &name(HELD), root) {
            self.abort(&mut kept)?;
            return self.answer(asked, refusal(&error.error));
        }
        let staged = folder.join(HELD);
        let (Some(cdhash), Some(new)) = (
            self.system.verified(&staged),
            Identity::of(&into, &name(HELD))?,
        ) else {
            self.abort(&mut kept)?;
            return self.answer(asked, refusal("The new bundle is not Atlas's, signed."));
        };
        // The installer's own next copy, frozen now as this very code.
        if let Err(error) = copy_in(manifest, source, &into, &name(NEXT), root) {
            self.abort(&mut kept)?;
            return self.answer(asked, refusal(&error.error));
        }
        if self.system.verified(&folder.join(NEXT)).as_ref() != Some(&cdhash) {
            self.abort(&mut kept)?;
            return self.answer(asked, refusal("The new installer is not the new bundle's."));
        }
        kept.phase = Phase::Verified;
        kept.new = Some(new);
        kept.cdhash = Some(hex(&cdhash));
        self.keep(&kept)?;
        self.swap_in(&mut kept, &applications, &into)?;
        let reply = phase_reply(&kept);
        self.answer(asked, reply)
    }

    /// Swap the verified copy in for the installed bundle, then check both
    /// places: placed, swapped back, or pending.
    fn swap_in(&self, kept: &mut Transaction, applications: &File, into: &File) -> Result<()> {
        if Identity::of(applications, &name(BUNDLE))? != Some(kept.installed) {
            return self.pend(
                kept,
                "The installed Atlas changed while this update was being made.",
            );
        }
        kept.phase = Phase::Swapping;
        self.keep(kept)?;
        synced(exchange_between(applications, &name(BUNDLE), into, &name(HELD)).map_err(io)?)?;
        kept.phase = Phase::PlacedUnverified;
        self.keep(kept)?;
        self.check_placed(kept, applications, into)
    }

    /// Whether both places hold what the swap put there.
    fn swapped(&self, kept: &Transaction, applications: &File, into: &File) -> Result<bool> {
        Ok(Identity::of(applications, &name(BUNDLE))? == kept.new
            && Identity::of(into, &name(HELD))? == Some(kept.installed))
    }

    /// Check a swap: both places hold what was swapped, and the new bundle
    /// is still Atlas's where it is now. Failing its check with both still
    /// in place, it is swapped back, and that swap checked too.
    fn check_placed(&self, kept: &mut Transaction, applications: &File, into: &File) -> Result<()> {
        if !self.swapped(kept, applications, into)? {
            return self.pend(kept, "A bundle is not where its update put it.");
        }
        let installed = self.layout.applications().join(BUNDLE);
        let verified = self.system.verified(&installed).map(|cdhash| hex(&cdhash));
        if verified == kept.cdhash {
            kept.phase = Phase::Placed;
            kept.placed_at = Some(self.system.now());
            return self.keep(kept);
        }
        kept.why = Some("The placed bundle did not verify where it was placed.".into());
        self.roll_back(kept)
    }

    /// Release `transaction`, asked by a daemon running the code `cdhash`.
    pub(super) fn release(
        &self,
        request: &Token,
        transaction: &Token,
        cdhash: &[u8],
    ) -> Result<Value> {
        let _one = self.lock();
        let asked = match self.begin(request, "release", transaction.as_str(), Some(transaction))? {
            Ok(asked) => asked,
            Err(reply) => return Ok(reply),
        };
        let Some(mut kept) = Transaction::read(&self.record(transaction))? else {
            return self.answer(asked, refusal("No such update."));
        };
        // A release cut short before its answer, or asked again once done:
        // where it went is its answer.
        if matches!(kept.phase, Phase::Releasing | Phase::Released) {
            if kept.phase == Phase::Releasing {
                self.finish_release(&mut kept)?;
            }
            let reply = phase_reply(&kept);
            return self.answer(asked, reply);
        }
        if kept.phase != Phase::Placed {
            return self.answer(asked, refusal("This update is not placed."));
        }
        if kept.cdhash.as_deref() != Some(hex(cdhash).as_str()) {
            return self.answer(
                asked,
                refusal("Only a daemon of the new build releases its update."),
            );
        }
        kept.phase = Phase::Releasing;
        self.keep(&kept)?;
        self.finish_release(&mut kept)?;
        let reply = phase_reply(&kept);
        self.answer(asked, reply)
    }

    /// Release, from wherever it was cut short: the installer's own copy
    /// replaced by its frozen next one, then the previous bundle removed.
    fn finish_release(&self, kept: &mut Transaction) -> Result<()> {
        let folder = self.folder(&kept.id);
        let into = File::open(&folder).map_err(io)?;
        let next = name(NEXT);
        let frozen = folder.join(NEXT).exists().then_some((&into, &next));
        let wanted =
            |bundle: &Path| self.system.verified(bundle).map(|cdhash| hex(&cdhash)) == kept.cdhash;
        // Any failure here may pass, a signature check that could not run
        // looking just like a copy of other code: it stays releasing, and
        // asking again or recovery finishes it.
        super::own::take(&self.layout.home(), frozen, &wanted)?;
        // The fallback goes only while what is installed is this update's.
        let applications = File::open(self.layout.applications()).map_err(io)?;
        if Identity::of(&applications, &name(BUNDLE))? != kept.new {
            return self.pend(kept, "The installed Atlas is not this update's.");
        }
        if let Step::Pended = self.remove_held(kept, &into, Some(kept.installed))? {
            return Ok(());
        }
        kept.phase = Phase::Released;
        self.keep(kept)
    }

    /// Roll `transaction` back, as asked.
    pub(super) fn rollback(&self, request: &Token, transaction: &Token) -> Result<Value> {
        let _one = self.lock();
        let asked =
            match self.begin(request, "rollback", transaction.as_str(), Some(transaction))? {
                Ok(asked) => asked,
                Err(reply) => return Ok(reply),
            };
        let Some(mut kept) = Transaction::read(&self.record(transaction))? else {
            return self.answer(asked, refusal("No such update."));
        };
        // A rollback cut short before its answer, or asked again once done:
        // where it went is its answer.
        if matches!(
            kept.phase,
            Phase::RollingBack | Phase::Restarting | Phase::RolledBack
        ) {
            if kept.phase != Phase::RolledBack {
                self.roll_back(&mut kept)?;
            }
            let reply = phase_reply(&kept);
            return self.answer(asked, reply);
        }
        if kept.phase != Phase::Placed {
            return self.answer(asked, refusal("This update cannot be rolled back now."));
        }
        self.roll_back(&mut kept)?;
        let reply = phase_reply(&kept);
        self.answer(asked, reply)
    }

    /// Put the previous bundle back, from wherever it was: each place's
    /// identity checked before the swap and after it, the new bundle's copy
    /// removed, and only then its daemon started again.
    fn roll_back(&self, kept: &mut Transaction) -> Result<()> {
        let applications = File::open(self.layout.applications()).map_err(io)?;
        let into = File::open(self.folder(&kept.id)).map_err(io)?;
        if kept.phase != Phase::Restarting {
            if kept.phase != Phase::RollingBack
                || Identity::of(&applications, &name(BUNDLE))? == kept.new
            {
                if !self.swapped(kept, &applications, &into)? {
                    return self.pend(kept, "A bundle is not where its update put it.");
                }
                kept.phase = Phase::RollingBack;
                self.keep(kept)?;
                synced(
                    exchange_between(&applications, &name(BUNDLE), &into, &name(HELD))
                        .map_err(io)?,
                )?;
            }
            if Identity::of(&applications, &name(BUNDLE))? != Some(kept.installed) {
                return self.pend(kept, "The previous Atlas is not back in place.");
            }
            let new = kept.new;
            if let Step::Pended = self.remove_held(kept, &into, new)? {
                return Ok(());
            }
            // The installer keeps the copy it runs: the next one goes too.
            let _ = std::fs::remove_dir_all(self.folder(&kept.id).join(NEXT));
            kept.phase = Phase::Restarting;
            self.keep(kept)?;
        }
        // The daemon the previous bundle runs, started again, whatever the
        // new one was doing; until it is, a restart is owed and tried again.
        if self.system.restart(kept.uid) {
            kept.phase = Phase::RolledBack;
            self.keep(kept)?;
        }
        Ok(())
    }

    /// Remove the bundle held in a transaction's folder, when it is the one
    /// `expected`; anything else there is left and the transaction pends.
    fn remove_held(
        &self,
        kept: &mut Transaction,
        into: &File,
        expected: Option<Identity>,
    ) -> Result<Step> {
        match Identity::of(into, &name(HELD))? {
            None => Ok(Step::Done),
            Some(held) if Some(held) == expected => {
                std::fs::remove_dir_all(self.folder(&kept.id).join(HELD)).map_err(io)?;
                into.sync_all().map_err(io)?;
                Ok(Step::Done)
            }
            Some(_) => {
                self.pend(kept, "Something unexpected is in an update's folder.")?;
                Ok(Step::Pended)
            }
        }
    }

    fn abort(&self, kept: &mut Transaction) -> Result<()> {
        let _ = std::fs::remove_dir_all(self.folder(&kept.id));
        kept.phase = Phase::Aborted;
        self.keep(kept)
    }

    fn pend(&self, kept: &mut Transaction, why: &str) -> Result<()> {
        kept.phase = Phase::Pending;
        kept.why = Some(why.into());
        self.keep(kept)
    }

    /// What request `request` came to, read without changing anything.
    pub(super) fn outcome(&self, request: &Token) -> Result<Value> {
        match Asked::read(&self.request_record(request))? {
            Some(asked) => self.reply_of(&asked),
            None => Ok(json!({"ok": true, "asked": false})),
        }
    }

    /// Where `transaction` is, read without changing anything.
    pub(super) fn transaction(&self, transaction: &Token) -> Result<Value> {
        Ok(match Transaction::read(&self.record(transaction))? {
            Some(kept) => phase_reply(&kept),
            None => refusal("No such update."),
        })
    }

    /// Settle what a failure left unfinished, and roll back a placed update
    /// no daemon of its build released in time: every half a minute, so a
    /// step that failed after its swap never waits for a restart.
    pub(super) fn deadline(&self) -> Result<()> {
        let _one = self.lock();
        let now = self.system.now();
        for mut kept in self.all()? {
            let due = kept
                .placed_at
                .is_some_and(|placed| now.saturating_sub(placed) >= DEADLINE_MS);
            if kept.phase == Phase::Placed && due {
                self.roll_back(&mut kept)?;
            } else {
                self.settle(&mut kept)?;
            }
        }
        Ok(())
    }

    /// Settle every transaction a crash left open, by its record and the
    /// identities found, before anything is answered.
    pub(super) fn recover(&self) -> Result<()> {
        let one = self.lock();
        for mut kept in self.all()? {
            self.settle(&mut kept)?;
        }
        drop(one);
        self.deadline()
    }

    /// Settle `kept`, left unfinished by a crash or a failure, by its record
    /// and the identities found; one at rest is left as it is. Called with
    /// the lock held, so no mutation is under way.
    fn settle(&self, kept: &mut Transaction) -> Result<()> {
        match kept.phase {
            Phase::Copying | Phase::Verified => self.abort(kept),
            Phase::Swapping | Phase::PlacedUnverified => {
                let applications = File::open(self.layout.applications()).map_err(io)?;
                let into = File::open(self.folder(&kept.id)).map_err(io)?;
                let placed = Identity::of(&applications, &name(BUNDLE))?;
                if placed == Some(kept.installed) && kept.phase == Phase::Swapping {
                    // Not swapped: what is held is its own copy, or nothing. Anything
                    // else there is left, and the update pends.
                    let held = Identity::of(&into, &name(HELD))?;
                    if held.is_some() && held != kept.new {
                        return self.pend(kept, "Something unexpected is in an update's folder.");
                    }
                    self.abort(kept)
                } else if placed == Some(kept.installed) {
                    // Swapped back after its check failed: what is left is its copy.
                    kept.phase = Phase::RollingBack;
                    self.keep(kept)?;
                    self.roll_back(kept)
                } else if placed == kept.new && kept.phase == Phase::PlacedUnverified {
                    // Swapped and its check not yet done: checked now.
                    self.check_placed(kept, &applications, &into)
                } else if placed == kept.new {
                    // Swapped without its record saying so: never finished blind.
                    self.roll_back(kept)
                } else {
                    self.pend(
                        kept,
                        "The installed Atlas is neither the previous one nor the new one.",
                    )
                }
            }
            Phase::RollingBack | Phase::Restarting => self.roll_back(kept),
            Phase::Releasing => self.finish_release(kept),
            Phase::Placed
            | Phase::Released
            | Phase::RolledBack
            | Phase::Aborted
            | Phase::Pending => Ok(()),
        }
    }
}

/// `bytes` in lowercase hexadecimal.
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
#[path = "transaction_tests.rs"]
mod tests;

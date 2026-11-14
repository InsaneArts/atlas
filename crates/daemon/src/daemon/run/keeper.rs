//! What outlives a supervised command's reply: its record on the guest's
//! disk, kept before its program may start; the word to start, given only
//! once that record is kept and the command is still admitted; the lease,
//! renewed until the work may no longer run; and the supervisor's end,
//! judged by its own exit code and its proof, which decides whether its
//! record may go.
use super::Lease;
use crate::machine::{
    processes::{Remembered, Supervised},
    supervise::{DONE, ENDED, RENEW, proof},
};
use atlas_core::{error::Code, process::Instance, protocol::failure};
use serde_json::Value;
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex, PoisonError},
    time::Duration,
};
use tokio::{io::AsyncWriteExt, net::unix::OwnedWriteHalf, process::Child, sync::oneshot};

/// How often the lease is renewed.
const RENEWAL: Duration = Duration::from_millis(500);

/// How a keeper stands with its supervisor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Keeping {
    /// Its work may run.
    Active,
    /// Its work is being ended, or has ended and is not yet judged.
    Ending,
    /// Its supervisor ended without proving its work did.
    Uncertain,
}

/// The supervisors this daemon's keepers own, and how each stands.
#[derive(Default)]
pub(crate) struct Keepers(Mutex<HashMap<Instance, Keeping>>);

impl Keepers {
    fn set(&self, supervisor: Instance, keeping: Keeping) {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(supervisor, keeping);
    }

    fn leave(&self, supervisor: &Instance) {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(supervisor);
    }

    /// A Stop was carried out: every keeper whose work may run now has it
    /// ended, so none counts as settled until it is judged.
    pub(crate) fn ending_all(&self) {
        for keeping in self
            .0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .values_mut()
        {
            if *keeping == Keeping::Active {
                *keeping = Keeping::Ending;
            }
        }
    }

    /// Whether any work is not known to have ended: changes are refused
    /// while it is.
    pub(crate) fn uncertain(&self) -> bool {
        self.any(|keeping| keeping == Keeping::Uncertain)
    }

    /// Run `then` only while every keeper's work may run, with none able to
    /// change how it stands meanwhile: whether it ran.
    pub(crate) fn settled_then(&self, then: impl FnOnce()) -> bool {
        let keepers = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        let settled = keepers.values().all(|keeping| *keeping == Keeping::Active);
        if settled {
            then();
        }
        settled
    }

    /// Whether any work is being ended, or is not known to have ended: a
    /// Resume waits for neither.
    pub(crate) fn unsettled(&self) -> bool {
        self.any(|keeping| keeping != Keeping::Active)
    }

    fn any(&self, matching: impl Fn(Keeping) -> bool) -> bool {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .values()
            .any(|keeping| matching(*keeping))
    }
}

/// What the keeper tells the command about its start.
pub(super) enum Verdict {
    /// Told to start: whether it did, its supervisor says.
    Went,
    /// Never told to start: nothing ran.
    Refused(Value),
}

/// Keep `supervisor`'s work, `child`: kept on disk, told to start,
/// renewed, then judged once its command has `released` it.
pub(super) async fn keep(
    supervisor: Instance,
    mut lifeline: OwnedWriteHalf,
    lease: Lease,
    verdict: oneshot::Sender<Verdict>,
    starting: Arc<Mutex<bool>>,
    mut child: Child,
    released: oneshot::Receiver<()>,
) {
    let Lease {
        proofs,
        generation,
        mut over,
        admitted,
        kept,
        keepers,
        ..
    } = lease;
    keepers.set(supervisor, Keeping::Active);
    let supervised = atlas_core::process::boot()
        .map(|boot| Supervised {
            boot,
            generation,
            supervisor,
        })
        .map_err(|error| failure(error.code, error.error));
    // Kept before the word to start, and always waited for: the lease, not
    // a timeout here, bounds a supervisor told nothing.
    let registered = match supervised.clone() {
        Ok(supervised) => {
            let kept = kept.clone();
            tokio::task::spawn_blocking(move || kept.add(supervised))
                .await
                .map_err(|_| failure(Code::Io, "The command could not be kept before it started"))
                .and_then(|added| added.map_err(|error| failure(error.code, error.error)))
        }
        Err(refusal) => Err(refusal),
    };
    let refusal = match registered {
        Ok(()) => admitted(),
        Err(refusal) => Some(refusal),
    };
    let mut ended_by_lease = false;
    match refusal {
        Some(refusal) => {
            // Dropping the lifeline ends the supervisor, which started nothing.
            drop(lifeline);
            let _ = verdict.send(Verdict::Refused(refusal));
        }
        None if let Err(refusal) = go(&starting, &lifeline, &*admitted).await => {
            drop(lifeline);
            let _ = verdict.send(Verdict::Refused(refusal));
        }
        None => {
            let _ = verdict.send(Verdict::Went);
            ended_by_lease = renew(&mut lifeline, &mut over).await;
            drop(lifeline);
        }
    }
    keepers.set(supervisor, Keeping::Ending);
    // Reaped only once the command is done signalling its group: until
    // then its PID keeps the group.
    let _ = released.await;
    let ended = child.wait().await.ok().and_then(|status| status.code());
    let proven = ended
        .and_then(|code| u8::try_from(code).ok())
        .filter(|code| [DONE, ENDED].contains(code))
        .is_some_and(|code| proof::left(&proofs, &supervisor) == Some(code));
    if !proven {
        keepers.set(supervisor, Keeping::Uncertain);
        return;
    }
    // Ended because its lease was over: the Stop being saved for it decides,
    // and recovery retires it under that Stop, by its proof.
    if !ended_by_lease
        && let Ok(supervised) = supervised
        && !retire(kept, supervised, proofs).await
    {
        keepers.set(supervisor, Keeping::Uncertain);
        return;
    }
    keepers.leave(&supervisor);
}

/// Renew over `lifeline` until the lease is `over` or the supervisor is
/// gone: whether it was over.
async fn renew(
    lifeline: &mut OwnedWriteHalf,
    over: &mut std::pin::Pin<Box<dyn Future<Output = ()> + Send>>,
) -> bool {
    let mut every = tokio::time::interval(RENEWAL);
    // The first tick is the word to start, already given.
    every.tick().await;
    loop {
        tokio::select! {
            biased;
            () = over.as_mut() => return true,
            _ = every.tick() => {
                if lifeline.write_all(&[RENEW]).await.is_err() {
                    return false;
                }
            }
        }
    }
}

/// Give the word to start, unless the command has stopped waiting for it
/// or is no longer admitted: checked under one lock, right before the word,
/// so a command ended before its program started never has it start after.
/// The refusal when the word was not given.
async fn go(
    starting: &Mutex<bool>,
    lifeline: &OwnedWriteHalf,
    admitted: &(dyn Fn() -> Option<Value> + Send + Sync),
) -> std::result::Result<(), Value> {
    let gone = || {
        failure(
            Code::SpawnFailed,
            "The command's supervisor was gone before it was told to start",
        )
    };
    loop {
        lifeline.writable().await.map_err(|_| gone())?;
        let ended = starting.lock().unwrap_or_else(PoisonError::into_inner);
        if *ended {
            return Err(failure(
                Code::Cancelled,
                "The command was ended before it started",
            ));
        }
        if let Some(refusal) = admitted() {
            return Err(refusal);
        }
        // One byte: written whole, or not at all, and tried again once
        // the socket says it can take it.
        match lifeline.try_write(&[RENEW]) {
            Ok(1) => return Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
            Ok(_) | Err(_) => return Err(gone()),
        }
    }
}

/// Forget a supervisor whose work is proved ended: its record, then its
/// proof. Whether both are forgotten; one that is not is left for recovery.
async fn retire(kept: Arc<dyn Remembered>, supervised: Supervised, proofs: PathBuf) -> bool {
    tokio::task::spawn_blocking(move || {
        kept.retire(std::slice::from_ref(&supervised))?;
        proof::forget(&proofs, &supervised.supervisor)?;
        Ok::<_, atlas_core::error::Error>(())
    })
    .await
    .is_ok_and(|retired| retired.is_ok())
}

//! Pairing a VM's guest with the station through the station's own code:
//! with the enrolment kept, it pairs on one connection and, cut short once
//! the guest may have committed, completes on a connection of its own. Its
//! record is kept in the VM's station folder; once paired, the enrolment
//! is deleted. A pairing cut short resumes from the record's pending state.
use super::enrolment::Enrolments;
use atlas_core::{
    desktop::DesktopId,
    error::{Code, Error, Result},
    files::Token,
    session::paired::Paired,
};
use atlas_session::{
    keys::Keypair,
    pairing::{Enrolment, StationRecords, station_completes, station_pairs},
    station::Dial,
};
use std::{future::Future, sync::Arc, time::Duration};

/// The longest one attempt may take, its connection included: the guest's
/// own bound on a pairing.
const ATTEMPT: Duration = Duration::from_secs(10);

/// What a pairing needs: the VM, the way to its guest, the station's key,
/// where the enrolment is kept and where its record goes.
pub struct Pairing<'a> {
    pub vm: &'a DesktopId,
    /// The VM's name, as the settings list it.
    pub name: &'a str,
    /// A new connection to the guest for pairing on each dial.
    pub dial: &'a dyn Dial,
    pub own: &'a Keypair,
    pub enrolments: Arc<dyn Enrolments>,
    pub records: Arc<dyn StationRecords>,
}

/// Pair the VM `pairing` names, or finish a pairing cut short, and delete
/// its enrolment once paired. Paired already, it only deletes the
/// enrolment, which an earlier pairing may have left.
///
/// # Errors
///
/// Fails when the VM's pairing was revoked, no enrolment is kept for an
/// unpaired VM, the guest does not pair, or a record or the enrolment
/// cannot be read, kept or deleted; what was kept stays, to resume from.
pub async fn pair(pairing: &Pairing<'_>) -> Result<Paired> {
    let paired = match read(&pairing.records).await? {
        Some(Enrolment::Paired(paired)) => paired,
        Some(Enrolment::Revoked { .. }) => {
            return Err(Error::new(
                Code::Identity,
                "The VM's pairing was revoked; it is not paired again.",
            ));
        }
        Some(Enrolment::Pending { change, .. }) => match complete(pairing, &change).await {
            Ok(paired) => paired,
            // A guest that never committed proves the secret again.
            Err(_) => with_secret(pairing).await?,
        },
        None => with_secret(pairing).await?,
    };
    forget(pairing).await?;
    Ok(paired)
}

/// Pair with the secret kept, then, cut short once the pending record was
/// kept, complete on a connection of its own.
async fn with_secret(pairing: &Pairing<'_>) -> Result<Paired> {
    let (enrolments, vm) = (pairing.enrolments.clone(), pairing.vm.clone());
    let enrolling = blocking(move || enrolments.kept(&vm))
        .await?
        .ok_or_else(|| {
            Error::new(
                Code::Identity,
                "The VM is not paired and no enrolment is kept for it; enrol it first.",
            )
        })?;
    let paired = bounded(async {
        let stream = dialled(pairing).await?;
        station_pairs(
            stream,
            pairing.own,
            &enrolling.secret,
            pairing.vm,
            &enrolling.change,
            &pairing.records,
        )
        .await
    })
    .await;
    let failed = match paired {
        Ok(paired) => return Ok(paired),
        Err(failed) => failed,
    };
    match read(&pairing.records).await? {
        Some(Enrolment::Pending { change, .. }) if change == enrolling.change => {
            complete(pairing, &change).await
        }
        _ => Err(failed),
    }
}

/// Complete enrolment `change`, whose guest may have committed, on a new
/// connection.
async fn complete(pairing: &Pairing<'_>, change: &Token) -> Result<Paired> {
    bounded(async {
        let stream = dialled(pairing).await?;
        station_completes(stream, pairing.own, pairing.vm, change, &pairing.records).await
    })
    .await
}

async fn dialled(pairing: &Pairing<'_>) -> Result<atlas_session::station::Raw> {
    pairing.dial.dial().await.map_err(|error| {
        Error::new(
            Code::Transport,
            format!("Could not reach {} to pair it.", pairing.name),
        )
        .with_source(error)
    })
}

/// Delete the enrolment, with the Keychain's prompts off.
async fn forget(pairing: &Pairing<'_>) -> Result<()> {
    let (enrolments, vm) = (pairing.enrolments.clone(), pairing.vm.clone());
    blocking(move || enrolments.forget(&vm)).await.map(|_| ())
}

async fn read(records: &Arc<dyn StationRecords>) -> Result<Option<Enrolment>> {
    let records = records.clone();
    blocking(move || records.read()).await
}

/// `attempt`, given up past [`ATTEMPT`].
async fn bounded<T>(attempt: impl Future<Output = Result<T>>) -> Result<T> {
    tokio::time::timeout(ATTEMPT, attempt)
        .await
        .map_err(|_| Error::new(Code::Transport, "The VM's guest did not pair in time."))?
}

/// `work`, which waits on the disk or the Keychain, off the runtime.
async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T> + Send + 'static,
) -> Result<T> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|_| Error::new(Code::State, "Keeping the VM's pairing stopped."))?
}

#[cfg(test)]
#[path = "pairing_tests.rs"]
mod tests;

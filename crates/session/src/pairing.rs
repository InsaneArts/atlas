//! Pairing a machine desktop with the station, once, with a one-time
//! secret.
//!
//! The guest dials and proves the secret in the handshake, then offers the
//! enrolment its secret came with. The station keeps the guest's key as a
//! pending enrolment, durably, and only then confirms: its confirmation is
//! the first proof the guest has that the station holds the secret too. The
//! guest commits the station's key, durably, forgets the secret, and acks;
//! the station marks the desktop paired, durably.
//!
//! A crash anywhere recovers by what the guest has. A guest that never
//! committed pairs again with the same secret, and the station takes the
//! key it authenticates in place of the one pending. A guest that committed
//! connects with both keys pinned and acks again; the station's pending key
//! is the one it pins. Every message names the enrolment and may come
//! twice.
use crate::{
    keys::{Keypair, PairingSecret},
    secure::{
        Purpose, Secure, connect_as_guest, connect_as_station, pair_as_guest, pair_as_station,
    },
};
use atlas_core::{
    desktop::DesktopId,
    error::{Code, Error, Result},
    files::Token,
    session::{key::PeerKey, paired::Paired},
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// The longest pairing message read.
const LINE: usize = 4096;

/// The station's record of one desktop's pairing.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "lowercase", deny_unknown_fields)]
pub enum Enrolment {
    /// The guest proved the secret with this key, and has not acked.
    Pending { change: Token, guest: PeerKey },
    /// The pairing is complete.
    Paired(Paired),
    /// The pairing was revoked: its keys are refused from now on.
    Revoked { change: Token },
}

/// Where the station keeps a desktop's pairing.
pub trait StationRecords: Send + Sync {
    /// The record kept; none when there is none.
    ///
    /// # Errors
    ///
    /// Fails, and so refuses, on a record that cannot be read, is damaged,
    /// or was written by a later version: none of those is an absent one.
    fn read(&self) -> Result<Option<Enrolment>>;

    /// Keep `record` in place of `expected`, what was read, durably before
    /// it returns: checked and kept under the desktop's own exclusion, which
    /// every station process takes, so a record that changed since it was
    /// read (revoked meanwhile, say) is never overwritten.
    ///
    /// # Errors
    ///
    /// Fails when the record changed since, or `record` is not kept; then
    /// nothing that depends on it is sent.
    fn replace(&self, expected: Option<&Enrolment>, record: &Enrolment) -> Result<()>;
}

/// Where the guest keeps its side of the pairing.
pub trait GuestRecords: Send + Sync {
    /// The station's key and the enrolment, once committed.
    ///
    /// # Errors
    ///
    /// Fails on a record that cannot be read or is damaged.
    fn committed(&self) -> Result<Option<(PeerKey, Token)>>;

    /// Commit to `station`'s key for `change`, durably before it returns,
    /// and only where nothing is committed yet: the same commitment again
    /// succeeds and changes nothing, any other is refused. Checked and kept
    /// at once, so of two attempts that overlap, at most one commits, and
    /// one that was given up and still runs never replaces the other's.
    ///
    /// # Errors
    ///
    /// Fails when another commitment is kept, or this one is not kept.
    fn commit(&self, station: PeerKey, change: &Token) -> Result<()>;

    /// The pairing secret, while it is kept.
    ///
    /// # Errors
    ///
    /// Fails when it cannot be read.
    fn secret(&self) -> Result<Option<PairingSecret>>;

    /// Forget the secret, for good.
    ///
    /// # Errors
    ///
    /// Fails when it cannot be removed.
    fn forget_secret(&self) -> Result<()>;
}

/// One pairing message.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase", deny_unknown_fields)]
enum Said {
    /// The guest offers the enrolment its secret came with.
    Offer { change: Token },
    /// The station kept the guest's key.
    Confirm { change: Token },
    /// The guest committed, and says what it is.
    Ack {
        change: Token,
        uid: u32,
        boot: String,
    },
}

/// What the guest says of itself when it pairs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GuestFacts {
    pub uid: u32,
    pub boot: String,
}

/// Pair as the station: the guest dialled on `stream` with the secret
/// made for enrolment `change`. Returns the pairing once it is complete
/// and kept; the secret may be dropped only then.
///
/// # Errors
///
/// Fails when the guest does not prove the secret or offers another
/// enrolment, when the desktop is paired or revoked already, when a record
/// cannot be kept, or when the connection fails; whatever was kept stays.
pub async fn station_pairs<S: AsyncRead + AsyncWrite + Unpin>(
    stream: S,
    own: &Keypair,
    secret: &PairingSecret,
    desktop: &DesktopId,
    change: &Token,
    records: &Arc<dyn StationRecords>,
) -> Result<Paired> {
    let mut secure = pair_as_station(stream, own, secret, desktop).await?;
    expect_offer(&mut secure, change).await?;
    let kept = off_runtime(records, |records| records.read()).await?;
    match &kept {
        None => {}
        // The same enrolment again: the key it authenticates now is kept.
        Some(Enrolment::Pending {
            change: pending, ..
        }) if pending == change => {}
        Some(Enrolment::Pending { .. }) => return Err(refused("another enrolment is pending")),
        Some(Enrolment::Paired(_)) => return Err(refused("the desktop is paired already")),
        Some(Enrolment::Revoked { .. }) => {
            return Err(refused("the desktop's pairing was revoked"));
        }
    }
    // Kept before the confirmation, so a guest that commits on it is always
    // one the station knows.
    let pending = Enrolment::Pending {
        change: change.clone(),
        guest: secure.peer(),
    };
    off_runtime(records, move |records| {
        records.replace(kept.as_ref(), &pending)
    })
    .await?;
    say(
        &mut secure,
        &Said::Confirm {
            change: change.clone(),
        },
    )
    .await?;
    complete(&mut secure, own, change, records).await
}

/// Complete, as the station, an enrolment whose guest committed already:
/// it connects with both keys pinned, the station's pending key the one
/// pinned, and acks again.
///
/// # Errors
///
/// Fails when no enrolment is pending, the guest is not the one pending,
/// a record cannot be kept, or the connection fails.
pub async fn station_completes<S: AsyncRead + AsyncWrite + Unpin>(
    stream: S,
    own: &Keypair,
    desktop: &DesktopId,
    change: &Token,
    records: &Arc<dyn StationRecords>,
) -> Result<Paired> {
    let guest = match off_runtime(records, |records| records.read()).await? {
        Some(Enrolment::Pending {
            change: pending,
            guest,
        }) if &pending == change => guest,
        Some(Enrolment::Paired(paired)) if &paired.change == change => paired.guest,
        _ => return Err(refused("no such enrolment is pending")),
    };
    let mut secure = connect_as_station(stream, own, guest, desktop, Purpose::Pairing).await?;
    complete(&mut secure, own, change, records).await
}

/// The guest's ack, then the pairing kept: once kept, an ack that comes
/// again changes nothing.
async fn complete<S: AsyncRead + AsyncWrite + Unpin>(
    secure: &mut Secure<S>,
    own: &Keypair,
    change: &Token,
    records: &Arc<dyn StationRecords>,
) -> Result<Paired> {
    let (uid, boot) = match hear(secure).await? {
        Said::Ack {
            change: acked,
            uid,
            boot,
        } if &acked == change => (uid, boot),
        _ => return Err(protocol("the guest did not ack its enrolment")),
    };
    let paired = Paired {
        station: own.public().fingerprint(),
        guest: secure.peer(),
        uid,
        boot,
        change: change.clone(),
    };
    let kept = off_runtime(records, |records| records.read()).await?;
    match &kept {
        Some(Enrolment::Paired(done))
            if done.guest == paired.guest && done.change == paired.change =>
        {
            return Ok(done.clone());
        }
        Some(Enrolment::Pending {
            change: pending,
            guest,
        }) if pending == change && *guest == paired.guest => {}
        _ => return Err(refused("the guest is not the one pending")),
    }
    let done = Enrolment::Paired(paired.clone());
    off_runtime(records, move |records| {
        records.replace(kept.as_ref(), &done)
    })
    .await?;
    let _ = secure.shutdown().await;
    Ok(paired)
}

/// Pair as the guest for enrolment `change`, on `stream` dialled to the
/// station. A guest that committed already connects with both keys pinned
/// and acks again; one that never did proves the secret. The station's key
/// once paired.
///
/// # Errors
///
/// Fails when there is neither a commitment nor a secret, the station does
/// not prove the secret, a record cannot be kept, or the connection fails.
pub async fn guest_pairs<S: AsyncRead + AsyncWrite + Unpin>(
    stream: S,
    own: &Keypair,
    desktop: &DesktopId,
    change: &Token,
    facts: &GuestFacts,
    records: &Arc<dyn GuestRecords>,
) -> Result<PeerKey> {
    if let Some((station, committed)) = off_runtime(records, |records| records.committed()).await? {
        if &committed != change {
            return Err(refused("the guest is paired for another enrolment"));
        }
        // A commitment that outlived its secret's removal finishes it.
        if off_runtime(records, |records| records.secret())
            .await?
            .is_some()
        {
            off_runtime(records, |records| records.forget_secret()).await?;
        }
        let mut secure = connect_as_guest(stream, own, station, desktop, Purpose::Pairing).await?;
        ack(&mut secure, change, facts).await?;
        return Ok(station);
    }
    let secret = off_runtime(records, |records| records.secret())
        .await?
        .ok_or_else(|| refused("the guest holds no pairing secret"))?;
    let mut secure = pair_as_guest(stream, own, &secret, desktop).await?;
    say(
        &mut secure,
        &Said::Offer {
            change: change.clone(),
        },
    )
    .await?;
    // The station's confirmation opening is its proof of the secret.
    match hear(&mut secure).await? {
        Said::Confirm { change: confirmed } if &confirmed == change => {}
        _ => return Err(protocol("the station did not confirm the enrolment")),
    }
    let station = secure.peer();
    // The ack must be one the station can read before anything is
    // committed on it.
    line(&ack_of(change, facts))?;
    let committing = change.clone();
    off_runtime(records, move |records| records.commit(station, &committing)).await?;
    off_runtime(records, |records| records.forget_secret()).await?;
    ack(&mut secure, change, facts).await?;
    Ok(station)
}

async fn ack<S: AsyncRead + AsyncWrite + Unpin>(
    secure: &mut Secure<S>,
    change: &Token,
    facts: &GuestFacts,
) -> Result<()> {
    say(secure, &ack_of(change, facts)).await?;
    let _ = secure.shutdown().await;
    Ok(())
}

fn ack_of(change: &Token, facts: &GuestFacts) -> Said {
    Said::Ack {
        change: change.clone(),
        uid: facts.uid,
        boot: facts.boot.clone(),
    }
}

async fn expect_offer<S: AsyncRead + AsyncWrite + Unpin>(
    secure: &mut Secure<S>,
    change: &Token,
) -> Result<()> {
    match hear(secure).await? {
        Said::Offer { change: offered } if &offered == change => Ok(()),
        Said::Offer { .. } => Err(refused("the guest offered another enrolment")),
        _ => Err(protocol("the guest did not offer its enrolment")),
    }
}

/// `said` as the line that goes, when the other end can read one so long.
fn line(said: &Said) -> Result<Vec<u8>> {
    let mut line = serde_json::to_vec(said)?;
    if line.len() > LINE {
        return Err(protocol("a pairing message would be too long to read"));
    }
    line.push(b'\n');
    Ok(line)
}

async fn say<S: AsyncRead + AsyncWrite + Unpin>(secure: &mut Secure<S>, said: &Said) -> Result<()> {
    let line = line(said)?;
    secure.write_all(&line).await.map_err(io)?;
    secure.flush().await.map_err(io)
}

async fn hear<S: AsyncRead + AsyncWrite + Unpin>(secure: &mut Secure<S>) -> Result<Said> {
    // Byte by byte, so nothing after the message is taken from the stream.
    let mut line = Vec::new();
    let mut byte = [0; 1];
    while line.len() <= LINE {
        secure.read_exact(&mut byte).await.map_err(io)?;
        if byte[0] == b'\n' {
            return serde_json::from_slice(&line)
                .map_err(|_| protocol("a pairing message was not understood"));
        }
        line.push(byte[0]);
    }
    Err(protocol("a pairing message ran too long"))
}

/// Revoke the desktop's pairing: its keys are refused from now on. The
/// owner of its live generations retires them. It waits on the disk, so it
/// runs off any async runtime.
///
/// # Errors
///
/// Fails when the record cannot be read, so a damaged one is kept as it
/// is (forgetting the desktop removes it), or when it cannot be kept.
pub fn revoke(records: &dyn StationRecords) -> Result<()> {
    let kept = records.read()?;
    let change = match &kept {
        Some(Enrolment::Paired(paired)) => paired.change.clone(),
        Some(Enrolment::Pending { change, .. } | Enrolment::Revoked { change }) => change.clone(),
        None => Token::new(),
    };
    records.replace(kept.as_ref(), &Enrolment::Revoked { change })
}

/// The guest key the station admits: a completed pairing's, and none
/// otherwise. It waits on the disk, so it runs off any async runtime.
///
/// # Errors
///
/// Fails, and so admits nothing, on a record that cannot be read.
pub fn admitted(records: &dyn StationRecords) -> Result<Option<PeerKey>> {
    Ok(match records.read()? {
        Some(Enrolment::Paired(paired)) => Some(paired.guest),
        _ => None,
    })
}

/// Run `operation` on `records` off the runtime: keeping a record waits on
/// the disk and on other station processes. Given up, it still runs to its
/// end, and the state it leaves is one the steps above recover from.
async fn off_runtime<R, T>(
    records: &Arc<R>,
    operation: impl FnOnce(&R) -> Result<T> + Send + 'static,
) -> Result<T>
where
    R: ?Sized + Send + Sync + 'static,
    T: Send + 'static,
{
    let records = records.clone();
    tokio::task::spawn_blocking(move || operation(&records))
        .await
        .map_err(|_| Error::new(Code::Io, "Keeping the pairing record stopped."))?
}

fn refused(why: &str) -> Error {
    Error::new(Code::Identity, format!("The pairing was refused: {why}."))
}

fn protocol(why: &str) -> Error {
    Error::new(Code::Protocol, format!("The pairing failed: {why}."))
}

fn io(error: std::io::Error) -> Error {
    Error::new(Code::Transport, "The pairing connection failed.").with_source(error)
}

#[cfg(test)]
#[path = "pairing_tests.rs"]
mod tests;

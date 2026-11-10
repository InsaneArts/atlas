//! Who may pair with the broker, and how: the kernel names each lane's peer
//! before any credential is read, a control lane's Hello earns a one-use
//! pair token, and only the same process may spend it on a frame lane. A
//! role or a packet's own fields never grant anything; the launcher's
//! capability bounds every right.
use atlas_core::{
    frame_pipe::{
        budget::OBSERVERS_MAX,
        diagnostics::Reason,
        model::{
            Attach, Attached, Hello, Hex32, Hex64, PAIR_EXPIRES_MS, Pair, Positive, Right,
            UpdateKind, Welcome,
        },
    },
    process::{self, Instance},
};
use std::time::{Duration, Instant};
use tokio::net::UnixStream;

/// How long a pair token, and the pair it starts, may wait for its last step.
const PAIRING: Duration = Duration::from_millis(PAIR_EXPIRES_MS as u64);

/// A lane's peer as the kernel names it: an account and one process.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Peer {
    pub(crate) uid: u32,
    pub(crate) instance: Instance,
}

/// The peer on `stream`, admitted only when it is `controller`'s.
///
/// # Errors
///
/// `WrongPeer` for another account or a process macOS cannot name.
pub(crate) fn peer(stream: &UnixStream, controller: u32) -> Result<Peer, Reason> {
    let credentials = stream.peer_cred().map_err(|_| Reason::WrongPeer)?;
    let pid = credentials.pid().ok_or(Reason::WrongPeer)?;
    if credentials.uid() != controller {
        return Err(Reason::WrongPeer);
    }
    let start = process::start(pid).map_err(|_| Reason::WrongPeer)?;
    Ok(Peer {
        uid: credentials.uid(),
        instance: Instance::new(pid, start),
    })
}

/// Whether two secrets are equal, in time that depends only on their length.
fn same(left: &str, right: &str) -> bool {
    left.len() == right.len()
        && left
            .bytes()
            .zip(right.bytes())
            .fold(0, |differs, (a, b)| differs | (a ^ b))
            == 0
}

/// A pair waiting for its frame lane.
struct Pending {
    token: Hex64,
    expires: Instant,
    peer: Peer,
    welcome: Welcome,
}

/// A pair whose frame lane attached, waiting for the client to confirm it.
pub(crate) struct Half {
    expires: Instant,
    paired: Paired,
}

/// Both lanes of one observer, from one process, with what it was granted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Paired {
    pub(crate) peer: Peer,
    pub(crate) pair: Pair,
    pub(crate) rights: Vec<Right>,
    pub(crate) updates: Vec<UpdateKind>,
    pub(crate) cleanup_token: Hex64,
}

/// The broker's admission: its instance, the capability its launcher handed
/// out with the rights it carries, and the pairs under way.
pub(crate) struct Gate {
    service: Hex32,
    bootstrap: Hex64,
    rights: Vec<Right>,
    updates: Vec<UpdateKind>,
    generation: u64,
    pending: Vec<Pending>,
}

impl Gate {
    /// A gate for service `service` whose capability `bootstrap` carries at
    /// most `rights`, sending pictures as `updates`.
    pub(crate) fn new(
        service: Hex32,
        bootstrap: Hex64,
        rights: Vec<Right>,
        updates: Vec<UpdateKind>,
    ) -> Self {
        Self {
            service,
            bootstrap,
            rights,
            updates,
            generation: 0,
            pending: Vec::new(),
        }
    }

    /// Welcome `peer`'s control lane, with what it asked for that the
    /// capability allows and a pair token for its frame lane.
    ///
    /// # Errors
    ///
    /// `ExpiredCapability` for another service or capability,
    /// `RightsDenied` when observing is not granted, and `BudgetExhausted`
    /// while as many pairs as observers are under way.
    pub(crate) fn hello(
        &mut self,
        peer: Peer,
        hello: &Hello,
        now: Instant,
    ) -> Result<Welcome, Reason> {
        if hello.service_instance != self.service
            || !same(hello.bootstrap_capability.as_str(), self.bootstrap.as_str())
        {
            return Err(Reason::ExpiredCapability);
        }
        let rights = granted(&hello.requested_rights, &self.rights);
        if !rights.contains(&Right::Observe) {
            return Err(Reason::RightsDenied);
        }
        self.pending.retain(|pending| pending.expires > now);
        if self.pending.len() as u64 >= OBSERVERS_MAX {
            return Err(Reason::BudgetExhausted);
        }
        self.generation = self.generation.checked_add(1).ok_or(Reason::Oversized)?;
        let welcome = Welcome {
            service_instance: self.service.clone(),
            observer_id: Hex32::random(),
            observer_generation: Positive::new(self.generation).ok_or(Reason::Oversized)?,
            control_connection_id: Hex32::random(),
            client_challenge: hello.client_challenge.clone(),
            granted_rights: rights,
            accepted_updates: granted(&hello.offered_updates, &self.updates),
            pair_token: Hex64::random(),
            pair_expires_in_ms: PAIR_EXPIRES_MS,
            cleanup_token: Hex64::random(),
        };
        self.pending.push(Pending {
            token: welcome.pair_token.clone(),
            expires: now + PAIRING,
            peer,
            welcome: welcome.clone(),
        });
        Ok(welcome)
    }

    /// Attach `peer`'s frame lane to the pair its token names. The token is
    /// spent whatever the outcome.
    ///
    /// # Errors
    ///
    /// `ExpiredCapability` for an unknown, spent or expired token,
    /// `WrongPeer` for another process than the control lane's, and
    /// `WrongChallenge` when the lane names another observer or service.
    pub(crate) fn attach(
        &mut self,
        peer: Peer,
        attach: &Attach,
        now: Instant,
    ) -> Result<(Attached, Half), Reason> {
        let found = self
            .pending
            .iter()
            .position(|pending| same(pending.token.as_str(), attach.pair_token.as_str()))
            .ok_or(Reason::ExpiredCapability)?;
        let Pending {
            expires,
            peer: control,
            welcome,
            ..
        } = self.pending.swap_remove(found);
        if expires <= now {
            return Err(Reason::ExpiredCapability);
        }
        if peer != control {
            return Err(Reason::WrongPeer);
        }
        if attach.service_instance != welcome.service_instance
            || attach.observer_id != welcome.observer_id
            || attach.observer_generation != welcome.observer_generation
            || attach.control_connection_id != welcome.control_connection_id
        {
            return Err(Reason::WrongChallenge);
        }
        let attached = Attached {
            service_instance: welcome.service_instance.clone(),
            observer_id: welcome.observer_id.clone(),
            observer_generation: welcome.observer_generation,
            control_connection_id: welcome.control_connection_id.clone(),
            frame_challenge: attach.frame_challenge.clone(),
            frame_connection_id: Hex32::random(),
        };
        let pair = Pair {
            service_instance: welcome.service_instance,
            observer_id: welcome.observer_id,
            observer_generation: welcome.observer_generation,
            control_connection_id: welcome.control_connection_id,
            frame_connection_id: attached.frame_connection_id.clone(),
        };
        let half = Half {
            expires,
            paired: Paired {
                peer,
                pair,
                rights: welcome.granted_rights,
                updates: welcome.accepted_updates,
                cleanup_token: welcome.cleanup_token,
            },
        };
        Ok((attached, half))
    }
}

impl Half {
    /// The pair, once the client's PairConfirm names both its lanes; the
    /// broker answers PairReady with the same.
    ///
    /// # Errors
    ///
    /// `ExpiredCapability` after the pair's deadline, and `WrongChallenge`
    /// for a confirmation that names anything else.
    pub(crate) fn confirm(self, confirm: &Pair, now: Instant) -> Result<Paired, Reason> {
        if self.expires <= now {
            return Err(Reason::ExpiredCapability);
        }
        if *confirm != self.paired.pair {
            return Err(Reason::WrongChallenge);
        }
        Ok(self.paired)
    }
}

/// What was asked for that is allowed, once each, in the order asked.
fn granted<T: Copy + PartialEq>(asked: &[T], allowed: &[T]) -> Vec<T> {
    let mut granted = Vec::new();
    for item in asked {
        if allowed.contains(item) && !granted.contains(item) {
            granted.push(*item);
        }
    }
    granted
}

#[cfg(test)]
#[path = "auth_tests.rs"]
mod tests;

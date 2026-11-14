//! The right to act for one generation of a desktop's session, checked
//! immediately before every effect: each event posted, each command or job
//! started, each stage committed.
//!
//! A generation ends one way, whatever ends it: it retires. Retiring takes
//! the right away at once, without waiting on anything that runs, so a
//! command holding a lock never holds retirement back. Whoever owns the
//! work hears it once, through the hook, and cancels and cleans up from
//! there; changes nobody answered stay pending.
use crate::{clock::Clock, generation::Generation};
use atlas_core::error::{Code, Error, Result};
use std::{
    sync::{Arc, Mutex, PoisonError},
    time::Duration,
};

/// How long after the last heartbeat effects may still run.
pub const LEASE: Duration = Duration::from_secs(5);

/// Why a generation retired.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Retired {
    /// A newer generation took its place.
    Replaced,
    /// Its control connection failed.
    ControlLost,
    /// No heartbeat came within [`LEASE`].
    Lapsed,
    /// The pairing it belongs to was revoked.
    Revoked,
}

/// What hears a generation retire, once: the owner of its work.
pub type Hook = Box<dyn Fn(Generation, Retired) + Send + Sync>;

/// The right to act for one generation. Clones share it: retiring one
/// retires them all.
///
/// Every decision, whether an effect may run, whether a heartbeat renews
/// the lease, whether a lane is admitted, is made under one lock, with the
/// clock read inside it, so none can miss a retirement or a heartbeat that
/// came first. The hook is called after the lock is let go. What a check
/// cannot cover is the moment between it and the effect itself, which is
/// why it is made immediately before each one.
#[derive(Clone)]
pub struct Authority {
    shared: Arc<Shared>,
}

struct Shared {
    generation: Generation,
    clock: Arc<dyn Clock>,
    state: Mutex<State>,
    hook: Hook,
}

struct State {
    /// When the last heartbeat came, on the clock.
    heard: Duration,
    /// The latest the clock was read: a reading earlier than it means the
    /// clock went back, and nothing measured on it holds.
    seen: Duration,
    retired: Option<Retired>,
}

impl std::fmt::Debug for Authority {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Authority")
            .field("generation", &self.shared.generation)
            .field("retired", &self.retired())
            .finish_non_exhaustive()
    }
}

impl Authority {
    /// The right to act for `generation`, its lease measured on `clock`
    /// from now; `hook` hears it retire. A clock that cannot be read
    /// retires it at once.
    pub fn new(generation: Generation, clock: Arc<dyn Clock>, hook: Hook) -> Self {
        let now = clock.now();
        let authority = Self {
            shared: Arc::new(Shared {
                generation,
                clock,
                state: Mutex::new(State {
                    heard: now.unwrap_or_default(),
                    seen: now.unwrap_or_default(),
                    retired: None,
                }),
                hook,
            }),
        };
        if now.is_none() {
            authority.retire(Retired::Lapsed);
        }
        authority
    }

    pub fn generation(&self) -> Generation {
        self.shared.generation
    }

    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.shared
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// Under the lock: whether the generation is live now, retiring it for
    /// a lapse when it is not. A clock that cannot be read, or reads
    /// earlier than it read before, counts as a lapse.
    fn live(&self, state: &mut State) -> std::result::Result<Duration, (Retired, bool)> {
        if let Some(cause) = state.retired {
            return Err((cause, false));
        }
        let now = self.shared.clock.now().filter(|now| *now >= state.seen);
        if let Some(now) = now {
            state.seen = now;
        }
        match now.filter(|now| now.saturating_sub(state.heard) <= LEASE) {
            Some(now) => Ok(now),
            None => {
                state.retired = Some(Retired::Lapsed);
                Err((Retired::Lapsed, true))
            }
        }
    }

    /// Run the hook for `cause` when this call retired the generation.
    fn told(&self, cause: Retired, retired_now: bool) -> Retired {
        if retired_now {
            (self.shared.hook)(self.shared.generation, cause);
        }
        cause
    }

    /// A heartbeat came: the lease runs from now, unless it had lapsed
    /// already, which retires the generation; a retired one stays retired.
    pub fn heard(&self) {
        let lapsed = {
            let mut state = self.state();
            match self.live(&mut state) {
                Ok(now) => {
                    state.heard = now;
                    None
                }
                Err(lapse) => Some(lapse),
            }
        };
        if let Some((cause, retired_now)) = lapsed {
            self.told(cause, retired_now);
        }
    }

    /// Whether an effect may run now. Called immediately before each one;
    /// a lease found lapsed retires the generation.
    ///
    /// # Errors
    ///
    /// Refuses once the generation retired, its lease lapsed, or its clock
    /// cannot be trusted.
    pub fn check(&self) -> Result<()> {
        self.while_live(|| Ok(()))
    }

    /// Run `register` while the generation is live, under the lock that
    /// retiring takes, so what it registers is either there before a
    /// retirement or never done. It must be short and never wait.
    ///
    /// # Errors
    ///
    /// Refuses as [`Authority::check`] does, or as `register` does.
    pub fn while_live<T>(&self, register: impl FnOnce() -> Result<T>) -> Result<T> {
        let refused_for = {
            let mut state = self.state();
            match self.live(&mut state) {
                Ok(_) => return register(),
                Err(lapse) => lapse,
            }
        };
        let (cause, retired_now) = refused_for;
        Err(refused(self.told(cause, retired_now)))
    }

    /// Retire the generation for `cause`: no effect runs for it from now
    /// on, and the hook hears it. Only the first cause counts; whether this
    /// call was it.
    pub fn retire(&self, cause: Retired) -> bool {
        {
            let mut state = self.state();
            if state.retired.is_some() {
                return false;
            }
            state.retired = Some(cause);
        }
        self.told(cause, true);
        true
    }

    /// Why the generation retired, once it has.
    pub fn retired(&self) -> Option<Retired> {
        self.state().retired
    }
}

/// The refusal of an effect after `cause`: a connection that changed is
/// proved again, a station not heard from is a Stop until the person
/// resumes, and a revoked pairing is no longer this desktop's.
fn refused(cause: Retired) -> Error {
    let (code, why) = match cause {
        Retired::Replaced => (
            Code::SessionChanged,
            "a newer connection to the desktop took its place",
        ),
        Retired::ControlLost => (
            Code::SessionChanged,
            "its connection to the station was lost",
        ),
        Retired::Lapsed => (Code::Stopped, "the station has not been heard from"),
        Retired::Revoked => (Code::Identity, "the desktop's pairing was revoked"),
    };
    Error::new(code, format!("Nothing more runs here: {why}."))
}

#[cfg(test)]
#[path = "authority_tests.rs"]
mod tests;

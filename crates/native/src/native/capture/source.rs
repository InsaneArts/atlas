//! The helper's one live capture source: who owns it, and whether its native
//! objects are known to be gone. Admission is held from start until a stop
//! is confirmed, through disconnects and failed stops. A source whose stop
//! was not confirmed stays quarantined, and is checked again before anything
//! else may capture. Calls that may stop the native source block, so they
//! belong off async runtime workers.
use atlas_core::{
    error::{Code, Error, Result},
    frame_pipe::diagnostics::Lifecycle,
    protocol::CaptureBackend,
};
use std::{
    collections::VecDeque,
    sync::{Mutex, MutexGuard, PoisonError},
};

/// How many drained generations are remembered for late drain queries.
const TOMBSTONES: usize = 16;

/// Stops the native source and drains its callbacks; only Ok confirms it.
pub type Stop = fn() -> Result<()>;

/// The live source and its owner.
pub struct Source {
    state: Mutex<State>,
    stop: Stop,
}

#[derive(Default)]
struct State {
    lifecycle: Lifecycle,
    /// The newest generation handed out.
    generation: u64,
    /// The generation that owns, or last owned without a confirmed stop.
    owner: Option<(u64, CaptureBackend)>,
    drained: VecDeque<Drained>,
}

/// A generation whose stop was confirmed, and the stop request that asked
/// for it, if one did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Drained {
    generation: u64,
    stop_request: Option<u64>,
}

/// The right to produce frames as one generation, until given back through
/// [`Source::stop`].
#[derive(Debug, PartialEq, Eq)]
#[must_use]
pub struct Owner {
    generation: u64,
    backend: CaptureBackend,
}

impl Owner {
    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn backend(&self) -> CaptureBackend {
        self.backend
    }
}

/// What the source says of one generation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Drain {
    /// Its stop was confirmed, by the stop request named when one asked.
    Drained { stop_request: Option<u64> },
    /// It still owns the source, or its stop is under way or unconfirmed.
    Held(Lifecycle),
    /// Not a generation this source remembers.
    Unknown,
}

fn busy() -> Error {
    Error::new(Code::Busy, "A live capture is already open")
}

fn quarantined() -> Error {
    Error::new(
        Code::Busy,
        "The previous live capture has not confirmed it stopped; try again shortly",
    )
}

impl Source {
    /// A stopped source whose native stop is `stop`.
    pub fn new(stop: Stop) -> Self {
        Self {
            state: Mutex::new(State {
                lifecycle: Lifecycle::Stopped,
                ..State::default()
            }),
            stop,
        }
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn lifecycle(&self) -> Lifecycle {
        self.state().lifecycle
    }

    /// Own the source as a new generation producing from `backend`. A
    /// quarantined source is first asked to stop again, which blocks.
    ///
    /// # Errors
    ///
    /// Returns busy while another generation owns the source or its stop is
    /// still unconfirmed.
    pub fn admit(&self, backend: CaptureBackend) -> Result<Owner> {
        self.confirm()?;
        let mut state = self.state();
        if state.lifecycle != Lifecycle::Stopped {
            return Err(busy());
        }
        state.generation += 1;
        state.lifecycle = Lifecycle::Active;
        state.owner = Some((state.generation, backend));
        Ok(Owner {
            generation: state.generation,
            backend,
        })
    }

    /// Run `capture`, a capture outside the live source, unless an earlier
    /// native source's stop is unconfirmed; such a source is asked to stop
    /// again first. The source cannot change state while `capture` runs, so
    /// no quarantine begins between the check and the capture. This blocks.
    ///
    /// # Errors
    ///
    /// Returns busy while an earlier live source's stop is unconfirmed.
    pub fn one_shot<T>(&self, capture: impl FnOnce() -> T) -> Result<T> {
        self.confirm()?;
        let state = self.state();
        if matches!(
            state.lifecycle,
            Lifecycle::Quarantined | Lifecycle::Stopping
        ) {
            return Err(quarantined());
        }
        Ok(capture())
    }

    /// Give the source back. A native source is stopped and drained first;
    /// `stop_request` names the request that asked, for later drain queries.
    /// Without a confirmed stop the source stays quarantined. This blocks.
    ///
    /// # Errors
    ///
    /// Returns the native stop's failure; the source is then quarantined.
    pub fn stop(&self, owner: Owner, stop_request: Option<u64>) -> Result<()> {
        self.state().lifecycle = Lifecycle::Stopping;
        let stopped = match owner.backend {
            CaptureBackend::CoreGraphics => Ok(()),
            CaptureBackend::ScreenCaptureKit => (self.stop)(),
        };
        self.settle(owner.generation, stopped.is_ok(), stop_request);
        stopped
    }

    /// Ask a quarantined source to stop again, which blocks; anything else
    /// is left as it is.
    fn confirm(&self) -> Result<()> {
        let generation = {
            let mut state = self.state();
            match (state.lifecycle, state.owner) {
                (Lifecycle::Quarantined, Some((generation, _))) => {
                    state.lifecycle = Lifecycle::Stopping;
                    generation
                }
                (Lifecycle::Stopping, _) => return Err(quarantined()),
                _ => return Ok(()),
            }
        };
        let stopped = (self.stop)().is_ok();
        self.settle(generation, stopped, None);
        if stopped { Ok(()) } else { Err(quarantined()) }
    }

    fn settle(&self, generation: u64, stopped: bool, stop_request: Option<u64>) {
        let mut state = self.state();
        if !stopped {
            state.lifecycle = Lifecycle::Quarantined;
            return;
        }
        state.lifecycle = Lifecycle::Stopped;
        state.owner = None;
        state.drained.push_back(Drained {
            generation,
            stop_request,
        });
        while state.drained.len() > TOMBSTONES {
            state.drained.pop_front();
        }
    }

    /// Whether `generation` is known to be drained.
    pub fn drain(&self, generation: u64) -> Drain {
        let state = self.state();
        if state.owner.is_some_and(|(owner, _)| owner == generation) {
            return Drain::Held(state.lifecycle);
        }
        state
            .drained
            .iter()
            .find(|drained| drained.generation == generation)
            .map_or(Drain::Unknown, |drained| Drain::Drained {
                stop_request: drained.stop_request,
            })
    }
}

#[cfg(test)]
mod tests;

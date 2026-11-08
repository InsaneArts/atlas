//! The helper's one live capture source: who owns it, and whether its native
//! objects are known to be gone. Admission is held from start until a stop
//! is confirmed, through disconnects and failed stops. A source whose stop
//! was not confirmed stays quarantined, and is checked again before anything
//! else may capture. Calls that may stop the native source block, so they
//! belong off async runtime workers.
//!
//! One-shot pictures come from the same native source. With nobody watching,
//! the source keeps it open for them itself, and gives it to a viewer that
//! asks, or stops it once no picture was taken for a while.
use atlas_core::{
    error::{Code, Error, Result},
    frame_pipe::diagnostics::Lifecycle,
    process::Instance,
    protocol::CaptureBackend,
};
use std::{
    collections::VecDeque,
    sync::{Mutex, MutexGuard, PoisonError, RwLock, TryLockError},
    time::{Duration, Instant},
};

/// How many drained generations are remembered for late drain queries.
const TOMBSTONES: usize = 16;

/// How often work waiting to run between captures looks again.
const FENCE_CHECK: Duration = Duration::from_millis(5);

/// Stops the native source and drains its callbacks; only Ok confirms it.
pub type Stop = fn() -> Result<()>;

/// The live source and its owner. `state` is only ever held briefly, and
/// only after `fence` when both are held. `fence` is held by every capture
/// while it runs, one-shot or a frame, and taken whole by a native stop, so
/// no quarantine begins under a capture that passed its check, and by work
/// that must run between captures.
pub struct Source {
    state: Mutex<State>,
    fence: RwLock<()>,
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
    /// A process that serves captures, signalled to end but not yet seen
    /// gone: no capture begins until it is.
    exiting: Option<Instance>,
    /// When the source itself owns the native source for one-shot pictures,
    /// when the last of them began.
    kept: Option<Instant>,
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

fn restarting() -> Error {
    Error::new(
        Code::Busy,
        "macOS's screen recorder is restarting; try again shortly",
    )
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
            fence: RwLock::new(()),
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
        if backend != CaptureBackend::ScreenCaptureKit {
            self.release_kept(Duration::ZERO)?;
        }
        let mut state = self.state();
        // A native source kept for pictures goes on for a native viewer, under
        // the viewer's own generation: nothing was ever delivered to the source
        // itself that a stop would have to drain.
        if state.kept.is_some() && backend == CaptureBackend::ScreenCaptureKit {
            state.kept = None;
            state.generation += 1;
            state.owner = Some((state.generation, backend));
            return Ok(Owner {
                generation: state.generation,
                backend,
            });
        }
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

    /// Run `capture`, a one-shot picture, unless an earlier native source's
    /// stop is unconfirmed; such a source is asked to stop again first. With
    /// nobody owning the source, it keeps the native source for this picture
    /// and the next ones itself, until [`Source::release_kept`]. Under a
    /// native owner, `capture` may open the native stream; under a
    /// CoreGraphics one it may not. The source cannot change state while
    /// `capture` runs, so no quarantine begins between the check and the
    /// capture. This blocks.
    ///
    /// # Errors
    ///
    /// Returns busy while an earlier live source's stop is unconfirmed.
    pub fn one_shot<T>(&self, capture: impl FnOnce() -> T) -> Result<T> {
        self.confirm()?;
        let _capturing = self.fence.read().unwrap_or_else(PoisonError::into_inner);
        let native;
        {
            let mut state = self.state();
            if matches!(
                state.lifecycle,
                Lifecycle::Quarantined | Lifecycle::Stopping
            ) {
                return Err(quarantined());
            }
            // Nobody owns the source: it keeps the native source for this
            // picture and the next ones itself.
            if state.lifecycle == Lifecycle::Stopped {
                state.generation += 1;
                state.lifecycle = Lifecycle::Active;
                state.owner = Some((state.generation, CaptureBackend::ScreenCaptureKit));
                state.kept = Some(Instant::now());
            } else if state.kept.is_some() {
                state.kept = Some(Instant::now());
            }
            native = state
                .owner
                .is_some_and(|(_, backend)| backend == CaptureBackend::ScreenCaptureKit);
        }
        self.exited()?;
        // Only a picture under a native owner, kept or a viewer's, may open
        // the native stream: a CoreGraphics owner's source stays CoreGraphics.
        Ok(if native {
            super::stream::opening(capture)
        } else {
            capture()
        })
    }

    /// Run `capture`, one frame of the live source's owner, holding the
    /// fence a one-shot capture holds, so [`Source::between_captures`]
    /// waits for it to end. This blocks while that runs.
    ///
    /// # Errors
    ///
    /// Returns busy while the native source's stop is unconfirmed, or while
    /// a process that serves captures has been told to end and is not yet
    /// gone.
    pub fn frame<T>(&self, capture: impl FnOnce() -> T) -> Result<T> {
        let _capturing = self.fence.read().unwrap_or_else(PoisonError::into_inner);
        if matches!(
            self.state().lifecycle,
            Lifecycle::Quarantined | Lifecycle::Stopping
        ) {
            return Err(quarantined());
        }
        self.exited()?;
        Ok(capture())
    }

    /// Keep every capture out until `instance`, a process that serves them
    /// and was told to end, is gone, so none runs while it may still exit.
    /// Called between captures, where none can be cut.
    pub fn until_gone(&self, instance: Instance) {
        self.state().exiting = Some(instance);
    }

    /// Ok once no process told to end may still serve a capture.
    fn exited(&self) -> Result<()> {
        let mut state = self.state();
        match state.exiting {
            Some(instance) if !instance.ended() => Err(restarting()),
            Some(_) => {
                state.exiting = None;
                Ok(())
            }
            None => Ok(()),
        }
    }

    /// Run `quiet` once no capture runs, with none able to begin until it
    /// returns. A CoreGraphics owner may stay: each of its frames is a
    /// capture of its own, and the next one waits. A native owner keeps its
    /// stream open between frames, so that stream is stopped and drained
    /// first; the owner stays, and its next frame opens a new stream, which
    /// has no picture from before. When that stop is not confirmed, `quiet`
    /// does not run and the source stays stopping: nothing captures until
    /// the owner's own stop settles it. A native source kept for pictures
    /// has no owner to do that, so it is given back instead: stopped and
    /// settled like any stop, quarantined when unconfirmed, and the next
    /// picture keeps a new one. Nor does `quiet` run while an
    /// earlier native source is stopping or unconfirmed, or when captures
    /// still run after `within`, so a capture that never ends holds nothing
    /// else up. None says `quiet` did not run. This blocks for up to
    /// `within`, and then for the native stop.
    pub fn between_captures<T>(&self, within: Duration, quiet: impl FnOnce() -> T) -> Option<T> {
        let deadline = Instant::now() + within;
        let _fence = loop {
            match self.fence.try_write() {
                Ok(fence) => break fence,
                Err(TryLockError::Poisoned(poisoned)) => break poisoned.into_inner(),
                Err(TryLockError::WouldBlock) if Instant::now() < deadline => {
                    std::thread::sleep(FENCE_CHECK);
                }
                Err(TryLockError::WouldBlock) => return None,
            }
        };
        let (native, kept) = {
            let mut state = self.state();
            match (state.lifecycle, state.owner) {
                (Lifecycle::Stopped, _) => (false, None),
                (Lifecycle::Active, Some((generation, backend))) => {
                    let native = backend == CaptureBackend::ScreenCaptureKit;
                    if native {
                        state.lifecycle = Lifecycle::Stopping;
                    }
                    (native, state.kept.take().map(|_| generation))
                }
                _ => return None,
            }
        };
        if let Some(generation) = kept {
            let stopped = (self.stop)();
            self.settle(generation, stopped.is_ok(), None);
            stopped.ok()?;
        } else if native {
            (self.stop)().ok()?;
            self.state().lifecycle = Lifecycle::Active;
        }
        Some(quiet())
    }

    /// Give the source back. A native source is stopped and drained first;
    /// `stop_request` names the request that asked, for later drain queries.
    /// Without a confirmed stop the source stays quarantined. This blocks.
    ///
    /// # Errors
    ///
    /// Returns the native stop's failure; the source is then quarantined.
    pub fn stop(&self, owner: Owner, stop_request: Option<u64>) -> Result<()> {
        let stopped = match owner.backend {
            CaptureBackend::CoreGraphics => Ok(()),
            CaptureBackend::ScreenCaptureKit => {
                let _fence = self.fence.write().unwrap_or_else(PoisonError::into_inner);
                self.state().lifecycle = Lifecycle::Stopping;
                (self.stop)()
            }
        };
        self.settle(owner.generation, stopped.is_ok(), stop_request);
        stopped
    }

    /// Stop the native source the source keeps for pictures once none was
    /// taken for `idle`. Nothing else is touched: a viewer's source, or one
    /// a viewer took over meanwhile, stays. This blocks while a capture runs,
    /// and then for the stop.
    ///
    /// # Errors
    ///
    /// Returns the native stop's failure; the source is then quarantined.
    pub fn release_kept(&self, idle: Duration) -> Result<()> {
        let _fence = self.fence.write().unwrap_or_else(PoisonError::into_inner);
        let generation = {
            let mut state = self.state();
            match (state.kept, state.owner, state.lifecycle) {
                (Some(used), Some((generation, _)), Lifecycle::Active)
                    if used.elapsed() >= idle =>
                {
                    state.kept = None;
                    state.lifecycle = Lifecycle::Stopping;
                    generation
                }
                _ => return Ok(()),
            }
        };
        let stopped = (self.stop)();
        self.settle(generation, stopped.is_ok(), None);
        stopped
    }

    /// Ask a quarantined source to stop again, which blocks; anything else
    /// is left as it is.
    fn confirm(&self) -> Result<()> {
        match self.state().lifecycle {
            Lifecycle::Quarantined => {}
            Lifecycle::Stopping => return Err(quarantined()),
            _ => return Ok(()),
        }
        let _fence = self.fence.write().unwrap_or_else(PoisonError::into_inner);
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

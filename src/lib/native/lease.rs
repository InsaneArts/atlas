//! Ownership is independent of the event engine, so cancellation never queues
//! behind a drag, text insertion or wait. Posting holds this gate briefly.
use crate::error::{Error, Result};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

const TTL: Duration = Duration::from_secs(6);
#[derive(Default)]
pub struct Gate(Mutex<State>);
#[derive(Default)]
pub struct State {
    owner: Option<String>,
    previous: Option<String>,
    expires: Option<Instant>,
    closed: bool,
}
impl State {
    fn expire(&mut self) {
        if self.expires.is_some_and(|at| at <= Instant::now()) {
            self.owner = None;
        }
    }
    fn require(&mut self, owner: &str) -> Result<()> {
        self.expire();
        if self.closed || self.owner.as_deref() != Some(owner) {
            return Err(Error::new(
                "lease-lost",
                "Native control was released or expired",
            ));
        }
        Ok(())
    }
}
impl Gate {
    fn lock(&self) -> Result<MutexGuard<'_, State>> {
        self.0
            .lock()
            .map_err(|_| Error::new("native", "Input ownership failed"))
    }
    pub fn active(&self) -> Result<bool> {
        let mut state = self.lock()?;
        state.expire();
        Ok(state.owner.is_some())
    }
    pub fn acquire(self: &Arc<Self>, owner: &str) -> Result<Grant> {
        let mut state = self.lock()?;
        state.expire();
        if state.closed || state.owner.is_some() {
            return Err(Error::new(
                "busy",
                "The native input service is unavailable or already controlled",
            ));
        }
        if owner.len() != 48 || !owner.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(Error::new("input", "Invalid native owner token"));
        }
        state.owner = Some(owner.to_owned());
        state.previous = state.owner.clone();
        state.expires = Some(Instant::now() + TTL);
        Ok(Grant {
            gate: self.clone(),
            owner: owner.to_owned(),
        })
    }
    pub fn grant(self: &Arc<Self>, owner: &str) -> Result<Grant> {
        self.lock()?.require(owner)?;
        Ok(Grant {
            gate: self.clone(),
            owner: owner.to_owned(),
        })
    }
    pub fn renew(&self, owner: &str) -> Result<()> {
        let mut state = self.lock()?;
        state.require(owner)?;
        state.expires = Some(Instant::now() + TTL);
        Ok(())
    }
    pub fn cancel(&self, owner: Option<&str>) -> Result<()> {
        let mut state = self.lock()?;
        if let Some(owner) = owner
            && state.previous.as_deref() != Some(owner)
        {
            return Err(Error::new(
                "lease-lost",
                "This token cannot release another controller",
            ));
        }
        state.owner = None;
        Ok(())
    }
    pub fn close(&self) -> Result<()> {
        let mut state = self.lock()?;
        state.closed = true;
        state.owner = None;
        Ok(())
    }
}
#[derive(Clone)]
pub struct Grant {
    gate: Arc<Gate>,
    owner: String,
}
impl Grant {
    pub fn authorize(&self) -> Result<MutexGuard<'_, State>> {
        let mut state = self.gate.lock()?;
        state.require(&self.owner)?;
        Ok(state)
    }
}

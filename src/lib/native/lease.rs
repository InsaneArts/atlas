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
    /// The last ownership ended by a cancel (the person's or the
    /// controller's doing), not by expiry or the gate closing.
    cancelled: bool,
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
            if self.cancelled && !self.closed && self.owner.is_none() {
                return Err(Error::new("cancelled", "Native control was stopped"));
            }

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
        // Owners are `control::token()`s: two hexadecimal digits a byte.
        if owner.len() != 2 * crate::control::TOKEN_BYTES
            || !owner.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err(Error::new("input", "Invalid native owner token"));
        }
        state.owner = Some(owner.to_owned());
        state.previous = state.owner.clone();
        state.expires = Some(Instant::now() + TTL);
        state.cancelled = false;
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

    /// End ownership on purpose: the person stopped, or the controller
    /// released it. What was under way then reads as cancelled.
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

        state.cancelled |= state.owner.is_some();
        state.owner = None;
        Ok(())
    }

    /// End ownership for a cause that is nobody's doing (the session
    /// changed, an action failed). What was under way reads as lost.
    pub fn revoke(&self) -> Result<()> {
        let mut state = self.lock()?;
        state.owner = None;
        state.cancelled = false;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn gate() -> Arc<Gate> {
        Arc::new(Gate::default())
    }

    fn owner(digit: char) -> String {
        digit.to_string().repeat(2 * crate::control::TOKEN_BYTES)
    }

    fn code<T>(result: Result<T>) -> &'static str {
        result.err().map_or("ok", |error| error.code)
    }

    #[test]
    fn a_second_controller_is_busy() {
        let gate = gate();
        let _grant = gate.acquire(&owner('a')).unwrap();
        assert!(gate.active().unwrap());
        assert_eq!(code(gate.acquire(&owner('b'))), "busy");
        assert_eq!(code(gate.acquire(&owner('a'))), "busy", "even the owner");
    }

    #[test]
    fn owners_are_control_tokens() {
        let gate = gate();
        assert_eq!(owner('a').len(), crate::control::token().len());
        for bad in [
            String::new(),
            "a".repeat(47),
            "a".repeat(49),
            "g".repeat(48),
        ] {
            assert_eq!(code(gate.acquire(&bad)), "input", "{bad:?}");
        }
        assert!(!gate.active().unwrap());
        assert_eq!(code(gate.acquire(&crate::control::token())), "ok");
    }

    #[test]
    fn ownership_expires_unless_renewed() {
        let gate = gate();
        let grant = gate.acquire(&owner('a')).unwrap();
        gate.renew(&owner('a')).unwrap();
        assert_eq!(code(gate.renew(&owner('b'))), "lease-lost");
        gate.lock().unwrap().expires = Some(Instant::now());
        assert!(!gate.active().unwrap());
        assert_eq!(code(gate.renew(&owner('a'))), "lease-lost");
        assert_eq!(code(gate.grant(&owner('a'))), "lease-lost");
        assert_eq!(code(grant.authorize()), "lease-lost");
        assert_eq!(code(gate.acquire(&owner('b'))), "ok", "free again");
    }

    #[test]
    fn a_stale_token_cannot_end_a_newer_controller() {
        let gate = gate();
        let _first = gate.acquire(&owner('a')).unwrap();
        gate.cancel(Some(&owner('a'))).unwrap();
        let _second = gate.acquire(&owner('b')).unwrap();
        assert_eq!(code(gate.cancel(Some(&owner('a')))), "lease-lost");
        assert!(gate.active().unwrap());
        gate.grant(&owner('b')).unwrap();
        gate.cancel(Some(&owner('b'))).unwrap();
        assert!(!gate.active().unwrap());
    }

    #[test]
    fn nothing_is_granted_after_a_cancel() {
        let gate = gate();
        let _acquired = gate.acquire(&owner('a')).unwrap();
        let grant = gate.grant(&owner('a')).unwrap();
        drop(grant.authorize().unwrap());
        gate.cancel(None).unwrap();
        assert_eq!(code(grant.authorize()), "cancelled");
        assert_eq!(code(gate.grant(&owner('a'))), "cancelled");
        let _again = gate.acquire(&owner('a')).unwrap();
        gate.revoke().unwrap();
        assert_eq!(
            code(gate.grant(&owner('a'))),
            "lease-lost",
            "nobody stopped it"
        );
    }

    #[test]
    fn a_closed_gate_stays_closed() {
        let gate = gate();
        let grant = gate.acquire(&owner('a')).unwrap();
        gate.close().unwrap();
        assert!(!gate.active().unwrap());
        assert_eq!(code(grant.authorize()), "lease-lost");
        assert_eq!(code(gate.acquire(&owner('b'))), "busy");
        gate.cancel(None).unwrap();
        assert_eq!(code(gate.acquire(&owner('b'))), "busy", "for good");
    }
}

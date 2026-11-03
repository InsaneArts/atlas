//! A test engine's stand-in for the session checks, the Dock and the session
//! event tap. It records each journal write, check and event in order; the
//! journal itself stays real.
use super::journal::Held;
use atlas_core::{actions::Button, error::Result};
use objc2_core_graphics::{CGEvent, CGEventField};
use std::sync::{Arc, Mutex};

/// What a tapped engine did, in order.
#[derive(Clone, Debug, PartialEq)]
pub(in crate::native) enum Seen {
    /// A held state the journal holds, once written.
    Write {
        button: Option<Button>,
        keys: Vec<u16>,
        flags: u64,
    },
    /// A session and permission check passed.
    Check,
    Post(Posted),
}

/// An event as it would have reached the session event tap.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::native) struct Posted {
    pub(in crate::native) kind: u32,
    pub(in crate::native) x: f64,
    pub(in crate::native) y: f64,
    pub(in crate::native) dx: i64,
    pub(in crate::native) dy: i64,
    pub(in crate::native) number: i64,
    pub(in crate::native) key: i64,
    pub(in crate::native) flags: u64,
}

/// Runs before each journal write; an error fails that write.
pub(in crate::native) type Writing = Box<dyn FnMut(&Held) -> Result<()> + Send>;

pub(in crate::native) struct Tap {
    seen: Arc<Mutex<Vec<Seen>>>,
    writing: Option<Writing>,
}

impl Tap {
    /// A tap and the log it keeps.
    pub(in crate::native) fn new() -> (Self, Arc<Mutex<Vec<Seen>>>) {
        let seen = Arc::new(Mutex::new(Vec::new()));
        (
            Self {
                seen: seen.clone(),
                writing: None,
            },
            seen,
        )
    }

    pub(in crate::native) fn writing(mut self, hook: Writing) -> Self {
        self.writing = Some(hook);
        self
    }

    fn record(&self, seen: Seen) {
        self.seen.lock().unwrap().push(seen);
    }

    pub(super) fn check(&self) {
        self.record(Seen::Check);
    }

    pub(super) fn before_write(&mut self, held: &Held) -> Result<()> {
        self.writing.as_mut().map_or(Ok(()), |hook| hook(held))
    }

    pub(super) fn written(&self, held: &Held) {
        self.record(Seen::Write {
            button: held.button,
            keys: held.keys.clone(),
            flags: held.flags,
        });
    }

    pub(super) fn post(&self, event: &CGEvent) {
        let field = |field| CGEvent::integer_value_field(Some(event), field);
        let at = CGEvent::location(Some(event));
        self.record(Seen::Post(Posted {
            kind: CGEvent::r#type(Some(event)).0,
            x: at.x,
            y: at.y,
            dx: field(CGEventField::MouseEventDeltaX),
            dy: field(CGEventField::MouseEventDeltaY),
            number: field(CGEventField::MouseEventNumber),
            key: field(CGEventField::KeyboardEventKeycode),
            flags: CGEvent::flags(Some(event)).0,
        }));
    }
}

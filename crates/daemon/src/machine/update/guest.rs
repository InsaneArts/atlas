//! What reconciliation asks of this guest itself: the installer on its
//! socket, the code this daemon runs, how its input stands, and one
//! capture of its main display.
use super::reconcile::{Health, System};
use crate::machine::input::{Input, Standing};
use atlas_core::error::{Code, Result};
use serde_json::Value;
use std::{path::Path, sync::Arc};

/// The guest as it is, its input service `input` when it has one.
pub(crate) struct Guest {
    pub(crate) input: Option<Arc<Input>>,
    /// Set once the daemon serves its station.
    pub(crate) serving: Arc<std::sync::atomic::AtomicBool>,
}

impl System for Guest {
    fn ask(&self, request: &Value) -> Result<Value> {
        super::installer::ask(Path::new(super::installer::SOCKET), request)
    }

    fn own_code(&self) -> Option<Vec<u8>> {
        let pid = i32::try_from(std::process::id()).ok()?;
        let token = atlas_native::native::process_token::Token::of(pid).ok()?;
        atlas_native::peer::running_code(token.words())
    }

    fn input(&self) -> (bool, Health) {
        let Some(input) = &self.input else {
            return (true, Health::Other);
        };
        let (standing, failure) = input.snapshot();
        // Ready with nothing unacknowledged; or no service alive at all,
        // the earlier one seen gone and its journal clear, as Gone says.
        let quiescent = match standing {
            Standing::Ready => !input.unsettled(),
            Standing::Gone => true,
            Standing::Starting | Standing::Stopping | Standing::CleanupUnknown => false,
        };
        let health = match (standing, failure) {
            (Standing::Ready, _) => Health::Ready,
            (_, Some(failure)) if failure.code == "permission" => Health::Permission,
            _ => Health::Other,
        };
        (quiescent, health)
    }

    fn serving(&self) -> bool {
        self.serving.load(std::sync::atomic::Ordering::SeqCst)
    }

    fn capture(&self) -> Health {
        let display = objc2_core_graphics::CGMainDisplayID();
        match atlas_native::native::capture::main_display(display, None, Some(64)) {
            Ok(_) => Health::Ready,
            Err(error) if error.code == Code::Permission => Health::Permission,
            Err(_) => Health::Other,
        }
    }
}

//! How long macOS says the person has been away, read three ways, and the
//! secure typing that decides how the one apps read is kept.
//!
//! The window server keeps one record of the last input for every login on
//! the Mac, and apps read it as the person's idle time: password managers
//! lock on it, chat apps mark the person away. While an app in the login on
//! the screen has secure typing on, as a password field does, the window
//! server stamps input into a private copy and copies it into that record
//! on a short timer, tied to a login. A switch between logins can lose that
//! timer, and the record then stops until secure typing next turns off in
//! the login on the screen. Each login also keeps its own idle time, and
//! the HID system keeps the hardware's; neither goes through that copy.
use super::cf;
use objc2_core_foundation::{CFDictionary, CFNumber, CFRetained, CFString};
use objc2_core_graphics::{CGEventSource, CGEventSourceStateID, CGEventType};
use objc2_io_kit::{
    IOObjectRelease, IORegistryEntryCreateCFProperty, IOServiceGetMatchingService,
    IOServiceMatching, kIOMainPortDefault,
};
use std::sync::{Mutex, PoisonError};

/// Every kind of input, as `kCGAnyInputEventType`.
const ANY_INPUT: CGEventType = CGEventType(u32::MAX);

/// How long ago macOS last saw input, in seconds, each way it keeps it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Idle {
    /// In the record every login's apps read as the person's idle time.
    pub shared: f64,
    /// In this login's own record.
    pub login: f64,
    /// In the HID system's, which counts the hardware alone; `None` when it
    /// does not say.
    pub hardware: Option<f64>,
}

impl Idle {
    /// As macOS keeps them now. Each is one read of memory the window
    /// server or the kernel keeps current, so it does not wait.
    pub fn now() -> Self {
        Self {
            shared: CGEventSource::seconds_since_last_event_type(
                CGEventSourceStateID::HIDSystemState,
                ANY_INPUT,
            ),
            login: CGEventSource::seconds_since_last_event_type(
                CGEventSourceStateID::CombinedSessionState,
                ANY_INPUT,
            ),
            hardware: hardware_idle(),
        }
    }
}

/// Whether secure typing is on in this login, as the window server records
/// it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SecureTyping {
    /// No process keeps it on.
    Off,
    /// A process keeps it on: by its process ID, the app responsible for it.
    HeldBy(i64),
}

impl SecureTyping {
    /// As the window server records it now, or `None` when this login's
    /// record cannot be read.
    pub fn now() -> Option<Self> {
        let session = cf::session()?;
        let holder = cf::integer(&session, "kCGSSessionSecureInputPID").filter(|pid| *pid > 0);
        Some(holder.map_or(Self::Off, Self::HeldBy))
    }
}

/// Held while this process turns secure typing on and off: Carbon's calls
/// are not safe to make from two threads at once.
static RESETTING: Mutex<()> = Mutex::new(());

/// Turn secure typing on and off once for this process. While no other
/// process in this login keeps it on, turning it off makes the window
/// server copy its private stamps back into the record apps read and stamp
/// that record directly again, which restarts a record that had stopped.
/// While another process keeps it on, it changes nothing.
#[allow(
    unsafe_code,
    reason = "FFI: Carbon's secure event input calls have no safe binding"
)]
pub fn reset_secure_typing() {
    #[link(name = "Carbon", kind = "framework")]
    unsafe extern "C" {
        fn EnableSecureEventInput() -> i32;
        fn DisableSecureEventInput() -> i32;
    }
    let _resetting = RESETTING.lock().unwrap_or_else(PoisonError::into_inner);
    // SAFETY: neither call takes an argument, and the lock above keeps every
    // other thread of this process from making them meanwhile. They count
    // this process's requests, so the pair leaves its count where it was,
    // and the window server drops a dead process's requests.
    unsafe {
        // Best effort: a refused call leaves the record as it was, and the
        // next look sees that.
        let _ = EnableSecureEventInput();
        let _ = DisableSecureEventInput();
    }
}

/// Seconds since the hardware last gave input, from the HID system's
/// `HIDIdleTime`, or `None` when it cannot be read.
#[allow(
    unsafe_code,
    reason = "FFI: IOKit's service lookup and property read have no safe binding"
)]
fn hardware_idle() -> Option<f64> {
    // SAFETY: the class name is a valid C string for the call.
    let matching = unsafe { IOServiceMatching(c"IOHIDSystem".as_ptr()) }?;
    // SAFETY: a mutable dictionary is a dictionary. The main port names the
    // default IOKit port, and the lookup consumes the dictionary it is given.
    let service = unsafe {
        IOServiceGetMatchingService(
            kIOMainPortDefault,
            Some(CFRetained::cast_unchecked::<CFDictionary>(matching)),
        )
    };
    if service == 0 {
        return None;
    }
    let key = CFString::from_static_str("HIDIdleTime");
    // SAFETY: the service was just looked up and is released below; the key
    // is a CF string and the default allocator is asked for with `None`.
    let value = unsafe { IORegistryEntryCreateCFProperty(service, Some(&key), None, 0) };
    // Best effort: releasing a service the lookup returned cannot fail in a
    // way that changes what was read.
    let _ = IOObjectRelease(service);
    let nanoseconds = value?.downcast_ref::<CFNumber>()?.as_i64()?;
    #[allow(
        clippy::cast_precision_loss,
        reason = "idle time in nanoseconds fits a double's precision for any idle time"
    )]
    let seconds = nanoseconds as f64 / 1e9;
    Some(seconds)
}

#[cfg(test)]
#[path = "idle_tests.rs"]
mod tests;

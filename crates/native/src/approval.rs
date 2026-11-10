//! One administrator approval in macOS's own dialog, for the window's steps
//! that need root once: creating the helper account and readying it for its
//! first login. The approved program runs as root with a private socket as
//! its standard input and output, so a password reaches it without a file or
//! a command line.
//!
//! The call that starts it, `AuthorizationExecuteWithPrivileges`, is
//! deprecated but still part of macOS; it is looked up when used, so a
//! macOS without it refuses here rather than keeping Atlas from launching.
#![allow(
    unsafe_code,
    reason = "FFI: Security.framework's Authorization calls have no safe binding"
)]

use objc2_security::{
    AuthorizationCopyRights, AuthorizationCreate, AuthorizationFlags, AuthorizationFree,
    AuthorizationItem, AuthorizationItemSet, AuthorizationRef, errAuthorizationCanceled,
};
use std::{
    ffi::{CString, c_char, c_void},
    os::{
        fd::{FromRawFd, OwnedFd},
        unix::{ffi::OsStrExt, net::UnixStream},
    },
    path::Path,
    ptr::{NonNull, null, null_mut},
};

/// Why a step under the approval did not start, or did not finish.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// The person cancelled macOS's dialog.
    Cancelled,
    /// This macOS has no call that starts it; authorization was never requested.
    Unavailable,
    /// It failed for another reason, said in one line.
    Failed(String),
}

/// `AuthorizationExecuteWithPrivileges`, as found in Security.framework.
type Execute = unsafe extern "C" fn(
    AuthorizationRef,
    *const c_char,
    AuthorizationFlags,
    *const *mut c_char,
    *mut *mut libc::FILE,
) -> i32;

/// An authorization, its rights given up when dropped.
struct Authorization(AuthorizationRef);

impl Authorization {
    fn new() -> Result<Self, Refusal> {
        let mut reference: AuthorizationRef = null();
        // SAFETY: no rights or environment yet; `reference` is writable.
        let status = unsafe {
            AuthorizationCreate(null(), null(), AuthorizationFlags::Defaults, &mut reference)
        };
        if status != 0 || reference.is_null() {
            return Err(Refusal::Failed(format!(
                "macOS could not start an approval ({status})"
            )));
        }
        Ok(Self(reference))
    }

    fn approve(&self, prompt: &CString) -> Result<(), Refusal> {
        let mut right = AuthorizationItem {
            name: c"system.privilege.admin".as_ptr(),
            valueLength: 0,
            value: null_mut(),
            flags: 0,
        };
        let mut rights = AuthorizationItemSet {
            count: 1,
            items: &mut right,
        };
        let mut message = AuthorizationItem {
            name: c"prompt".as_ptr(),
            valueLength: prompt.as_bytes().len(),
            value: prompt.as_ptr().cast_mut().cast(),
            flags: 0,
        };
        let environment = AuthorizationItemSet {
            count: 1,
            items: &mut message,
        };
        let flags = AuthorizationFlags::InteractionAllowed
            | AuthorizationFlags::ExtendRights
            | AuthorizationFlags::PreAuthorize;
        // SAFETY: the right, the prompt and the sets pointing at them all outlive
        // the call; no authorized rights are asked back.
        let status = unsafe {
            AuthorizationCopyRights(
                self.0,
                NonNull::from(&mut rights),
                &environment,
                flags,
                null_mut(),
            )
        };
        match status {
            0 => Ok(()),
            cancelled if cancelled == errAuthorizationCanceled => Err(Refusal::Cancelled),
            other => Err(Refusal::Failed(format!(
                "macOS did not give the approval ({other})"
            ))),
        }
    }

    fn run(
        &self,
        execute: Execute,
        program: &CString,
        argv: &[*mut c_char],
    ) -> Result<UnixStream, Refusal> {
        let mut pipe: *mut libc::FILE = null_mut();
        // SAFETY: program and the NULL-terminated argv outlive the call;
        // pipe is writable and self holds the approval just given.
        let status = unsafe {
            execute(
                self.0,
                program.as_ptr(),
                AuthorizationFlags::Defaults,
                argv.as_ptr(),
                &mut pipe,
            )
        };
        execution_started(status, NonNull::new(pipe).map(Pipe))?.stream()
    }
}

impl Drop for Authorization {
    fn drop(&mut self) {
        // SAFETY: the reference came from AuthorizationCreate and is freed once.
        unsafe {
            AuthorizationFree(self.0, AuthorizationFlags::DestroyRights);
        }
    }
}

/// The deprecated call, when this macOS still has it.
fn execute() -> Option<Execute> {
    // SAFETY: a NUL-terminated name looked up among the loaded images, where
    // objc2-security has linked Security.framework.
    let symbol = unsafe {
        libc::dlsym(
            libc::RTLD_DEFAULT,
            c"AuthorizationExecuteWithPrivileges".as_ptr(),
        )
    };
    // SAFETY: the symbol is that function, whose C signature `Execute` matches.
    (!symbol.is_null()).then(|| unsafe { std::mem::transmute::<*mut c_void, Execute>(symbol) })
}

#[cfg(any(test, feature = "test-support"))]
thread_local! {
    static AUTHORIZATION_CALLS: std::cell::Cell<Option<usize>> = const { std::cell::Cell::new(None) };
}

/// Count and refuse authorization in a test without opening a macOS dialog.
///
/// # Panics
///
/// If `act` ends the count itself.
#[cfg(any(test, feature = "test-support"))]
pub fn without_authorization<T>(act: impl FnOnce() -> T) -> (T, usize) {
    AUTHORIZATION_CALLS.with(|calls| calls.set(Some(0)));
    let result = act();
    let count = AUTHORIZATION_CALLS.with(|calls| calls.replace(None).unwrap());
    (result, count)
}

/// Ask macOS for one administrator approval, `prompt` shown in its dialog,
/// then run `program` with `arguments` as root. Gives the socket that is the
/// approved program's standard input and output.
///
/// # Errors
///
/// Returns a [`Refusal`] when approval is unavailable or denied, or the program
/// can't start.
pub fn run(program: &Path, arguments: &[&str], prompt: &str) -> Result<UnixStream, Refusal> {
    #[cfg(any(test, feature = "test-support"))]
    if AUTHORIZATION_CALLS.with(|calls| {
        calls.get().is_some_and(|count| {
            calls.set(Some(count + 1));
            true
        })
    }) {
        return Err(Refusal::Unavailable);
    }
    let execute = execute().ok_or(Refusal::Unavailable)?;
    let text = |value: &[u8]| {
        CString::new(value).map_err(|_| Refusal::Failed("An argument held a NUL byte".into()))
    };
    let program = text(program.as_os_str().as_bytes())?;
    let arguments = arguments
        .iter()
        .map(|argument| text(argument.as_bytes()))
        .collect::<Result<Vec<_>, _>>()?;
    let argv: Vec<*mut c_char> = arguments
        .iter()
        .map(|argument| argument.as_ptr().cast_mut())
        .chain(std::iter::once(null_mut()))
        .collect();
    let prompt = text(prompt.as_bytes())?;

    let authorization = Authorization::new()?;
    authorization.approve(&prompt)?;
    authorization.run(execute, &program, &argv)
}

/// The stream macOS returned, closed once even when execution failed.
struct Pipe(NonNull<libc::FILE>);

impl Pipe {
    fn stream(&self) -> Result<UnixStream, Refusal> {
        // SAFETY: self owns an open stream. Copy its descriptor with close-on-exec
        // before Drop closes the original, so no later child inherits the channel.
        let descriptor =
            unsafe { libc::fcntl(libc::fileno(self.0.as_ptr()), libc::F_DUPFD_CLOEXEC, 0) };
        if descriptor < 0 {
            return Err(Refusal::Failed(
                "Atlas could not talk to the approved step".into(),
            ));
        }
        // SAFETY: fcntl returned a fresh descriptor nothing else owns.
        Ok(UnixStream::from(unsafe {
            OwnedFd::from_raw_fd(descriptor)
        }))
    }
}

impl Drop for Pipe {
    fn drop(&mut self) {
        // SAFETY: this is the stream returned by macOS, owned and closed once.
        unsafe { libc::fclose(self.0.as_ptr()) };
    }
}

/// `pipe`, the channel to the step macOS started. Once approval was requested,
/// failure must stop the attempt instead of offering another authorization
/// mechanism. A pipe that came with a failure is dropped here.
fn execution_started<T>(status: i32, pipe: Option<T>) -> Result<T, Refusal> {
    pipe.filter(|_| status == 0).ok_or_else(|| {
        Refusal::Failed(format!(
            "macOS could not run the approved step ({status}); no further approval was requested"
        ))
    })
}

#[cfg(test)]
#[path = "approval_tests.rs"]
mod tests;

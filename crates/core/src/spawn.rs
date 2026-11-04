//! Starting child processes and making pipes, one at a time in this process.
//!
//! On macOS a new pipe is made close-on-exec only after it exists, as there
//! is no `pipe2`. A child that another thread starts in between inherits both
//! ends and holds the pipe open while it runs, so a command that finished
//! never reaches the end of its output. Every child and every pipe Atlas makes
//! takes one lock here, held only while the process starts or the pipe is
//! made: never while a child runs or is waited for, and never across an
//! await. A spawn may still take as long as the system needs to start the
//! program, and others wait their turn meanwhile.
//!
//! This guards pipes from Atlas's own spawns. Descriptors made elsewhere
//! without atomic close-on-exec, such as sockets that std or tokio connect or
//! accept and tokio's kqueue, can still be inherited by a child started at
//! that moment. Callers may hold their own locks across these calls; nothing
//! here takes another lock or calls back into the caller.
#![allow(
    clippy::disallowed_methods,
    reason = "the one place Atlas starts a process or makes a pipe"
)]

use std::{
    future::Future,
    io::{self, PipeReader, PipeWriter},
    process::{Child, Command, ExitStatus, Output, Stdio},
    sync::{Mutex, MutexGuard, PoisonError},
};

/// Held while a process starts or a pipe is made. It guards no data, so a
/// thread that panicked holding it leaves nothing to recover.
static ALONE: Mutex<()> = Mutex::new(());

fn alone() -> MutexGuard<'static, ()> {
    ALONE.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Start `command`, as [`Command::spawn`] does, while no other Atlas spawn or
/// pipe is being made.
///
/// # Errors
///
/// Returns the error of starting the program.
pub fn spawn(command: &mut Command) -> io::Result<Child> {
    let _alone = alone();
    command.spawn()
}

/// Run `command` to its end with its standard streams as they are set,
/// inherited unless set otherwise, as [`Command::status`] does.
///
/// # Errors
///
/// Returns the error of starting or waiting for the program.
pub fn status(command: &mut Command) -> io::Result<ExitStatus> {
    spawn(command)?.wait()
}

/// Run `command` to its end with no input, capturing its output and errors,
/// as [`Command::output`] does by default. It sets all three streams, so it
/// is only for a command that sets none of them itself.
///
/// # Errors
///
/// Returns the error of starting the program or reading its output.
pub fn output(command: &mut Command) -> io::Result<Output> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    spawn(command)?.wait_with_output()
}

/// A pipe, as [`std::io::pipe`] makes one, made close-on-exec while no Atlas
/// spawn is under way.
///
/// # Errors
///
/// Returns the error of making the pipe.
pub fn pipe() -> io::Result<(PipeReader, PipeWriter)> {
    let _alone = alone();
    std::io::pipe()
}

/// Start `command`, as [`tokio::process::Command::spawn`] does, while no
/// other Atlas spawn or pipe is being made.
///
/// # Errors
///
/// Returns the error of starting the program.
pub fn spawn_async(command: &mut tokio::process::Command) -> io::Result<tokio::process::Child> {
    let _alone = alone();
    command.spawn()
}

/// [`tokio::process::Command::output`]: the program starts here, while no
/// other Atlas spawn is under way, and the future only waits for it.
pub fn output_async(
    command: &mut tokio::process::Command,
) -> impl Future<Output = io::Result<Output>> + use<> {
    let _alone = alone();
    command.output()
}

/// [`tokio::process::Command::status`]: the program starts here, while no
/// other Atlas spawn is under way, and the future only waits for it.
pub fn status_async(
    command: &mut tokio::process::Command,
) -> impl Future<Output = io::Result<ExitStatus>> + use<> {
    let _alone = alone();
    command.status()
}

#[cfg(test)]
mod tests;

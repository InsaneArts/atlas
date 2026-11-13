//! A daemon started while the helper was on the screen, as at its first
//! login or when the person logs in to it there, gets no display mode from
//! macOS for the display the helper's session has once it leaves the
//! screen, however long it runs. ScreenCaptureKit needs that mode, so every
//! picture would come from CoreGraphics. Such a daemon restarts once the
//! helper has left the screen, and launchd starts a fresh one, which has it.
use std::{future::Future, time::Duration};

/// How often the screen and the display are looked at.
const CHECK: Duration = Duration::from_secs(1);
/// How long the display is given to settle after the helper leaves the
/// screen, before its mode is looked for.
const SETTLE: Duration = Duration::from_secs(2);

/// Restart this process once the helper is off the screen, when it started
/// on it and then has no mode for its display. Waits while commands run,
/// as a restart would end them; background jobs go on regardless.
pub(super) async fn restart_off_screen(
    started_on_screen: bool,
    on_console: fn() -> bool,
    has_mode: fn() -> bool,
    idle: impl AsyncFn() -> bool,
    restart: impl Fn(),
) {
    watch(
        started_on_screen,
        || tokio::time::sleep(CHECK),
        || tokio::time::sleep(SETTLE),
        on_console,
        has_mode,
        idle,
        restart,
    )
    .await;
}

async fn watch<Pause, Settle>(
    started_on_screen: bool,
    mut pause: impl FnMut() -> Pause,
    settle: impl FnOnce() -> Settle,
    on_console: impl Fn() -> bool,
    has_mode: impl Fn() -> bool,
    idle: impl AsyncFn() -> bool,
    restart: impl Fn(),
) where
    Pause: Future<Output = ()>,
    Settle: Future<Output = ()>,
{
    if !started_on_screen {
        return;
    }
    while on_console() {
        pause().await;
    }
    settle().await;
    if has_mode() {
        return;
    }
    loop {
        if !on_console() && idle().await {
            restart();
            return;
        }
        pause().await;
    }
}

#[cfg(test)]
#[path = "display_mode_tests.rs"]
mod tests;

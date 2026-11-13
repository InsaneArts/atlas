use super::*;
use std::cell::Cell;

/// What the screen says each time it is looked at: on it until `on_for`
/// looks have passed.
fn screen(on_for: usize) -> impl Fn() -> bool {
    let looks = Cell::new(0);
    move || {
        looks.set(looks.get() + 1);
        looks.get() <= on_for
    }
}

/// Runs the watch with every wait at once, and says how often it restarted.
async fn restarts(
    started_on_screen: bool,
    on_console: impl Fn() -> bool,
    has_mode: bool,
    idle: impl AsyncFn() -> bool,
) -> usize {
    let restarted = Cell::new(0);
    watch(
        started_on_screen,
        || std::future::ready(()),
        || std::future::ready(()),
        on_console,
        || has_mode,
        idle,
        || restarted.set(restarted.get() + 1),
    )
    .await;
    restarted.get()
}

#[tokio::test]
async fn a_daemon_started_off_the_screen_never_restarts_for_its_display() {
    assert_eq!(restarts(false, screen(0), false, async || true).await, 0);
}

#[tokio::test]
async fn a_daemon_started_on_the_screen_that_has_a_mode_once_off_it_stays() {
    assert_eq!(restarts(true, screen(3), true, async || true).await, 0);
}

#[tokio::test]
async fn a_daemon_started_on_the_screen_with_no_mode_once_off_it_restarts_once() {
    assert_eq!(restarts(true, screen(3), false, async || true).await, 1);
}

#[tokio::test]
async fn a_restart_waits_until_no_command_runs() {
    let asked = Cell::new(0);
    let idle = async || {
        asked.set(asked.get() + 1);
        asked.get() > 2
    };
    assert_eq!(restarts(true, screen(1), false, idle).await, 1);
    assert_eq!(asked.get(), 3, "restarted while a command ran");
}

#[tokio::test]
async fn a_restart_waits_while_the_helper_is_back_on_the_screen() {
    // Off the screen at the third look, back on it for the fourth and fifth.
    let looks = Cell::new(0);
    let on_console = || {
        looks.set(looks.get() + 1);
        matches!(looks.get(), 1 | 2 | 4 | 5)
    };
    assert_eq!(restarts(true, on_console, false, async || true).await, 1);
    assert_eq!(
        looks.get(),
        6,
        "restarted while the helper was on the screen"
    );
}

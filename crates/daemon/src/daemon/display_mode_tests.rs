use super::*;
use std::cell::Cell;

/// What the screen says at each look: on it at the looks `on` names.
fn screen(on: &'static [usize]) -> impl Fn() -> bool {
    let looks = Cell::new(0);
    move || {
        looks.set(looks.get() + 1);
        on.contains(&looks.get())
    }
}

/// Runs the watch with every wait at once, and says how often it restarted.
async fn restarts(
    started_on_screen: bool,
    on_console: impl Fn() -> bool,
    has_mode: impl Fn() -> bool,
    close: impl AsyncFn() -> bool,
) -> usize {
    let restarted = Cell::new(0);
    watch(
        started_on_screen,
        || std::future::ready(()),
        || std::future::ready(()),
        on_console,
        has_mode,
        close,
        || restarted.set(restarted.get() + 1),
    )
    .await;
    restarted.get()
}

#[tokio::test]
async fn a_daemon_started_off_the_screen_never_restarts_for_its_display() {
    assert_eq!(
        restarts(false, screen(&[]), || false, async || true).await,
        0
    );
}

#[tokio::test]
async fn a_daemon_started_on_the_screen_that_has_a_mode_once_off_it_stays() {
    assert_eq!(
        restarts(true, screen(&[1, 2]), || true, async || true).await,
        0
    );
}

#[tokio::test]
async fn a_daemon_started_on_the_screen_with_no_mode_once_off_it_restarts_once() {
    assert_eq!(
        restarts(true, screen(&[1, 2]), || false, async || true).await,
        1
    );
}

#[tokio::test]
async fn a_restart_waits_until_the_daemon_closes_to_new_commands() {
    let asked = Cell::new(0);
    let close = async || {
        asked.set(asked.get() + 1);
        asked.get() > 2
    };
    assert_eq!(restarts(true, screen(&[1]), || false, close).await, 1);
    assert_eq!(asked.get(), 3, "restarted before the daemon closed");
}

#[tokio::test]
async fn a_display_looked_at_back_on_the_screen_is_not_taken_for_the_one_off_it() {
    // Each wait moves to the next moment: on the screen, off it, back on it
    // during the settle, then off it for good. On it the display has a mode,
    // off it none.
    let on_screen = [true, false, true, false];
    let moment = Cell::new(0);
    let now = || on_screen[moment.get().min(on_screen.len() - 1)];
    let next = || {
        moment.set(moment.get() + 1);
        std::future::ready(())
    };
    let restarted = Cell::new(0);
    watch(
        true,
        next,
        next,
        now,
        now,
        async || true,
        || restarted.set(restarted.get() + 1),
    )
    .await;
    assert_eq!(
        restarted.get(),
        1,
        "the display on the screen was taken for it"
    );
}

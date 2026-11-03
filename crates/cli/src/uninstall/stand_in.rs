//! What uninstall changes beyond a test's own folders, stood in for on the
//! thread of a test that asks: Atlas's one-time start in another account,
//! Open at Login, the agents' plugins, the Trash and the installed settings.
use std::cell::RefCell;

thread_local! {
    static REACHED: RefCell<Option<Vec<&'static str>>> = const { RefCell::new(None) };
}

/// Whether `effect` is stood in for on this thread, noting that it was
/// reached.
pub(super) fn reached(effect: &'static str) -> bool {
    REACHED.with(|reached| {
        reached
            .borrow_mut()
            .as_mut()
            .map(|seen| seen.push(effect))
            .is_some()
    })
}

/// Run `act` with every effect stood in for: what it returned, and the
/// effects it reached, in order.
pub(super) fn standing_in<T>(act: impl FnOnce() -> T) -> (T, Vec<&'static str>) {
    REACHED.with(|reached| *reached.borrow_mut() = Some(Vec::new()));
    let outcome = act();
    let reached = REACHED
        .with(|reached| reached.borrow_mut().take())
        .unwrap_or_default();
    (outcome, reached)
}

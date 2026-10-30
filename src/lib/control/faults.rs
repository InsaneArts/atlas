//! Faults the controller tests set off: each stops one step partway, once,
//! on the thread that set it, as a process killed there would. Only test
//! builds have them.
use std::cell::Cell;

thread_local! {
    /// Set, the next removal of a claim on this thread stops once its claim
    /// file is gone and before its executor record goes.
    static INTERRUPT_CLAIM_REMOVAL: Cell<bool> = const { Cell::new(false) };

    /// Set to the end of a record's name, the next replacement of that
    /// record on this thread stops before its rename.
    static INTERRUPT_REPLACE: Cell<Option<&'static str>> = const { Cell::new(None) };
}

/// Stop the next removal of a claim on this thread once its claim file is
/// gone.
pub(super) fn interrupt_claim_removal() {
    INTERRUPT_CLAIM_REMOVAL.with(|interrupt| interrupt.set(true));
}

/// Whether the removal of a claim under way stops here, clearing the fault.
pub(super) fn claim_removal_stops() -> bool {
    INTERRUPT_CLAIM_REMOVAL.with(|interrupt| interrupt.replace(false))
}

/// Stop the next replacement of the record whose name ends with `name`
/// before its rename.
pub(super) fn interrupt_replacing(name: &'static str) {
    INTERRUPT_REPLACE.with(|interrupt| interrupt.set(Some(name)));
}

/// Whether the replacement of the record called `name` stops before its
/// rename, clearing the fault when it does.
pub(super) fn replacing_stops(name: &str) -> bool {
    INTERRUPT_REPLACE.with(|interrupt| {
        let stops = interrupt.get().is_some_and(|ending| name.ends_with(ending));
        if stops {
            interrupt.set(None);
        }
        stops
    })
}

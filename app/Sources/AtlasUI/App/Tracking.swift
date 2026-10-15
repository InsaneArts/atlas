import Observation

/// Apply `value` now and again each time what it reads changes, for AppKit
/// objects (status item, window) that follow an `@Observable` model.
/// It stops once `value` returns nil, which it does when its owner is gone.
@MainActor
func track<Value>(
    _ value: @escaping @MainActor @Sendable () -> Value?,
    _ apply: @escaping @MainActor @Sendable (Value) -> Void
) {
    let current = withObservationTracking {
        value()
    } onChange: {
        Task { @MainActor in track(value, apply) }
    }
    if let current { apply(current) }
}

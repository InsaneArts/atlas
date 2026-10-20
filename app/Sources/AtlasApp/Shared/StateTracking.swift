import Observation

/// Keeps an AppKit object (a status item, a window) in step with an `@Observable` controller, which SwiftUI views
/// follow by themselves but AppKit objects do not.
@MainActor
enum StateTracking {
  /// Applies `value` now and again each time what it reads changes. It stops once `value` returns nil, which it does
  /// when its owner is gone.
  ///
  /// - Parameters:
  ///   - value: Reads the controller's state; nil once its owner is gone.
  ///   - apply: Applies the value to the AppKit object.
  static func follow<Value>(
    _ value: @escaping @MainActor @Sendable () -> Value?,
    apply: @escaping @MainActor @Sendable (Value) -> Void
  ) {
    let current = withObservationTracking {
      value()
    } onChange: {
      Task { @MainActor in follow(value, apply: apply) }
    }
    if let current { apply(current) }
  }
}

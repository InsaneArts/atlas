import Observation

/// A value a test changes, which observation sees.
@MainActor
@Observable
final class ObservedBox {
  /// The value.
  var value = 0
}

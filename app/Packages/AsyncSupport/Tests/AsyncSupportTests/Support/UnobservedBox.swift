/// A value a test changes, which observation cannot see.
@MainActor
final class UnobservedBox {
  /// The value.
  var value = 0
}

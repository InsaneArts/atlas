/// Whether what a viewer's window draws was told to change, as its view follows the state it draws.
nonisolated final class Presented: @unchecked Sendable {
  /// Whether it was.
  var again = false
}

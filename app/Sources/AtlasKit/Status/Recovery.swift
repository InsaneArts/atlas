/// The recovery alert: what it says, and whether recovery is running.
package struct Recovery: Equatable, Sendable {
  /// The alert's title.
  package let title: String
  /// Why recovery is needed.
  package let reason: String
  /// Whether recovery is running.
  package let busy: Bool
}

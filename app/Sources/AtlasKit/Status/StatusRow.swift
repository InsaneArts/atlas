/// One check in the viewer sidebar's Ready card.
package struct StatusRow: Equatable, Sendable, Identifiable {
  /// How a check stands.
  package enum State: Sendable {
    /// It passed.
    case ready
    /// It cannot be observed yet; never counted as failed.
    case unknown
    /// It failed.
    case bad
  }

  /// The check's name.
  package let name: String
  /// How it stands.
  package let state: State
  /// What it says, in words.
  package let text: String
  /// The check's name, which is unique.
  package var id: String { name }
}

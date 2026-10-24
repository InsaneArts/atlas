/// One check in the viewer sidebar's Ready card: what it checks, how it stands and what it found, which the app words.
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

  /// What it checks.
  package let subject: Subject
  /// How it stands.
  package let state: State
  /// What it found.
  package let detail: Detail
  /// What it checks, which is unique.
  package var id: Subject { subject }
}

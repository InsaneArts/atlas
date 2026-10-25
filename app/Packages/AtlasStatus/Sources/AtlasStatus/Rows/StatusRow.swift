/// One check in the viewer sidebar's Ready card: what it checks, how it stands and what it found, which the app words.
public struct StatusRow: Equatable, Sendable, Identifiable {
  /// How a check stands.
  public enum State: Sendable {
    /// It passed.
    case ready
    /// It cannot be observed yet; never counted as failed.
    case unknown
    /// It failed.
    case bad
  }

  /// What it checks.
  public let subject: Subject
  /// How it stands.
  public let state: State
  /// What it found.
  public let detail: Detail
  /// What it checks, which is unique.
  public var id: Subject { subject }
}

/// A screenshot request the scripted service holds until the test lets it answer.
public struct HeldShot {
  /// The screenshot asked for.
  public let name: String
  /// Whether it was asked for as taken, rather than as a thumbnail.
  public let full: Bool
  /// Lets it answer.
  public let resume: CheckedContinuation<Void, Never>
  /// Whether it was let answer already.
  public var answered = false
}

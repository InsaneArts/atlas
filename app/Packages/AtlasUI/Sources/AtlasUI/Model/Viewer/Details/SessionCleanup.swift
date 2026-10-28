/// Cleaning up the chosen host session's work, last in its part of Details: the button that asks first, and once a
/// cleanup ran, what each of its steps did.
public struct SessionCleanup: Equatable, Sendable {
  /// The session's key, which the button's event names.
  public var session: String
  /// The button's title, such as "Clean Up This Session's Work…".
  public var title: String
  /// Where the cleanup stands.
  public var stage: Stage

  /// Creates the cleanup's state.
  ///
  /// - Parameters:
  ///   - session: The session's key.
  ///   - title: The button's title.
  ///   - stage: Where the cleanup stands; ready by default.
  public init(session: String, title: String, stage: Stage = .ready) {
    self.session = session
    self.title = title
    self.stage = stage
  }

  /// Whether the button takes a press: not while a cleanup is read or runs.
  public var isEnabled: Bool { stage != .working }

  /// What each step of the cleanup that ran did, in order; none before one ran, or while one runs.
  public var receipt: [Fact] {
    if case .finished(let steps) = stage { return steps }
    return []
  }
}

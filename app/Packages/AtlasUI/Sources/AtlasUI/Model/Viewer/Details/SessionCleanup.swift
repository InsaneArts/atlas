/// Cleaning up the chosen host session's work, last in its part of Details: the button that asks first, and once a
/// cleanup ran, what each of its steps did.
public struct SessionCleanup: Equatable, Sendable {
  /// The session's key, which the button's event names.
  public var session: String
  /// The button's title, such as "Clean Up This Session's Work…".
  public var title: String
  /// What VoiceOver hears while it works, such as "Cleaning Up…".
  public var workingTitle: String
  /// Where the cleanup stands.
  public var stage: Stage
  /// Whether another session's cleanup is read or runs now, or may still run, so this one waits its turn without a
  /// spinner of its own.
  public var waitsForAnother: Bool

  /// Creates the cleanup's state.
  ///
  /// - Parameters:
  ///   - session: The session's key.
  ///   - title: The button's title.
  ///   - workingTitle: What VoiceOver hears while it works.
  ///   - stage: Where the cleanup stands; ready by default.
  ///   - waitsForAnother: Whether another session's cleanup holds it; not by default.
  public init(
    session: String,
    title: String,
    workingTitle: String,
    stage: Stage = .ready,
    waitsForAnother: Bool = false
  ) {
    self.session = session
    self.title = title
    self.workingTitle = workingTitle
    self.stage = stage
    self.waitsForAnother = waitsForAnother
  }

  /// Whether this session's cleanup is read or runs now, which shows the button's spinner.
  public var isWorking: Bool { stage == .working }

  /// Whether the button takes a press: not while any cleanup is read or runs.
  public var isEnabled: Bool { !isWorking && !waitsForAnother }

  /// What each step of the cleanup that ran did, in order; none before one ran, or while one runs.
  public var receipt: [Fact] {
    if case .finished(let steps) = stage { return steps }
    return []
  }
}

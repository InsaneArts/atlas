/// The end-to-end check, once the person is back.
public struct OnboardingFinishingScreenState: Equatable, Sendable {
  /// The headline.
  public var title: String
  /// Its second line.
  public var tail: String
  /// The first check's line, naming the helper that is logged in.
  public var loggedIn: String
  /// The permissions check's word on the right: Allowed.
  public var permissionsNote: String
  /// How the last check, the live one, stands.
  public var live: CheckState
  /// The live check's word on the right, such as Passed.
  public var liveNote: String
  /// Why the check failed, over Try Again; nil while it runs or once it passed.
  public var failure: String?

  /// Creates the finishing check's state.
  ///
  /// - Parameters:
  ///   - title: The headline.
  ///   - tail: Its second line.
  ///   - loggedIn: The first check's line, naming the helper that is logged in.
  ///   - permissionsNote: The permissions check's word on the right: Allowed.
  ///   - live: How the last check, the live one, stands.
  ///   - liveNote: The live check's word on the right, such as Passed.
  ///   - failure: Why the check failed, over Try Again; nil while it runs or once it passed.
  public init(
    title: String,
    tail: String,
    loggedIn: String,
    permissionsNote: String,
    live: CheckState,
    liveNote: String,
    failure: String? = nil
  ) {
    self.title = title
    self.tail = tail
    self.loggedIn = loggedIn
    self.permissionsNote = permissionsNote
    self.live = live
    self.liveNote = liveNote
    self.failure = failure
  }
}

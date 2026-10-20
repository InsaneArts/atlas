/// The end-to-end check, once the person is back.
package struct OnboardingFinishingScreenState: Equatable, Sendable {
  /// The headline.
  package var title: String
  /// Its second line.
  package var tail: String
  /// The first check's line, naming the helper that is logged in.
  package var loggedIn: String
  /// How the last check, the live one, stands.
  package var live: CheckState
  /// The live check's word on the right, such as "passed".
  package var liveNote: String
  /// Why the check failed, over Try Again; nil while it runs or once it passed.
  package var failure: String?

  /// Creates the finishing check's state.
  ///
  /// - Parameters:
  ///   - title: The headline.
  ///   - tail: Its second line.
  ///   - loggedIn: The first check's line, naming the helper that is logged in.
  ///   - live: How the last check, the live one, stands.
  ///   - liveNote: The live check's word on the right, such as "passed".
  ///   - failure: Why the check failed, over Try Again; nil while it runs or once it passed.
  package init(
    title: String,
    tail: String,
    loggedIn: String,
    live: CheckState,
    liveNote: String,
    failure: String? = nil
  ) {
    self.title = title
    self.tail = tail
    self.loggedIn = loggedIn
    self.live = live
    self.liveNote = liveNote
    self.failure = failure
  }
}

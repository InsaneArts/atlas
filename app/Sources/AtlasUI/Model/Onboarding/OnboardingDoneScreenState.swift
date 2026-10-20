/// The last screen in the person's account.
package struct OnboardingDoneScreenState: Equatable, Sendable {
  /// What to do after a restart, naming the helper.
  package var lede: String
  /// Whether Open Atlas at login is on.
  package var opensAtLogin: Bool

  /// Creates the done screen's state.
  ///
  /// - Parameters:
  ///   - lede: What to do after a restart, naming the helper.
  ///   - opensAtLogin: Whether Open Atlas at login is on.
  package init(lede: String, opensAtLogin: Bool) {
    self.lede = lede
    self.opensAtLogin = opensAtLogin
  }
}

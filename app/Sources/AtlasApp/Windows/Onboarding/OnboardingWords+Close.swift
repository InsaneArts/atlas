/// The close screen's words: a deleted account's login still open, which Atlas closes before setup goes on.
extension OnboardingWords {
  /// The close screen's headline.
  static let leftoverTitle = "A deleted account is still logged in"
  /// What happened and what closing it does.
  static let leftoverLede =
    "macOS kept a login open for an account that was deleted. Close it before you go on, or the next login can stall. macOS asks you to approve once."
  /// The headline once Atlas closed it.
  static let leftoverClosed = "Closed. You can go on."
  /// While Atlas closes it.
  static let closing = "Closing…"
  /// After Choose Another Account, when the helper given up is still logged in.
  static let logOutBeforeDeleting = "Log it out before deleting it."
  /// The first login has run for minutes and Atlas there hasn't answered yet: Atlas keeps waiting.
  static let slowFirstLogin = "This first login is slow. Atlas keeps waiting and never stops a login."
}

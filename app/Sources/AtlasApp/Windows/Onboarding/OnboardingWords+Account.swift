/// The account screens' words: the choice of an account to be the helper, and the form that creates one.
extension OnboardingWords {
  /// The form's tail.
  static let approveOnce = "You'll approve it once in macOS."

  /// A standard account's row detail.
  ///
  /// - Parameter account: Its short name.
  /// - Returns: "testaccount · standard account on this Mac"
  static func candidateDetail(_ account: String) -> String {
    "\(account) · standard account on this Mac"
  }
}

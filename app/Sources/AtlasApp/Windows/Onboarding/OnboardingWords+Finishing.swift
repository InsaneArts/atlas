/// The finishing screen's words: Atlas checking the helper works, and what failed when it does not.
extension OnboardingWords {
  /// The headline while the check runs or failed.
  static let checking = "Checking everything"
  /// Its tail.
  static let endToEnd = "end to end."
  /// The headline once the check passed.
  static let everythingWorks = "Everything works."
  /// Its tail.
  static let agentsHaveAMac = "Your agents have a Mac now."
  /// The headline once the check failed.
  static let almostThere = "Almost there."
  /// Its tail.
  static let oneCheckFailed = "One check didn't pass."
  /// The live check's note while it runs.
  static let checkingNote = "checking…"
  /// Its note once passed.
  static let passed = "passed"
  /// Its note once failed.
  static let failed = "failed"

  /// The first check, naming the helper.
  ///
  /// - Parameter helper: The helper as people know it.
  /// - Returns: "The Atlas account is logged in"
  static func loggedIn(_ helper: String) -> String {
    "\(namedFirst(helper)) is logged in"
  }
}

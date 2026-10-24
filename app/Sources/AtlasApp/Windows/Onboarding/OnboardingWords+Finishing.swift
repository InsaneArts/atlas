/// The finishing screen's words: Atlas checking the helper works, and what failed when it does not. The live check's
/// own word is ``StatusWord``'s.
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
  static let oneCheckFailed = "One check didn’t pass."

  /// The first check, naming the helper.
  ///
  /// - Parameter helper: The helper as people know it.
  /// - Returns: "The Atlas account is logged in"
  static func loggedIn(_ helper: String) -> String {
    "\(namedFirst(helper)) is logged in"
  }
}

import AtlasUI

/// The tooltips' facts: the helper's names for them, Ready to act's one fact and the count of the checks that pass,
/// a session's plans and screenshots, and one fact as a line.
extension ViewerWords {
  /// The account fact's name.
  static let accountRow = "Account"
  /// The state fact's name.
  static let stateRow = "State"
  /// How often the picture updates, as its fact's name.
  static let updatesRow = "Picture"
  /// How old the newest picture is, as its fact's name.
  static let newestRow = "Newest picture"
  /// The login time fact's name.
  static let loggedInRow = "Logged in"
  /// The connected agents fact's name.
  static let agentsRow = "Agents"
  /// Ready to act's one fact while every check passes.
  static let readyToAct = "Ready to act"
  /// That fact's value.
  static let allChecksPass = "All checks pass"
  /// No agent is connected to Atlas yet.
  static let noAgents = "None"
  /// An environment plan a session applied, as its fact's name.
  static let planFact = "Environment plan"
  /// How many screenshots a session took, as its fact's name.
  static let shotsFact = "Screenshots"
  /// The heading over what a session's last cleanup did, in its tooltip.
  static let lastCleanup = "Last cleanup"

  /// The quiet count of the checks that pass, after those that do not.
  ///
  /// - Parameter count: How many pass.
  /// - Returns: "1 more check passes" or "2 more checks pass".
  static func othersPass(_ count: Int) -> String {
    count == 1 ? "1 more check passes" : "\(count) more checks pass"
  }

  /// Where a plan stands, and when it was made once known, as its fact's value.
  ///
  /// - Parameters:
  ///   - phase: Where it stands (``planPhase(_:)``).
  ///   - madeAt: When it was made, on the person's clock; nil when unknown.
  /// - Returns: "Applied · can roll back · made at 10:02", or "Applied".
  static func planLine(_ phase: String, madeAt: String?) -> String {
    madeAt.map { "\(phase) · made at \($0)" } ?? phase
  }

  /// One fact as a line of a tooltip or a menu.
  ///
  /// - Parameter fact: The fact.
  /// - Returns: "Folder: understudy", or its name alone while it has no value, as "Copy of Opie".
  static func factLine(_ fact: Fact) -> String {
    fact.value.isEmpty ? fact.name : "\(fact.name): \(fact.value)"
  }
}

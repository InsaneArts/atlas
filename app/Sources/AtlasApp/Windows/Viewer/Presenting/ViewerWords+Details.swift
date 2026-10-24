/// Details' words: its rows' names, Ready to act's one row, and the count of the checks that pass.
extension ViewerWords {
  /// The account row's name.
  static let accountRow = "Account"
  /// The state row's name.
  static let stateRow = "State"
  /// The login time row's name.
  static let loggedInRow = "Logged in"
  /// The connected agents row's name.
  static let agentsRow = "Agents"
  /// Ready to act's one row while every check passes.
  static let readyToAct = "Ready to act"
  /// That row's value.
  static let allChecksPass = "All checks pass"

  /// The quiet count of the checks that pass, under those that do not.
  ///
  /// - Parameter count: How many pass.
  /// - Returns: "1 other passes" or "2 others pass".
  static func othersPass(_ count: Int) -> String {
    count == 1 ? "1 other passes" : "\(count) others pass"
  }

  /// No agent is connected to Atlas yet.
  static let noAgents = "None"
}

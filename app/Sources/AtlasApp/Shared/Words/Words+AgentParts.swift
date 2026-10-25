import AtlasCommands

extension Words {
  /// What to do after changing a connection while the Claude app is running.
  static let claudeRestart = "Quit and reopen the Claude app for this to take effect."

  /// Which Claude connections apply on this Mac.
  ///
  /// - Parameter parts: The installed CLI component ids.
  /// - Returns: The parts in plain words.
  static func agentParts(_ parts: [String]) -> String {
    let code = parts.contains(AgentID.claude.rawValue)
    let app = parts.contains(AgentsStatus.claudeAppID)
    if code && app { return "Claude Code and the Claude app" }
    if code { return "Claude Code" }
    if app { return "The Claude app" }
    return "Not installed"
  }
}

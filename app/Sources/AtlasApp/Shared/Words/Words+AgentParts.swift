import AtlasCommands

extension Words {
  /// What to do after changing a connection while the Claude app is running.
  static let claudeRestart = "Quit and reopen the Claude app for this to take effect."

  /// What to do after changing this host's connection.
  ///
  /// - Parameter agent: The host whose change requires a restart.
  /// - Returns: The instruction naming that host.
  static func agentRestart(_ agent: AgentID) -> String {
    switch agent {
    case .claude: claudeRestart
    case .opencode: "Restart opencode for this change to take effect."
    case .pi: "Start a new Pi session or use /reload for this change to take effect."
    case .codex: "Start a new Codex session for this change to take effect."
    case .cursor: "Reload the Cursor window or start a new Cursor Agent session for this change to take effect."
    }
  }

  /// Every required restart from one connection change, in the app's agent order.
  ///
  /// - Parameter change: The CLI's decoded results, when available.
  /// - Returns: The required instructions, or nil when no host needs restarting.
  static func agentRestarts(_ change: AgentsChange?) -> String? {
    guard let ids = change?.restarting, !ids.isEmpty else { return nil }
    return ids.map(agentRestart)
      .joined(separator: " ")
  }

  /// What a connection enables, including the live Mac view available inside the chat apps.
  ///
  /// - Parameters:
  ///   - agent: The agent whose connection the row offers.
  ///   - parts: The installed Claude components; other agents have one connection.
  /// - Returns: The benefit shown under the agent's name in setup and Settings.
  static func agentDetail(_ agent: AgentID, parts: [String]) -> String {
    switch agent {
    case .claude:
      let code = parts.contains(AgentID.claude.rawValue)
      let app = parts.contains(AgentsStatus.claudeAppID)
      if code && app { return "Claude Code tools and a live Mac view in the app" }
      if app { return "Live Mac view in the Claude app" }
      return "Test Mac apps from Claude Code"
    case .codex: return "Live Mac view in the Codex app"
    case .opencode, .pi: return "Test Mac apps on a hidden desktop"
    case .cursor: return "Test Mac apps from Cursor"
    }
  }
}

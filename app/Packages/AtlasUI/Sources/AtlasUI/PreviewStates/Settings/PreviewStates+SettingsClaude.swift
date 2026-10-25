#if DEBUG
  extension PreviewStates {
    /// Only Claude Code is installed.
    @MainActor static let settingsClaudeCode: SettingsState = {
      var state = selecting(.agents)
      state.agents.agents[0].detail = "Claude Code"
      return state
    }()

    /// Only the Claude app is installed.
    @MainActor static let settingsClaudeApp: SettingsState = {
      var state = selecting(.agents)
      state.agents.agents[0].detail = "The Claude app"
      return state
    }()

    /// Claude Code is connected and the Claude app still needs Connect.
    @MainActor static let settingsClaudePartial: SettingsState = {
      var state = selecting(.agents)
      state.agents.agents[0].connection = .connect(isEnabled: true)
      return state
    }()
  }
#endif

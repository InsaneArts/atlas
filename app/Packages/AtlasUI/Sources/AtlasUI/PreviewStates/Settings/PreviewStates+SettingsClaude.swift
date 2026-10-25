#if DEBUG
  extension PreviewStates {
    /// Only Claude Code is installed.
    @MainActor static let settingsClaudeCode: SettingsState = {
      var state = selecting(.agents)
      state.agents.agents[0].detail = "Test Mac apps from Claude Code"
      return state
    }()

    /// Only the Claude app is installed.
    @MainActor static let settingsClaudeApp: SettingsState = {
      var state = selecting(.agents)
      state.agents.agents[0].detail = "Live Mac view in the Claude app"
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

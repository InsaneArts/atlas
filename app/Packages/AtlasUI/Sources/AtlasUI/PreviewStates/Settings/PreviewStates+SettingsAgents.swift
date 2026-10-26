#if DEBUG
  extension PreviewStates {
    /// The Agents pane: Claude and Codex connected, Cursor configured, and two projects taught.
    static let settingsAgentsPane = SettingsAgentsState(
      agents: [
        SettingsAgentRow(
          id: "claude",
          title: "Claude",
          detail: "Claude Code tools and a live Mac view in the app",
          icon: .app(AgentLogos.claude),
          connection: .connected,
          canDisconnect: true
        ),
        SettingsAgentRow(
          id: "codex",
          title: "Codex",
          detail: "Live Mac view in the Codex app",
          icon: .app(AgentLogos.codex),
          connection: .connected,
          canDisconnect: true
        ),
        SettingsAgentRow(
          id: "opencode",
          title: "opencode",
          detail: "Test Mac apps on a hidden desktop",
          icon: .mark(.opencode),
          connection: .connect(isEnabled: true)
        ),
        SettingsAgentRow(
          id: "pi",
          title: "Pi",
          detail: "Test Mac apps on a hidden desktop",
          icon: .mark(.pi),
          connection: .connect(isEnabled: true)
        ),
        SettingsAgentRow(
          id: "cursor",
          title: "Cursor",
          detail: "Test Mac apps from Cursor",
          icon: .app(AgentLogos.cursor),
          connection: .configured,
          canDisconnect: true
        ),
      ],
      projects: [
        SettingsProjectRow(id: "/Users/me/dev/opie", name: "opie", detail: "~/dev/opie", forgetLabel: "Forget opie"),
        SettingsProjectRow(
          id: "/Users/me/dev/atlas-landing",
          name: "atlas-landing",
          detail: "No longer has the Atlas section",
          detailIsPath: false,
          forgetLabel: "Forget atlas-landing"
        ),
      ]
    )

    /// Claude and Cursor connecting: Connect keeps its width with a spinner in its words' place.
    @MainActor static let settingsAgentsConnecting: SettingsState = {
      var state = selecting(.agents)
      state.agents.agents[0].connection = .connecting
      state.agents.agents[0].canDisconnect = false
      state.agents.agents[4].connection = .connecting
      state.agents.agents[4].canDisconnect = false
      return state
    }()

    /// Codex not on this Mac, no project taught yet, and a Connect that failed on the Claude app's settings file.
    @MainActor static let settingsAgentsEmpty: SettingsState = {
      var state = selecting(.agents)
      state.agents.agents[1].detail = "Not installed"
      state.agents.agents[1].connection = .missing
      state.agents.agents[0].problem = "The Claude app couldn't connect: its settings file has a mistake"
      state.agents.agents[0].showsFile = true
      state.agents.agents[0].connection = .connect(isEnabled: true)
      state.agents.agents[4].detail = "Not installed"
      state.agents.agents[4].connection = .missing
      state.agents.agents[4].canDisconnect = false
      state.agents.agents[4].problem = "Cursor's Atlas files changed. Move the plugin aside before reconnecting."
      state.agents.projects = []
      return state
    }()
  }
#endif

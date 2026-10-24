#if DEBUG
  extension PreviewStates {
    /// The Agents pane: Claude Code and Codex connected, the Claude app ready to connect, and two projects taught.
    static let settingsAgentsPane = SettingsAgentsState(
      agents: [
        SettingsAgentRow(
          id: "claude",
          title: "Claude Code",
          detail: "Uses Atlas in every project",
          icon: .tile(.symbol("asterisk")),
          connection: .connected
        ),
        SettingsAgentRow(
          id: "codex",
          title: "Codex",
          detail: "Uses Atlas in every project",
          icon: .tile(.text(">_")),
          connection: .connected
        ),
        SettingsAgentRow(
          id: "claude-app",
          title: "Claude app",
          detail: "Shows the helper in your chats",
          icon: .tile(.symbol("bubble.left")),
          connection: .connect(isEnabled: true)
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

    /// The Claude app connecting: Connect keeps its width with a spinner in its words' place.
    @MainActor static let settingsAgentsConnecting: SettingsState = {
      var state = selecting(.agents)
      state.agents.agents[2].connection = .connecting
      return state
    }()

    /// Codex not on this Mac, no project taught yet, and a Connect that failed on the Claude app's settings file.
    @MainActor static let settingsAgentsEmpty: SettingsState = {
      var state = selecting(.agents)
      state.agents.agents[1].detail = "Not installed"
      state.agents.agents[1].connection = .missing
      state.agents.agents[2].problem = "Couldn't connect: its settings file has a mistake"
      state.agents.agents[2].showsFile = true
      state.agents.projects = []
      return state
    }()
  }
#endif

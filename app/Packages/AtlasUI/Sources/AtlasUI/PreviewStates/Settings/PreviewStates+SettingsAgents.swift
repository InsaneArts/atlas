#if DEBUG
  extension PreviewStates {
    /// v3's Agents pane: Claude Code and Codex connected, the Claude app ready to connect, and two projects taught.
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

    /// The Claude app connecting: Connect keeps its place and gains a spinner.
    @MainActor static let settingsAgentsConnecting: SettingsState = {
      var state = selecting(.agents)
      state.agents.agents[2].connection = .connecting
      return state
    }()

    /// Codex not on this Mac, no project taught yet, and a Connect that failed.
    @MainActor static let settingsAgentsEmpty: SettingsState = {
      var state = selecting(.agents)
      state.agents.agents[1].detail = "Not on this Mac"
      state.agents.agents[1].connection = .note("Not installed")
      state.agents.projects = []
      state.agents.notice = SettingsNote(text: "Claude app: its settings file is not valid JSON.", kind: .problem)
      return state
    }()
  }
#endif

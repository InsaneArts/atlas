import SwiftUI

/// Coding agents with their existing actions and failures, followed by a live link to taught projects.
struct AgentsPane: View {
  /// What the pane shows.
  let state: SettingsAgentsState
  /// Receives what the person does.
  let send: @MainActor (SettingsEvent) -> Void

  /// What it draws, from its state.
  var body: some View {
    VStack(alignment: .leading, spacing: SettingsLayout.sectionGap) {
      GroupedSection(
        state.agents,
        title: "Coding agents",
        description: "Let supported agents use the helper.",
        notes: notes
      ) { agent in
        AgentConnectionRow(agent: agent) {
          send(.connect(agent.id))
        } disconnect: {
          send(.disconnect(agent.id))
        } showFile: {
          send(.showAgentFile(agent.id))
        }
      }
      GroupedSection(
        [GeneralRow.picture],
        title: "Project instructions",
        description: "Projects taught to agents without the Atlas plugin."
      ) { _ in
        SettingsDisclosureRow(title: "Projects", summary: state.projectsSummary) { send(.navigate(.projects)) }
      }
    }
  }

  /// What the last Connect or Forget said, under the agents.
  private var notes: [GroupedSectionNote] {
    [state.notice?.sectionNote]
      .compactMap { $0 }
  }
}

#if DEBUG
  #Preview("Agents") {
    FormColumn(insets: SettingsLayout.swapInsets) {
      AgentsPane(state: PreviewStates.settingsAgents.agents) { _ in }
    }
    .frame(width: SettingsLayout.width)
    .sampleSurface()
  }
#endif

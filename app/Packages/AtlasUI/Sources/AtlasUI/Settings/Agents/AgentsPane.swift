import SwiftUI

/// The Agents pane: each agent with its icon and whether it is connected, then, when `atlas agents project` taught
/// any, each project with Forget…, which asks first. Why a Connect failed sits in its agent's row; what else the last
/// command said sits under the agents.
struct AgentsPane: View {
  /// What the pane shows.
  let state: SettingsAgentsState
  /// Receives what the person does.
  let send: @MainActor (SettingsEvent) -> Void

  /// What it draws, from its state.
  var body: some View {
    VStack(alignment: .leading, spacing: SettingsLayout.sectionGap) {
      GroupedSection(state.agents, notes: notes) { agent in
        AgentConnectionRow(agent: agent) {
          send(.connect(agent.id))
        } showFile: {
          send(.showAgentFile(agent.id))
        }
      }
      if !state.projects.isEmpty {
        GroupedSection(
          state.projects,
          title: "Projects",
          notes: [
            .text("Agents without the Atlas plugin read these projects' AGENTS.md. Forget takes Atlas's section out.")
          ]
        ) { project in
          ProjectRow(project: project) { send(.askForget(project.id)) }
        }
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

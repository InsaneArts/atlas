import SwiftUI

/// The Integrations page: each coding agent with its icon and its connection's switch, then, when
/// `atlas agents project` taught any, each project with Forget…, which asks first. Why a change failed sits in its
/// agent's row; what else the last command said sits under the agents.
struct IntegrationsPage: View {
  /// What the page shows.
  let state: SettingsAgentsState
  /// Receives what the person does.
  let send: @MainActor (SettingsEvent) -> Void

  /// What it draws, from its state.
  var body: some View {
    VStack(alignment: .leading, spacing: SettingsLayout.sectionGap) {
      GroupedSection(state.agents, title: "Coding Agents", notes: notes) { agent in
        AgentConnectionRow(agent: agent) {
          send(.connect(agent.id))
        } disconnect: {
          send(.disconnect(agent.id))
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

  /// What the last Connect or Forget said, then what connecting does.
  private var notes: [GroupedSectionNote] {
    [state.notice?.sectionNote]
      .compactMap { $0 }
      + [.text("Connecting installs the Atlas plugin, so the agent knows how to use the helper.")]
  }
}

#if DEBUG
  #Preview("Integrations") {
    SettingsPageSample { IntegrationsPage(state: PreviewStates.settingsAgents.agents) { _ in } }
  }

  #Preview("Integrations, dark") {
    SettingsPageSample { IntegrationsPage(state: PreviewStates.settingsAgents.agents) { _ in } }
      .environment(\.colorScheme, .dark)
  }
#endif

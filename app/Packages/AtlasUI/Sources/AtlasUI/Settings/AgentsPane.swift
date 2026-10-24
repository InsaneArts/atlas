import SwiftUI

/// The Agents pane: each agent with whether it is connected or Connect, then, when `atlas agents project` taught any,
/// each project with Forget, which asks first.
struct AgentsPane: View {
  /// What the pane shows.
  let state: SettingsAgentsState
  /// Receives what the person does.
  let send: @MainActor (SettingsEvent) -> Void

  var body: some View {
    VStack(alignment: .leading, spacing: SettingsLayout.groupGap) {
      SettingsGroup(title: "Connected", note: state.notice) {
        SeparatedRows(items: state.agents) { agent in
          FormRow(title: agent.title, detail: agent.detail, accessory: .icon(agent.icon)) {
            AgentConnection(connection: agent.connection) { send(.connect(agent.id)) }
          }
        }
      }
      if !state.projects.isEmpty {
        SettingsGroup(
          title: "Projects",
          footer: "Agents without the Atlas plugin read these projects' AGENTS.md. Forget takes Atlas's section out."
        ) {
          SeparatedRows(items: state.projects) { project in
            FormRow(title: project.name, detail: project.detail) {
              Button("Forget") { send(.askForget(project.id)) }
                .buttonStyle(.capsule(.glass, size: .mini))
                .accessibilityLabel(project.forgetLabel)
            }
          }
        }
      }
    }
  }
}

#if DEBUG
  #Preview("Agents") {
    AgentsPane(state: PreviewStates.settingsAgents.agents) { _ in }
      .frame(width: SettingsLayout.formWidth)
      .sampleSurface()
  }
#endif

import SwiftUI

/// The Agents pane: each agent with whether it is connected or Connect, then each project taught with Forget, which
/// asks first.
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
      SettingsGroup(
        title: "Projects",
        footer: "Teach a project so agents know how to build it and check it in the helper."
      ) {
        if state.projects.isEmpty {
          FormRow(title: "No projects taught yet")
        } else {
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

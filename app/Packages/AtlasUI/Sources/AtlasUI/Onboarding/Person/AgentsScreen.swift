import SwiftUI

/// Connect agents to Atlas: one group, each agent under its own icon with a small switch, which waits, dimmed, until
/// the agent is found. While the agents are read, the status line says so. Continue is the footer's.
struct AgentsScreen: View {
  /// The agents.
  let state: OnboardingAgentsScreenState
  /// The status under the group.
  let note: WorkNote
  /// Receives the switches.
  let send: @MainActor (OnboardingEvent) -> Void

  /// The agents' group and the status in its kept place.
  var body: some View {
    ScreenColumn {
      OnboardingHeader(label: "Your agents", title: OnboardingScreen.agentsTitle, tail: OnboardingScreen.agentsTail)
      GroupedList(state.agents) { agent in
        ListRow(
          agent.title,
          detail: agent.detail,
          lead: Self.lead(agent.icon),
          trailing: .toggle(isOn: agent.isOn)
        ) { send(.setAgent(id: agent.id, on: !agent.isOn)) }
        .disabled(!agent.isEnabled)
      }
      .controlSize(.small)
      .padding(.top, OnboardingLayout.Rhythm.paragraph)
      StatusSlot(note: note)
    }
  }

  /// What leads an agent's row: its installed app's icon or its bundled original logo.
  ///
  /// - Parameter icon: The agent's icon.
  /// - Returns: The row's lead.
  private static func lead(_ icon: RowIcon) -> ListRow.Lead {
    switch icon {
    case .app(let image): .image(image)
    case .tile(let tile): .tile(tile)
    }
  }
}

#if DEBUG
  #Preview("Agents") {
    AgentsScreen(state: PreviewStates.onboardingAgentsScreen, note: .none) { _ in }
      .onboardingSample()
  }
#endif

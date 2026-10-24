import SwiftUI

/// Connect agents to Atlas: one group, each agent with a small switch, which waits, dimmed, until the agent is found
/// (v3 D6). Continue is the footer's.
struct AgentsScreen: View {
  /// The agents.
  let state: OnboardingAgentsScreenState
  /// The status under the group.
  let note: WorkNote
  /// Receives the switches.
  let send: @MainActor (OnboardingEvent) -> Void

  /// The agents' group and the status.
  var body: some View {
    ScrollColumn(width: OnboardingLayout.column) {
      OnboardingHeader(label: "Your agents", title: OnboardingScreen.agentsTitle, tail: OnboardingScreen.agentsTail)
      GroupedList(state.agents) { agent in
        ListRow(
          agent.title,
          detail: agent.detail,
          lead: .tile(.text(">_")),
          trailing: .toggle(isOn: agent.isOn)
        ) { send(.setAgent(id: agent.id, on: !agent.isOn)) }
        .disabled(!agent.isEnabled)
      }
      .controlSize(.small)
      .padding(.top, OnboardingLayout.Rhythm.paragraph)
      .pagePiece(.content)
    } bottom: {
      if let status = StatusLine(note: note) {
        status
          .padding(.top, OnboardingLayout.Rhythm.content)
          .pagePiece(.status)
      }
    }
  }
}

#if DEBUG
  #Preview("Agents") {
    AgentsScreen(state: PreviewStates.onboardingAgentsScreen, note: .none) { _ in }
      .onboardingSample()
  }
#endif

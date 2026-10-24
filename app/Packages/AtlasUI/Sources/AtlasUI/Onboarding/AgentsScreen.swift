import SwiftUI

/// Connect agents to Atlas.
struct AgentsScreen: View {
  /// The agents.
  let state: OnboardingAgentsScreenState
  /// The line under the card.
  let note: WorkNote
  /// Receives the switches.
  let send: @MainActor (OnboardingEvent) -> Void

  /// The agents' card and the work note.
  var body: some View {
    ScreenColumn(width: OnboardingLayout.listColumn) {
      GroupLabel(text: "Your agents")
      Headline(title: OnboardingScreen.agentsTitle, tail: OnboardingScreen.agentsTail)
      CardGroup(state.agents) { agent in
        AgentRow(agent: agent) { send(.setAgent(id: agent.id, on: $0)) }
      }
      .padding(.top, Spacing.xsPlus)
    } bottom: {
      WorkNoteLine(note: note)
    }
  }
}

#if DEBUG
  #Preview("Agents") {
    AgentsScreen(state: PreviewStates.onboardingAgentsScreen, note: .none) { _ in }
      .onboardingSample()
  }
#endif

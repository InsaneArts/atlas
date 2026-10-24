import SwiftUI

/// Connect agents to Atlas. Continue is the footer's.
struct AgentsScreen: View {
  /// The agents.
  let state: OnboardingAgentsScreenState
  /// The line under the card.
  let note: WorkNote
  /// Receives the switches.
  let send: @MainActor (OnboardingEvent) -> Void

  /// The agents' card and the work note.
  var body: some View {
    ScrollColumn(width: OnboardingLayout.column) {
      OnboardingHeader(label: "Your agents", title: OnboardingScreen.agentsTitle, tail: OnboardingScreen.agentsTail)
      CardGroup(state.agents) { agent in
        AgentRow(agent: agent) { send(.setAgent(id: agent.id, on: $0)) }
      }
      .padding(.top, OnboardingLayout.Rhythm.paragraph)
      .pagePiece(.content)
    } bottom: {
      WorkNoteLine(note: note)
        .padding(.top, OnboardingLayout.Rhythm.content)
        .pagePiece(.status)
    }
  }
}

#if DEBUG
  #Preview("Agents") {
    AgentsScreen(state: PreviewStates.onboardingAgentsScreen, note: .none) { _ in }
      .onboardingSample()
  }
#endif

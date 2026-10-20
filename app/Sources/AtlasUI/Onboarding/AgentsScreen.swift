import SwiftUI

/// Connect agents to Atlas, and teach project folders to check their work on screen.
struct AgentsScreen: View {
  /// The agents and the chosen projects.
  let state: OnboardingAgentsScreenState
  /// The line under the cards.
  let note: WorkNote
  /// Whether a command runs.
  let isBusy: Bool
  /// Receives the switches, the folder choice and removals.
  let send: @MainActor (OnboardingEvent) -> Void

  /// The agents' card, the projects' card, and the work note.
  var body: some View {
    ScreenColumn(width: OnboardingLayout.listColumn) {
      Eyebrow(text: "Your agents")
      Headline(title: "Teach your agents", tail: "to test on screen.")
      CardGroup(state.agents) { agent in
        AgentRow(agent: agent) { send(.setAgent(id: agent.id, on: $0)) }
      }
      .padding(.top, Spacing.s)
      ProjectsCard(projects: state.projects, isBusy: isBusy, send: send)
      WorkNoteLine(note: note)
    }
  }
}

#if DEBUG
  #Preview("Agents") {
    AgentsScreen(state: PreviewStates.onboardingAgentsScreen, note: .none, isBusy: false) { _ in }
      .onboardingSample()
  }
#endif

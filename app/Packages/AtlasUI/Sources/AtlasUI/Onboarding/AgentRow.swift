import SwiftUI

/// One agent, with its switch.
struct AgentRow: View {
  /// The agent.
  let agent: OnboardingAgentItem
  /// Receives the switch turned on or off.
  let set: @MainActor (Bool) -> Void

  /// The agent's tile, name and state, then its switch, read as one.
  var body: some View {
    IconRow(icon: .text(">_"), title: agent.title, detail: agent.detail) {
      Toggle(agent.title, isOn: Binding(get: { agent.isOn }, set: { set($0) }))
        .toggleStyle(.accentSwitch)
        .disabled(!agent.isEnabled)
    }
    .accessibilityElement(children: .combine)
  }
}

#if DEBUG
  #Preview("Found and connected") {
    CardGroup(PreviewStates.onboardingAgentsScreen.agents) { agent in
      AgentRow(agent: agent) { _ in }
    }
    .frame(width: SampleWidth.column)
    .sampleSurface()
  }
#endif

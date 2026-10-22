import SwiftUI

/// The right side of an agent's row: "Connected" in green, Connect, Connecting… while it runs, or why it can't.
struct AgentConnection: View {
  /// Where the agent stands.
  let connection: SettingsAgentRow.Connection
  /// Connects the agent.
  let connect: @MainActor () -> Void

  var body: some View {
    switch connection {
    case .connected:
      Text("Connected")
        .font(Typography.font(.callout, weight: .medium))
        .foregroundStyle(Color.success)
    case .connect(let isEnabled):
      Button("Connect", action: connect)
        .buttonStyle(.capsule(.primary, size: .mini))
        .disabled(!isEnabled)
    case .connecting:
      Button("Connecting…", action: connect)
        .buttonStyle(.capsule(.primary, size: .mini))
        .disabled(true)
    case .note(let text):
      Text(text)
        .font(Typography.font(.callout))
        .foregroundStyle(Color.textTertiary)
    }
  }
}

#if DEBUG
  #Preview("Samples") {
    VStack(alignment: .trailing, spacing: Spacing.l) {
      AgentConnection(connection: .connected) {}
      AgentConnection(connection: .connect(isEnabled: true)) {}
      AgentConnection(connection: .connecting) {}
      AgentConnection(connection: .note(String("Not installed"))) {}
    }
    .sampleSurface()
  }
#endif

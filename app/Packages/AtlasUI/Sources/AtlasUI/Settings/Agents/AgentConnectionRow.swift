import SwiftUI

/// One agent in the Agents pane (v3 G7): its real icon, its name over what Atlas does in it, then where it stands:
/// a green check and Connected, which pops in; Connect, which keeps its label and gains a spinner as Connecting…; or
/// why it can't be connected.
struct AgentConnectionRow: View {
  /// The agent.
  let agent: SettingsAgentRow
  /// Connects the agent.
  let connect: @MainActor () -> Void
  @Environment(\.accessibilityReduceMotion) private var reduceMotion

  /// What it draws, from its state.
  var body: some View {
    ListRowBody(agent.title, detail: agent.detail, lead: lead, size: .compact) {
      ZStack(alignment: .trailing) {
        connection
          .id(isConnected)
          .transition(
            Motion.transition(.scale(scale: Layout.arrival).combined(with: .opacity), reduceMotion: reduceMotion)
          )
      }
      .animation(Motion.animation(Motion.pop, reduceMotion: reduceMotion, reduced: Motion.reduced), value: isConnected)
    }
  }

  /// Where the agent stands, at the row's end.
  @ViewBuilder private var connection: some View {
    switch agent.connection {
    case .connected:
      HStack(spacing: Spacing.xsPlus) {
        OutcomeMark(kind: .done)
        Text(verbatim: "Connected")
          .textStyle(.callout)
          .foregroundStyle(Color.textSuccess)
      }
      .accessibilityElement(children: .combine)
    case .connect(let isEnabled):
      connectButton(isWorking: false)
        .disabled(!isEnabled)
    case .connecting:
      connectButton(isWorking: true)
        .disabled(true)
    case .note(let text):
      Text(text)
        .textStyle(.callout)
        .foregroundStyle(Color.textTertiary)
    }
  }

  /// Connect, or Connecting… with a spinner while it runs: the same button, so its place never moves.
  ///
  /// - Parameter isWorking: Whether the connection runs now.
  /// - Returns: The button.
  private func connectButton(isWorking: Bool) -> some View {
    Button(action: connect) {
      WorkingLabel(title: isWorking ? "Connecting…" : "Connect", isWorking: isWorking)
    }
    .buttonStyle(.capsule(.glass, size: .mini, isWorking: isWorking))
  }

  /// The agent's icon: its app's own where the app is on this Mac, else its tile.
  private var lead: ListRow.Lead {
    switch agent.icon {
    case .app(let icon): .image(icon)
    case .tile(let tile): .tile(tile)
    }
  }

  /// Whether the agent is connected, which pops its check in.
  private var isConnected: Bool {
    agent.connection == .connected
  }
}

/// The row's own measures (v3's Connect → Connected pop).
private enum Layout {
  /// 0.5: how small Connected's check and word start as they pop in.
  static let arrival: CGFloat = 0.5
}

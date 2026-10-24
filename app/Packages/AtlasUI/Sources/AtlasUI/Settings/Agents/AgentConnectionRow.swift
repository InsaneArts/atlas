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
          .transition(Motion.transition(arrival, reduceMotion: reduceMotion))
      }
      .animation(Motion.animation(Motion.pop, reduceMotion: reduceMotion, reduced: Motion.reduced), value: isConnected)
    }
  }

  /// Where the agent stands, at the row's end. Connect and Connecting… are one branch, so they are one button whose
  /// label morphs, never two that swap.
  @ViewBuilder private var connection: some View {
    switch Self.trailing(agent.connection) {
    case .connected:
      HStack(spacing: Spacing.xsPlus) {
        OutcomeMark(kind: .done)
        Text(verbatim: "Connected")
          .textStyle(.callout)
          .foregroundStyle(Color.textSuccess)
      }
      .accessibilityElement(children: .combine)
    case .button(let isWorking, let isEnabled):
      connectButton(isWorking: isWorking)
        .disabled(!isEnabled)
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

  /// How Connected's check and word pop in: from small as they fade in.
  private var arrival: AnyTransition {
    AnyTransition.scale(scale: Layout.arrival)
      .combined(with: .opacity)
  }

  /// What stands at the row's end for a connection: Connect and Connecting… as one button, so its place and identity
  /// hold while the connection runs.
  ///
  /// - Parameter connection: Where the agent stands.
  /// - Returns: The row's end.
  nonisolated static func trailing(_ connection: SettingsAgentRow.Connection) -> Trailing {
    switch connection {
    case .connected: .connected
    case .connect(let isEnabled): .button(isWorking: false, isEnabled: isEnabled)
    case .connecting: .button(isWorking: true, isEnabled: false)
    case .note(let text): .note(text)
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

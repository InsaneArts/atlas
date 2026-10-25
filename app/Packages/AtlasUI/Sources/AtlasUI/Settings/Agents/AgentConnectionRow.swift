import SwiftUI

/// One agent in the Agents pane: its real icon, its name over what Atlas does in it, then where it stands: a green
/// check and Connected, which pops in; Connect, which keeps its width with a spinner in its words' place as
/// Connecting…; or, for an agent not on this Mac, "Not installed" under its faded icon and nothing at its end. A
/// Connect that failed says why in the row, in orange, with Show File where a file needs mending.
struct AgentConnectionRow: View {
  /// The agent.
  let agent: SettingsAgentRow
  /// Connects the agent.
  let connect: @MainActor () -> Void
  /// Disconnects all the agent's parts.
  let disconnect: @MainActor () -> Void
  /// Shows the file its problem is in.
  let showFile: @MainActor () -> Void
  @Environment(\.accessibilityReduceMotion) private var reduceMotion

  /// What it draws, from its state.
  var body: some View {
    ListRowBody(
      agent.title,
      detail: agent.problem ?? agent.detail,
      detailStyle: agent.problem == nil ? .plain : .problem,
      lead: lead,
      size: .compact,
      leadOpacity: agent.connection == .missing ? Opacity.muted : 1
    ) {
      HStack(spacing: Spacing.m) {
        if agent.canDisconnect {
          Button("Disconnect", action: disconnect)
            .buttonStyle(.textLink(.small))
        }
        if agent.showsFile {
          Button("Show File", action: showFile)
            .buttonStyle(.textLink(.small))
            .help("Show the file in the Finder")
        }
        ZStack(alignment: .trailing) {
          connection
            .id(isConnected)
            .transition(Motion.transition(arrival, reduceMotion: reduceMotion))
        }
        .animation(
          Motion.animation(Motion.pop, reduceMotion: reduceMotion, reduced: Motion.reduced),
          value: isConnected
        )
      }
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
          .foregroundStyle(Color.textSecondary)
      }
      .accessibilityElement(children: .combine)
    case .button(let isWorking, let isEnabled):
      connectButton(isWorking: isWorking)
        .disabled(!isEnabled)
    case .none:
      EmptyView()
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
      WorkingLabel(title: "Connect", isWorking: isWorking, workingTitle: "Connecting…")
    }
    .buttonStyle(.capsule(.glass, size: .mini, isWorking: isWorking))
  }

  /// The agent's icon: its installed app's icon or its bundled original logo.
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
    case .missing: .none
    case .note(let text): .note(text)
    }
  }

  /// Whether the agent is connected, which pops its check in.
  private var isConnected: Bool {
    agent.connection == .connected
  }
}

/// The row's own measures.
private enum Layout {
  /// 0.5: how small Connected's check and word start as they pop in.
  static let arrival: CGFloat = 0.5
}

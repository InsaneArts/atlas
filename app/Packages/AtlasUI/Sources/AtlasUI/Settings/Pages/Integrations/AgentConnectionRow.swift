import SwiftUI

/// One agent on the Integrations page: its icon, its name over what Atlas does in it, and its connection as a switch,
/// on to connect and off to disconnect. While a connection or a disconnection runs a spinner stands beside the switch,
/// which waits on, and VoiceOver hears which runs.
/// Partly connected, it says so and offers Connect beside the switch to finish. Not on this Mac, its icon fades and it
/// says "Not installed", with no switch; while its state isn't known it says so in the switch's place. A change that
/// failed says why in the row, in orange, with Show File where a file needs mending.
struct AgentConnectionRow: View {
  /// The agent.
  let agent: SettingsAgentRow
  /// Connects the agent.
  let connect: @MainActor () -> Void
  /// Disconnects all the agent's parts.
  let disconnect: @MainActor () -> Void
  /// Shows the file its problem is in.
  let showFile: @MainActor () -> Void
  @Environment(\.motionReduced) private var reduceMotion

  /// What it draws, from its state.
  var body: some View {
    ListRowBody(
      agent.title,
      detail: line,
      detailStyle: agent.problem == nil ? .plain : .problem,
      lead: lead,
      size: .compact,
      leadOpacity: agent.connection == .missing ? Opacity.muted : 1
    ) {
      HStack(spacing: Spacing.sPlus) {
        if agent.showsFile {
          Button("Show File", action: showFile)
            .buttonStyle(.textLink(.small))
            .help("Show the file in the Finder")
        }
        if agent.isPartlyConnected {
          Button("Connect", action: connect)
            .buttonStyle(.capsule(.secondary, size: .row))
            .disabled(!Self.canConnect(agent.connection))
        }
        end
      }
      .animation(Motion.animation(Motion.state, reduceMotion: reduceMotion), value: Self.trailing(agent))
    }
  }

  /// The line under the name: why the last change failed, that it is connected only in part, or what Atlas does in
  /// it.
  private var line: String {
    if let problem = agent.problem { return problem }
    return agent.isPartlyConnected ? "Partly connected" : agent.detail
  }

  /// The row's end: the switch, a spinner beside it, or words in its place.
  @ViewBuilder private var end: some View {
    switch Self.trailing(agent) {
    case .toggle(let isOn, let isEnabled):
      connectionSwitch(isOn: isOn)
        .disabled(!isEnabled)
    case .working(let isConnecting):
      Spinner()
        .transition(.opacity)
      connectionSwitch(isOn: isConnecting)
        .disabled(true)
        .accessibilityValue(Text(Self.progress(agent.title, isConnecting: isConnecting)))
    case .none:
      EmptyView()
    case .note(let text):
      Text(text)
        .textStyle(.callout)
        .foregroundStyle(Color.textTertiary)
    }
  }

  /// The connection's switch: turning it on connects, turning it off disconnects.
  ///
  /// - Parameter isOn: Whether it stands on.
  /// - Returns: The switch.
  private func connectionSwitch(isOn: Bool) -> some View {
    Toggle(
      "Connect \(agent.title)",
      isOn: Binding(get: { isOn }, set: { $0 ? connect() : disconnect() })
    )
    .toggleStyle(.accentSwitch)
  }

  /// The agent's icon: its installed app's icon or its bundled original logo.
  private var lead: ListRow.Lead {
    switch agent.icon {
    case .app(let icon): .image(icon)
    case .tile(let tile): .tile(tile)
    case .mark(let kind): .mark(kind)
    }
  }

  /// What stands at a row's end for an agent: its switch, on while any part is connected and waiting while nothing
  /// may change; a spinner beside it while the connection runs; nothing for an agent not on this Mac; or words while
  /// its state isn't known.
  ///
  /// - Parameter agent: The agent.
  /// - Returns: The row's end.
  nonisolated static func trailing(_ agent: SettingsAgentRow) -> Trailing {
    switch agent.connection {
    case .connected, .configured: .toggle(isOn: true, isEnabled: agent.canDisconnect)
    case .connect(let isEnabled):
      .toggle(isOn: agent.hasConnection, isEnabled: agent.hasConnection ? agent.canDisconnect : isEnabled)
    case .connecting: .working(isConnecting: true)
    case .disconnecting: .working(isConnecting: false)
    case .missing: .none
    case .note(let text): .note(text)
    }
  }

  /// What VoiceOver hears of a switch whose change runs, so it never reads as one merely waiting on other work.
  ///
  /// - Parameters:
  ///   - agent: The agent's name.
  ///   - isConnecting: Whether it is being connected, rather than disconnected.
  /// - Returns: "Connecting Claude…" or "Disconnecting Claude…".
  nonisolated static func progress(_ agent: String, isConnecting: Bool) -> String {
    isConnecting ? "Connecting \(agent)…" : "Disconnecting \(agent)…"
  }

  /// Whether Connect can run now: the agent is on this Mac and nothing else connects.
  ///
  /// - Parameter connection: Where the agent stands.
  /// - Returns: Whether it can.
  nonisolated static func canConnect(_ connection: SettingsAgentRow.Connection) -> Bool {
    if case .connect(let isEnabled) = connection { return isEnabled }
    return false
  }
}

import Testing

@testable import AtlasUI

@Suite("AgentConnectionRow")
struct AgentConnectionRowTests {
  /// An agent standing at `connection`, with any parts connected or not.
  ///
  /// - Parameters:
  ///   - connection: Where it stands.
  ///   - canDisconnect: Whether any parts are connected and can be disconnected now.
  /// - Returns: The row.
  private static func agent(_ connection: SettingsAgentRow.Connection, canDisconnect: Bool = false) -> SettingsAgentRow
  {
    SettingsAgentRow(
      id: "claude",
      title: "Claude",
      detail: "Claude Code tools and a live Mac view in the app",
      icon: .tile(.text("C")),
      connection: connection,
      canDisconnect: canDisconnect
    )
  }

  @Test("a connected agent's switch stands on, and one ready to connect stands off, each waiting while it can't change")
  func theSwitchFollowsTheConnection() {
    let row = AgentConnectionRow.self
    #expect(row.trailing(Self.agent(.connected, canDisconnect: true)) == .toggle(isOn: true, isEnabled: true))
    #expect(row.trailing(Self.agent(.configured, canDisconnect: true)) == .toggle(isOn: true, isEnabled: true))
    #expect(row.trailing(Self.agent(.connected)) == .toggle(isOn: true, isEnabled: false))
    #expect(row.trailing(Self.agent(.connect(isEnabled: true))) == .toggle(isOn: false, isEnabled: true))
    #expect(row.trailing(Self.agent(.connect(isEnabled: false))) == .toggle(isOn: false, isEnabled: false))
  }

  @Test("a connection that runs shows its spinner beside the switch, and an agent not on this Mac shows no switch")
  func workingAndMissingAgents() {
    #expect(AgentConnectionRow.trailing(Self.agent(.connecting)) == .working(isConnecting: true))
    #expect(AgentConnectionRow.trailing(Self.agent(.disconnecting)) == .working(isConnecting: false))
    #expect(AgentConnectionRow.trailing(Self.agent(.missing)) == AgentConnectionRow.Trailing.none)
  }

  @Test("an agent whose state isn't known yet says so in the switch's place rather than showing it off")
  func anUnknownAgentSaysSo() {
    #expect(AgentConnectionRow.trailing(Self.agent(.note("Checking…"))) == .note("Checking…"))
    #expect(AgentConnectionRow.trailing(Self.agent(.note("Couldn't check"))) == .note("Couldn't check"))
  }

  @Test("a partly connected agent keeps its switch on to disconnect and offers Connect to finish")
  func aPartlyConnectedAgentOffersBoth() {
    let partly = Self.agent(.connect(isEnabled: true), canDisconnect: true)
    #expect(partly.isPartlyConnected)
    #expect(AgentConnectionRow.trailing(partly) == .toggle(isOn: true, isEnabled: true))
    #expect(AgentConnectionRow.canConnect(partly.connection))
    #expect(!Self.agent(.connected, canDisconnect: true).isPartlyConnected)
    #expect(!Self.agent(.connect(isEnabled: true)).isPartlyConnected)
  }

  @Test("while another agent connects, a partly connected one keeps its switch on, waiting, and still says so")
  func aPartialConnectionWaitsOn() {
    var partly = Self.agent(.connect(isEnabled: false), canDisconnect: false)
    partly.hasConnection = true
    #expect(partly.isPartlyConnected)
    #expect(AgentConnectionRow.trailing(partly) == .toggle(isOn: true, isEnabled: false))
    #expect(!AgentConnectionRow.canConnect(partly.connection))
  }

  @Test("VoiceOver hears a switch whose change runs as connecting or disconnecting, never as one merely waiting")
  func progressIsSpoken() {
    #expect(AgentConnectionRow.progress("Claude", isConnecting: true) == "Connecting Claude…")
    #expect(AgentConnectionRow.progress("Claude", isConnecting: false) == "Disconnecting Claude…")
  }
}

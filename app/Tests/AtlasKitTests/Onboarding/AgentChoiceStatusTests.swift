import Foundation
import Testing

@testable import AtlasKit

@Suite("AgentChoice+Status")
struct AgentChoiceStatusTests {
  /// One row of `atlas agents status`.
  ///
  /// - Parameters:
  ///   - agent: The agent's id.
  ///   - installed: Whether it is installed.
  ///   - connected: Whether it is connected.
  /// - Returns: The decoded row.
  /// - Throws: When the row does not decode, which a fixed reply never does.
  private func row(_ agent: String, installed: Bool, connected: Bool) throws -> AgentsStatus.Agent {
    let reply = #"{"agents": [{"agent": "\#(agent)", "installed": \#(installed), "connected": \#(connected)}]}"#
    let status = Outcome(ok: true, reply: Data(reply.utf8), message: "")
      .decode(AgentsStatus.self)
    return try #require(status?.agents.first)
  }

  @Test("a reported agent reads found and connected as reported, switched on when found; an unknown one is passed over")
  func merged() throws {
    let agents = AgentChoice.merged(
      AgentChoice.all,
      with: [try row("codex", installed: true, connected: false), try row("other", installed: true, connected: true)]
    )
    #expect(agents.map(\.id) == AgentID.allCases)
    let codex = try #require(agents.first { $0.id == .codex })
    #expect(codex.found && !codex.connected && codex.on)
    let claude = try #require(agents.first { $0.id == .claude })
    #expect(!claude.found && !claude.on, "an agent nothing was reported of stays off")
  }

  @Test("an agent reported installed and connected reads connected, and Continue leaves it out")
  func installedAndConnected() throws {
    let agents = AgentChoice.merged(AgentChoice.all, with: [try row("claude", installed: true, connected: true)])
    let claude = try #require(agents.first { $0.id == .claude })
    #expect(claude.found && claude.connected && claude.on)
    #expect(AgentChoice.toConnect(agents).isEmpty)
  }

  @Test("Continue installs only agents switched on, found and not connected yet")
  func toConnect() {
    var on = AgentChoice(id: .claude)
    on.found = true
    on.on = true
    var connected = AgentChoice(id: .codex)
    connected.found = true
    connected.on = true
    connected.connected = true
    #expect(AgentChoice.toConnect([on, connected]) == [.claude])
    on.found = false
    #expect(AgentChoice.toConnect([on]).isEmpty, "an agent not on this Mac cannot be installed")
  }
}

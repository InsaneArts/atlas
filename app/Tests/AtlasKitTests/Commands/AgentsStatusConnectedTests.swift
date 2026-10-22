import Foundation
import Testing

@testable import AtlasKit

@Suite("AgentsStatus+Connected")
struct AgentsStatusConnectedTests {
  @Test("only agents reported connected are listed, in order, and one this app does not know is passed over")
  func onlyConnectedAgentsAreListed() throws {
    let reply = Data(
      #"""
      {"agents": [{"agent": "codex", "installed": true, "connected": true},
      {"agent": "claude", "installed": true, "connected": false},
      {"agent": "other", "installed": true, "connected": true}]}
      """#.utf8
    )
    let status = try JSONDecoder().decode(AgentsStatus.self, from: reply)
    #expect(status.connected == [.codex])
  }
}

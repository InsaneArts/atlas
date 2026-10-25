import Foundation
import Testing

@testable import AtlasCommands

@Suite("Pi connections")
struct PiConnectionTests {
  @Test("Pi connects and disconnects through its own CLI selector")
  func selectors() {
    #expect(Action.installAgents([.pi]).arguments == ["agents", "install", "--pi"])
    #expect(Action.changeAgentParts(["pi"], remove: true).arguments == ["agents", "remove", "--pi"])
  }

  @Test("Pi status and changes stay independent of the grouped Claude connection")
  func independentStatus() throws {
    let status = try JSONDecoder()
      .decode(
        AgentsStatus.self,
        from: Data(
          #"{"agents":[{"agent":"claude","installed":true,"connected":true},{"agent":"claude-app","installed":true,"connected":false},{"agent":"pi","installed":true,"connected":true}]}"#
            .utf8
        )
      )
    #expect(status.grouped.map(\.agent) == ["claude", "pi"])
    #expect(status.grouped.first?.connected == false)
    #expect(status.grouped.last?.connected == true)
    let change = try JSONDecoder()
      .decode(
        AgentsChange.self,
        from: Data(#"{"agents":[{"agent":"pi","ok":true}]}"#.utf8)
      )
    #expect(change.connected == [.pi])
    #expect(!change.requiresRestart)
  }
}

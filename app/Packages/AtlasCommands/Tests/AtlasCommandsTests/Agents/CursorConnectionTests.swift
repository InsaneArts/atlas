import Foundation
import Testing

@testable import AtlasCommands

@Suite("Cursor connections")
struct CursorConnectionTests {
  @Test("Cursor preserves its selector and reports registration independently of host presence")
  func registration() throws {
    #expect(Action.installAgents([.cursor]).arguments == ["agents", "install", "--cursor"])
    #expect(Action.changeAgentParts(["cursor"], remove: true).arguments == ["agents", "remove", "--cursor"])
    let status = try JSONDecoder()
      .decode(
        AgentsStatus.self,
        from: Data(
          #"{"agents":[{"agent":"cursor","installed":false,"connected":true,"verification":"registration"}]}"#.utf8
        )
      )
    #expect(status.connected == [.cursor])
    #expect(status.grouped.first?.parts == ["cursor"])
    #expect(AgentsStatus.partTitle("cursor") == "Cursor")
    let change = try JSONDecoder()
      .decode(
        AgentsChange.self,
        from: Data(#"{"agents":[{"agent":"cursor","ok":true,"restart":true}]}"#.utf8)
      )
    #expect(change.restarting == [.cursor])
    #expect(change.connected == [.cursor])
    #expect(change.failures.isEmpty)
  }
}

import Foundation
import Testing

@testable import AtlasCommands

@Suite("opencode connections")
struct OpencodeConnectionTests {
  @Test("opencode uses its own selector and a restart result names the correct hosts")
  func selectorsAndRestarts() throws {
    #expect(Action.installAgents([.opencode, .pi]).arguments == ["agents", "install", "--opencode", "--pi"])
    #expect(Action.changeAgentParts(["opencode"], remove: true).arguments == ["agents", "remove", "--opencode"])
    let change = try JSONDecoder()
      .decode(
        AgentsChange.self,
        from: Data(
          #"{"agents":[{"agent":"claude-app","ok":true,"restart":true},{"agent":"opencode","ok":true,"restart":true},{"agent":"pi","ok":false,"restart":true}]}"#
            .utf8
        )
      )
    #expect(change.requiresRestart)
    #expect(change.restarting == [.claude, .opencode])
    #expect(change.connected == [.claude, .opencode])
    #expect(change.failures.first?.agent == .pi)
  }
}

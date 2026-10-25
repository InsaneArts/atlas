import Foundation
import Testing

@testable import AtlasCommands

@Suite("Claude connections")
struct ClaudeConnectionTests {
  @Test("agent changes decode an optional restart request", arguments: ["true", "false", "missing"])
  func decodesRestart(restart: String) throws {
    let field = restart == "missing" ? "" : ",\"restart\":\(restart)"
    let change = try JSONDecoder()
      .decode(
        AgentsChange.self,
        from: Data("{\"agents\":[{\"agent\":\"claude-app\",\"ok\":true\(field)}]}".utf8)
      )
    #expect(change.agents.first?.restart == (restart == "missing" ? nil : restart == "true"))
    #expect(change.requiresRestart == (restart == "true"))
  }

  @Test("an unsuccessful agent change does not request a restart")
  func failedChangeDoesNotRequestRestart() throws {
    let change = try JSONDecoder()
      .decode(
        AgentsChange.self,
        from: Data(#"{"agents":[{"agent":"claude-app","ok":false,"restart":true}]}"#.utf8)
      )
    #expect(!change.requiresRestart)
  }

  @Test("Claude is connected only when every installed part is connected", arguments: [false, true])
  func allPartsMustConnect(appConnected: Bool) throws {
    let status = try JSONDecoder()
      .decode(
        AgentsStatus.self,
        from: Data(
          """
          {"agents":[{"agent":"claude","installed":true,"connected":true},
          {"agent":"claude-app","installed":true,"connected":\(appConnected)},
          {"agent":"codex","installed":false}]}
          """.utf8
        )
      )
    #expect(status.grouped.map(\.agent) == ["claude", "codex"])
    #expect(status.grouped.first?.connected == appConnected)
    #expect(status.grouped.first?.parts == ["claude", "claude-app"])
    #expect(status.connected.contains(.claude) == appConnected)
  }

  @Test("one installed Claude part connects alone", arguments: ["claude", "claude-app"])
  func onePartConnectsAlone(part: String) throws {
    let status = try JSONDecoder()
      .decode(
        AgentsStatus.self,
        from: Data(
          """
          {"agents":[{"agent":"\(part)","installed":true,"connected":true}]}
          """.utf8
        )
      )
    #expect(status.grouped.first?.parts == [part])
    #expect(status.connected == [.claude])
  }

  @Test("neither installed Claude part is reported as connected")
  func neitherPartIsInstalled() throws {
    let status = try JSONDecoder()
      .decode(
        AgentsStatus.self,
        from: Data(#"{"agents":[{"agent":"claude","installed":false},{"agent":"claude-app","installed":false}]}"#.utf8)
      )
    #expect(status.grouped.first?.installed == false)
    #expect(status.grouped.first?.parts == [])
    #expect(status.connected.isEmpty)
  }

  @Test("a failed Claude part prevents a successful grouped install and names the failing part")
  func aFailedPartIsNamed() throws {
    let change = try JSONDecoder()
      .decode(
        AgentsChange.self,
        from: Data(
          #"{"agents":[{"agent":"claude","ok":true},{"agent":"claude-app","ok":false,"error":"settings file is not valid JSON"}]}"#
            .utf8
        )
      )
    #expect(change.connected.isEmpty)
    #expect(change.failures.first?.agent == .claude)
    #expect(change.failures.first?.error == "The Claude app: settings file is not valid JSON")
  }
}

import Foundation
import Testing

@testable import AtlasService

@Suite("ActivityLog.Entry")
struct ActivityLogEntryTests {
  @Test("an activity row with every field null reads as an agent's failed action")
  func allNull() throws {
    let log = try ViewerClient.activity(activityAllNull, response: reply(200))
    #expect(log.entries == [.init(id: "9f2c41d07a3b55e1", outcome: .failed, actor: .agent, operation: "action")])
  }

  @Test("an activity row's start is read as milliseconds since 1970")
  func startedAt() throws {
    let body = Data(#"{"activity": [{"id": "7", "startedAt": 1792665097600, "ageSeconds": 4}]}"#.utf8)
    let log = try ViewerClient.activity(body, response: reply(200))
    #expect(log.entries.first?.startedAt == Date(timeIntervalSince1970: 1_792_665_097.6))
  }

  @Test("an empty session key reads as Terminal's, as a missing one does, and the person's own rows have no group")
  func emptySessionKeyIsTerminal() throws {
    let body = Data(
      #"""
      {"activity": [
        {"id": "1", "actor": "agent", "session": ""},
        {"id": "2", "actor": "agent"},
        {"id": "3", "actor": "agent", "session": "claude:a1"},
        {"id": "4", "actor": "viewer"}
      ]}
      """#.utf8
    )
    let entries = try ViewerClient.activity(body, response: reply(200)).entries
    #expect(entries.map(\.session) == [nil, nil, "claude:a1", nil])
    #expect(entries.map(\.sessionKey) == ["", "", "claude:a1", ""])
    #expect(entries.map(\.groupKey) == ["", "", "claude:a1", nil])
    let scope = ActivityScope.session("")
    let terminal = entries.filter { scope.admits($0) }
    #expect(terminal.map(\.id) == ["1", "2"])
  }

  @Test("an activity row's verb, app, label, bundle and keys read as the service writes them")
  func namesWhatItDid() throws {
    let body = Data(
      #"""
      {"activity": [{"id": "1", "verb": "key", "app": "TextEdit", "label": "⇧⌘Z Page Up",
      "bundleId": "com.apple.TextEdit", "keys": [["⇧", "⌘", "Z"], ["Page Up"]]}]}
      """#.utf8
    )
    let entry = try #require(try ViewerClient.activity(body, response: reply(200)).entries.first)
    #expect(entry.verb == "key")
    #expect(entry.app == "TextEdit")
    #expect(entry.label == "⇧⌘Z Page Up")
    #expect(entry.bundleId == "com.apple.TextEdit")
    #expect(entry.keys == [["⇧", "⌘", "Z"], ["Page Up"]])
  }

  @Test("an activity row's verb, app, label, bundle and keys of the wrong type or missing read as none")
  func namesReadLeniently() throws {
    let body = Data(
      #"""
      {"activity": [
        {"id": "1", "verb": 3, "app": "", "label": ["Save"], "bundleId": null, "keys": "⌘S"},
        {"id": "2", "keys": [["⌘", 7]]},
        {"id": "3"}
      ]}
      """#.utf8
    )
    let entries = try ViewerClient.activity(body, response: reply(200)).entries
    #expect(entries.count == 3)
    for entry in entries {
      #expect(entry.verb == nil)
      #expect(entry.app == nil)
      #expect(entry.label == nil)
      #expect(entry.bundleId == nil)
      #expect(entry.keys.isEmpty)
    }
  }
}

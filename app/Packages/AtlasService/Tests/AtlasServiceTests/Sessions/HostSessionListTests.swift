import Foundation
import Testing

@testable import AtlasService

@Suite("HostSessionList")
struct HostSessionListTests {
  /// A `sessions.json` body decoded.
  ///
  /// - Parameter body: The body's JSON.
  /// - Returns: The list.
  /// - Throws: A decoding error when the body is not an object.
  func list(_ body: String) throws -> HostSessionList {
    try JSONDecoder()
      .decode(HostSessionList.self, from: Data(body.utf8))
  }

  @Test("an ended session reads what it made, and an odd entry is dropped while the rest are kept")
  func anEndedSessionReadsWhatItMade() throws {
    let reading = try list(
      """
      {"ok": true, "sessions": [
        {"key": "a1", "host": "pi", "hostLabel": "Pi", "name": "Pi · notes-sync", "folder": "notes-sync",
         "startedAt": 1793163000000, "lastSeenAt": 1793168100000, "endedAt": 1793168160000,
         "state": "ended", "actions": 6,
         "apps": [{"name": "Notes", "copied": false}, {"copied": true}, 3],
         "copies": [{"app": "Notes"}],
         "plans": [{"phase": "applied", "madeAt": 1793163300000, "canRollBack": true}, {"phase": "rolledSideways"}],
         "screenshots": ["1793163300000000000-capture.png", 4, ""]},
        {"host": "codex", "name": "Codex · opie"},
        "not a session"
      ]}
      """
    )
    let record = try #require(reading.sessions.first)
    #expect(reading.sessions.count == 1)
    #expect(record.state == .ended && record.endedAt == Date(timeIntervalSince1970: 1_793_168_160))
    #expect(record.apps == [HostSessionApp(name: "Notes")])
    #expect(record.copies == [HostSessionCopy(app: "Notes")])
    #expect(record.plans.map(\.phase) == [.applied, .unknown])
    #expect(record.plans.first?.canRollBack == true)
    #expect(record.screenshots == ["1793163300000000000-capture.png"])
  }

  @Test("a body with no list has no sessions")
  func aBodyWithNoListHasNone() throws {
    let reading = try list(#"{"ok": true, "sessions": "none"}"#)
    #expect(reading.sessions.isEmpty)
    #expect(reading.live.isEmpty, "an older service sends no live sessions")
  }

  @Test("every live session reads in the status's shape and order, an odd one dropped")
  func everyLiveSessionReads() throws {
    let reading = try list(
      """
      {"ok": true, "sessions": [], "live": [
        {"key": "b2", "host": "codex", "hostLabel": "Codex", "name": "Codex · opie", "state": "waiting",
         "waitingFor": "helper", "waitingSeconds": 4, "started": 1793163000000, "live": true},
        {"host": "pi"},
        {"key": "", "host": "none", "name": "Terminal", "state": "idle"}
      ]}
      """
    )
    let keys = reading.live.map(\.key)
    #expect(keys == ["b2", ""])
    let codex = try #require(reading.live.first)
    #expect(codex.state == .waiting && codex.waitingFor == .helper && codex.waitingSeconds == 4)
    #expect(codex.startedAt == Date(timeIntervalSince1970: 1_793_163_000))
  }
}

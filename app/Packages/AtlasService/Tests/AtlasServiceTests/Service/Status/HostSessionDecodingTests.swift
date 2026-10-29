import AtlasServiceTesting
import Foundation
import Testing

@testable import AtlasService

@Suite("ViewerStatus.HostSession")
struct HostSessionDecodingTests {
  @Test("a session missing its key is dropped and the rest are kept")
  func aSessionMissingItsKeyIsDropped() throws {
    var keyless = hostSession("x", host: "codex", name: "Codex · opie")
    keyless["key"] = nil
    let reading = try status([
      "overview": [
        "sessionsShown": true,
        "hostSessions": [
          hostSession("3f9a1c07b2e4", host: "claude", name: "Claude Code · understudy", state: "acting"),
          keyless,
          ["key": "8be20c4d1a77", "host": "codex", "name": ""],
          "not a session",
          hostSession("", host: "none", name: "Terminal"),
        ] as [Any],
      ]
    ])
    #expect(reading.overview.hostSessions.map(\.key) == ["3f9a1c07b2e4", ""], "Terminal's empty key is a key")
    #expect(reading.overview.hostSessions.first?.state == .acting)
    #expect(reading.overview.hostSessions.last?.host == .terminal)
  }

  @Test("an unknown host reads as other, and an unknown state as idle")
  func anUnknownHostReadsAsOther() throws {
    let reading = try status([
      "overview": [
        "hostSessions": [
          hostSession("a", host: "named", name: "Bot · scratch", state: "dancing"),
          ["key": "b", "name": "Quiet", "host": 7, "state": false, "waitingSeconds": "8"],
        ] as [Any]
      ]
    ])
    #expect(reading.overview.hostSessions.map(\.host) == [.other, .other])
    #expect(reading.overview.hostSessions.map(\.state) == [.idle, .idle])
    #expect(reading.overview.hostSessions.last?.waitingSeconds == nil, "a mistyped field takes its default")
  }

  @Test("a session reads its times, its wait, its newest action and what may not have finished")
  func aSessionReadsEveryField() throws {
    let reading = try status([
      "overview": [
        "holder": "",
        "moreSessions": 2,
        "hostSessions": [
          [
            "key": "8be20c4d1a77", "host": "codex", "hostLabel": "Codex", "name": "Codex · atlas-landing",
            "folder": "atlas-landing", "started": 1_793_170_410_000, "lastSeen": 1_793_170_420_000, "ended": NSNull(),
            "live": true, "state": "waiting", "waitingFor": "helper", "waitingSeconds": 8, "holdsHelper": false,
            "latest": ["says": "opened Notes", "ok": true, "ageSeconds": 31],
            "uncertainRow": "r9", "uncertainVerb": "type",
          ]
        ],
      ]
    ])
    let session = try #require(reading.overview.hostSessions.first)
    #expect(session.startedAt == Date(timeIntervalSince1970: 1_793_170_410))
    #expect(session.lastSeenAt == Date(timeIntervalSince1970: 1_793_170_420))
    #expect(session.endedAt == nil && session.isLive)
    #expect(session.waitingFor == .helper && session.waitingSeconds == 8)
    #expect(session.latest?.says == "opened Notes")
    #expect(session.uncertainRow == "r9" && session.uncertainVerb == "type")
    #expect(session.folder == "atlas-landing" && session.hostLabel == "Codex")
    #expect(reading.overview.holder == "", "Terminal can hold the helper")
    #expect(reading.overview.moreSessions == 2)
  }

  @Test("a status without sessions has none")
  func aStatusWithoutSessionsHasNone() throws {
    let reading = try status([
      "overview": ["mode": "agent", "activity": [["operation": "input", "ok": false, "says": "couldn't type"]]]
    ])
    #expect(reading.overview.hostSessions.isEmpty)
    #expect(!reading.overview.sessionsShown)
    #expect(reading.overview.holder == nil && reading.overview.moreSessions == 0)
    let row = try #require(reading.overview.activity.first)
    #expect(row.session == nil && !row.nothingSent)
  }

  @Test("a row names its session, and whether it sent nothing")
  func aRowNamesItsSession() throws {
    let reading = try status([
      "overview": [
        "activity": [
          ["operation": "input", "ok": false, "session": "3f9a1c07b2e4", "nothingSent": true],
          ["operation": "input", "ok": false, "uncertain": true],
        ]
      ]
    ])
    #expect(reading.overview.activity.map(\.session) == ["3f9a1c07b2e4", nil])
    #expect(reading.overview.activity.map(\.nothingSent) == [true, false])
    #expect(reading.overview.activity.map(\.outcome) == [.notSent, .failed])
  }

  @Test("one odd activity row is dropped and the rest are kept")
  func oneOddActivityRowIsDropped() throws {
    let reading = try status(["overview": ["activity": [["operation": "input", "ok": true], "not a row", 7]]])
    #expect(reading.overview.activity.map(\.operation) == ["input"])
  }
}

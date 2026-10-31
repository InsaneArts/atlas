import AtlasService
import AtlasServiceTesting
import Foundation
import Testing

@testable import AtlasStatus

@Suite("StatusReading+Sessions")
struct StatusReadingSessionsTests {
  /// Claude Code acting with the helper, Codex waiting its turn and Cursor idle.
  static var three: [[String: Any]] {
    [
      hostSession(
        "claude",
        host: "claude",
        name: "Claude Code · understudy",
        state: "acting",
        holdsHelper: true,
        latest: ["says": "clicking Show Center in Opie", "ok": true, "started": true, "ageSeconds": 0]
      ),
      hostSession(
        "codex",
        host: "codex",
        name: "Codex · atlas-landing",
        state: "waiting",
        waitingFor: "helper",
        waitingSeconds: 8
      ),
      hostSession(
        "cursor",
        host: "cursor",
        name: "Cursor · opie",
        latest: ["says": "opened Opie", "ok": true, "ageSeconds": 240]
      ),
    ]
  }

  /// A reading of an overview with these fields.
  ///
  /// - Parameter overview: The overview's fields.
  /// - Returns: The reading.
  /// - Throws: A decoding error, which a fixed reply never gives.
  func reading(_ overview: [String: Any]) throws -> StatusReading {
    StatusReading(try status(["overview": overview]))
  }

  @Test("sessions show only with two live")
  func sessionsShowOnlyWithTwoLive() throws {
    let one = try reading([
      "sessionsShown": false, "holder": "claude", "hostSessions": [Self.three[0]],
    ])
    #expect(!one.showsSessions)
    #expect(one.holder == nil, "with one session no one is named")
    let many = try reading([
      "sessionsShown": true, "holder": "claude", "moreSessions": 2,
      "hostSessions": Self.three + [hostSession("pi", host: "pi", name: "Pi · notes-sync", state: "ended")],
    ])
    #expect(many.showsSessions)
    #expect(many.liveSessions.map(\.key) == ["claude", "codex", "cursor"], "an ended session is not live")
    #expect(many.holder?.name == "Claude Code · understudy")
    #expect(many.sessionsPast(2) == 3, "the third live one and the two the service did not list")
    #expect(many.session("")?.name == nil)
    #expect(many.session("pi")?.state == .ended)
    #expect(try reading([:]).liveSessions.isEmpty)
  }

  @Test("the panel lists its sessions while one is live, and names who has the helper only from two")
  func thePanelListsOneLiveSession() throws {
    let one = try reading(["sessionsShown": false, "holder": "claude", "hostSessions": [Self.three[0]]])
    #expect(one.listsSessions)
    #expect(!one.showsSessions && one.holder == nil)
    let ended = try reading([
      "hostSessions": [hostSession("pi", host: "pi", name: "Pi · notes-sync", state: "ended")]
    ])
    #expect(!ended.listsSessions, "an ended session is not live")
    #expect(try !reading([:]).listsSessions)
  }

  @Test("the causer is the session whose action is unknown")
  func theCauserIsTheSessionWhoseActionIsUnknown() throws {
    let needing = try reading([
      "sessionsShown": true, "pending": 1,
      "hostSessions": [
        hostSession(
          "claude",
          host: "claude",
          name: "Claude Code · understudy",
          state: "needsYou",
          uncertainRow: "r9",
          uncertainVerb: "click"
        ),
        hostSession("codex", host: "codex", name: "Codex · atlas-landing", state: "waiting", waitingFor: "recovery"),
      ],
    ])
    #expect(needing.causer?.key == "claude")
    #expect(needing.holder == nil, "no one has the helper while it needs the person")
    let killed = try reading([
      "sessionsShown": true, "pending": 1,
      "hostSessions": [
        hostSession("claude", host: "claude", name: "Claude Code · understudy", state: "waiting"),
        hostSession("codex", host: "codex", name: "Codex · atlas-landing", state: "waiting"),
      ],
    ])
    #expect(killed.causer == nil, "a command killed without a row blames no session")
    #expect(killed.liveSessions.map(killed.line(of:)) == [.waitingForRecovery, .waitingForRecovery])
  }

  @Test("one session exposes its unresolved row without drawing a session label, only while recovery is pending")
  func oneSessionExposesItsUnresolvedRow() throws {
    for pending in [0, 1] {
      let reading = try reading([
        "sessionsShown": false, "pending": pending,
        "hostSessions": [
          hostSession("claude", host: "claude", name: "Claude Code", state: "needsYou", uncertainRow: "r9")
        ],
      ])
      #expect(reading.causer == nil)
      #expect(reading.uncertainRow == (pending == 1 ? "r9" : nil))
    }
  }

  @Test("each state has its line")
  func eachStateHasItsLine() throws {
    let ended = Date(timeIntervalSince1970: 1_793_168_160)
    let reading = try reading([
      "sessionsShown": true,
      "hostSessions": Self.three + [
        hostSession("pi", host: "pi", name: "Pi · notes-sync", state: "ended")
          .merging(["ended": 1_793_168_160_000]) { _, new in new },
        hostSession("quiet", host: "codex", name: "Codex · opie", state: "idle"),
        hostSession("cause", host: "claude", name: "Claude Code · opie", state: "needsYou", uncertainVerb: "type"),
      ],
    ])
    let lines = reading.status.overview.hostSessions.map(reading.line(of:))
    #expect(
      lines == [
        .acting(says: "clicking Show Center in Opie"),
        .waitingForHelper(seconds: 8),
        .last(says: "opened Opie", age: 240),
        .ended(at: ended),
        .none,
        .mayNotHaveFinished(verb: "type"),
      ]
    )
  }

  @Test("a row's session is its item's")
  func aRowsSessionIsItsItems() throws {
    let reading = try reading(["activity": [["operation": "input", "ok": true, "session": "codex"]]])
    let sessions = reading.activity()
      .map(\.session)
    #expect(sessions == ["codex"])
  }

  @Test("a row refused before anything was sent is not a failure, and one that went wrong is")
  func aRefusedRowIsNotAFailure() throws {
    let reading = try reading([
      "activity": [
        ["operation": "input", "ok": false, "nothingSent": true],
        ["operation": "input", "ok": false],
        ["operation": "input", "ok": true],
      ]
    ])
    let failed = reading.activity()
      .map(\.failed)
    #expect(failed == [false, true, false])
  }
  @Test("an ended causer stays relevant without being live or naming a holder")
  func anEndedCauserStaysRelevantWithoutBeingLive() throws {
    let ended: [String: Any] = [
      "key": "gone", "name": "Codex", "state": "needsYou", "ended": 1000, "live": true, "holdsHelper": true,
      "uncertainRow": "row", "uncertainVerb": "click",
    ]
    let reading = try reading([
      "sessionsShown": true, "holder": "gone", "pending": 1, "hostSessions": [ended, Self.three[2], ended],
    ])
    #expect(reading.liveSessions.count == 1)
    #expect(reading.relevantSessions.map(\.key) == ["gone", "cursor"])
    #expect(reading.listsSessions && reading.sessionsPast(1) == 1)
    #expect(reading.holder == nil)
    #expect(reading.causer?.key == "gone" && reading.uncertainRow == "row")
    #expect(reading.line(of: reading.relevantSessions[0]) == .mayNotHaveFinished(verb: "click"))
  }

  @Test("an explicitly ended working session remains relevant without becoming a holder or causer")
  func anExplicitlyEndedWorkingSessionRemainsRelevant() throws {
    let work: [String: Any] = [
      "key": "gone", "name": "Codex", "state": "acting", "ended": 1000, "live": true, "holdsHelper": true,
      "workInProgress": true,
    ]
    let reading = try reading(["sessionsShown": true, "holder": "gone", "pending": 1, "hostSessions": [work]])
    #expect(reading.liveSessions.isEmpty)
    #expect(reading.relevantSessions.count == 1 && reading.listsSessions)
    #expect(reading.holder == nil && reading.causer == nil)
    #expect(reading.line(of: reading.relevantSessions[0]) == .acting(says: nil))
  }

}

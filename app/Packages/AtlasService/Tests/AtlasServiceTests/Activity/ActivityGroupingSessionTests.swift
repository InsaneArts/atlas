import AtlasServiceTesting
import Foundation
import Testing

@testable import AtlasService

@Suite("ActivityGrouping, by host session")
struct ActivityGroupingSessionTests {
  @Test("Terminal's moment scope excludes the person's own rows")
  func terminalScopeExcludesThePerson() {
    let rules = ActivityFixtures.rules([
      ActivityFixtures.row(id: "t1", says: "took a screenshot", ageSeconds: 1, session: nil),
      ActivityFixtures.row(id: "y1", says: "clicked in Opie", actor: .you, ageSeconds: 2, session: nil),
      ActivityFixtures.row(id: "a1", says: "clicked Save", ageSeconds: 4, session: "claude"),
    ])
    let terminal = ActivityMoments(rules, captures: [], scope: .session(""))
      .shown(filter: .all, query: "")
      .flatMap(\.runs)
      .map(\.id)
    #expect(terminal == ["t1"])
  }

  @Test("a refused row is never a problem")
  func aRefusedRowIsNeverAProblem() throws {
    let data = Data(
      """
      {"activity": [{"id": "r1", "ok": false, "nothingSent": true, "says": "didn't type in Notes"},
      {"id": "r2", "ok": false, "says": "couldn't click Save"}]}
      """.utf8
    )
    let log = try JSONDecoder()
      .decode(ActivityLog.self, from: data)
    #expect(log.entries.map(\.outcome) == [.notSent, .failed])
    #expect(ActivityAction(log.entries[0]) == .typing)
    let rules = ActivityGrouping(
      log: log,
      readAt: ActivityFixtures.readAt,
      now: ActivityFixtures.readAt,
      calendar: ActivityFixtures.calendar
    )
    let problems = ActivityMoments(rules, captures: [], scope: .all)
      .shown(filter: .problems, query: "")
      .flatMap(\.runs)
      .map(\.id)
    #expect(problems == ["r2"])
  }

  @Test("a screenshot row takes its own capture before one near its time")
  func aScreenshotRowTakesItsOwnCapture() {
    let started = ActivityFixtures.readAt
      .addingTimeInterval(-20)
    let near = Screenshot(name: "near.png", takenAt: started)
    let own = Screenshot(name: "own.png", takenAt: started.addingTimeInterval(-60))
    let rules = ActivityFixtures.rules([
      ActivityFixtures.row(
        id: "s1",
        says: "took a screenshot",
        ageSeconds: 20,
        operation: "screenshot",
        capture: "own.png"
      ),
      ActivityFixtures.row(id: "s2", says: "took a screenshot", ageSeconds: 20, operation: "screenshot"),
    ])
    let shots = rules.shots(in: [near, own])
    #expect(shots == ["s1": "own.png", "s2": "near.png"])
  }

  @Test("a screenshot row whose named capture is gone takes none near its time")
  func aMissingNamedCaptureStaysMissing() {
    let started = ActivityFixtures.readAt
      .addingTimeInterval(-20)
    let near = Screenshot(name: "near.png", takenAt: started)
    let rules = ActivityFixtures.rules([
      ActivityFixtures.row(
        id: "s1",
        says: "took a screenshot",
        ageSeconds: 20,
        operation: "screenshot",
        capture: "pruned.png"
      )
    ])
    #expect(rules.shots(in: [near]).isEmpty)
  }

  @Test("repeats from two sessions stay two lines")
  func repeatsFromTwoSessionsStayTwoLines() {
    let rules = ActivityFixtures.rules([
      ActivityFixtures.row(id: "c1", says: "scrolled in Opie", ageSeconds: 1, session: "codex"),
      ActivityFixtures.row(id: "a1", says: "scrolled in Opie", ageSeconds: 2, session: "claude"),
      ActivityFixtures.row(id: "a2", says: "scrolled in Opie", ageSeconds: 3, session: "claude"),
    ])
    let lines = ActivityMoments(rules, captures: [], scope: .all)
      .shown(filter: .all, query: "")
      .flatMap(\.runs)
    #expect(lines.map(\.id) == ["c1", "a2"])
    #expect(lines.map(\.count) == [1, 2])
  }

  @Test("one session's count today leaves out the other sessions' rows and the person's")
  func oneSessionsCountLeavesOutTheRest() {
    let rules = ActivityFixtures.rules([
      ActivityFixtures.row(id: "a1", says: "clicked in Opie", ageSeconds: 1, session: "claude"),
      ActivityFixtures.row(id: "c1", says: "clicked in Opie", ageSeconds: 2, session: "codex"),
      ActivityFixtures.row(id: "y1", says: "clicked in Opie", actor: .you, ageSeconds: 3, session: nil),
      ActivityFixtures.row(id: "a2", says: "clicked in Opie", ageSeconds: 4, session: "claude"),
      ActivityFixtures.row(id: "t1", says: "clicked in Opie", ageSeconds: 5, session: nil),
    ])
    #expect(rules.countToday == 5)
    #expect(rules.countToday(in: .session("claude")) == 2)
    #expect(rules.countToday(in: .session("")) == 1, "Terminal's are the agents' rows no host named")
  }
}

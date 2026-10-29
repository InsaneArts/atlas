import Foundation
import Testing

@testable import AtlasService

@Suite("ActivityGrouping, by host session")
struct ActivityGroupingSessionTests {
  /// The rules over `entries`, read and looked at 10:31:42 on 22 October 2026, in UTC.
  ///
  /// - Parameter entries: The log's rows, newest first.
  /// - Returns: The rules.
  static func rules(_ entries: [ActivityLog.Entry]) -> ActivityGrouping {
    ActivityGroupingTests.rules(entries)
  }

  /// An agent's row in a session, `age` seconds old.
  ///
  /// - Parameters:
  ///   - id: Its id.
  ///   - session: Its session's key; nil for none.
  ///   - age: Its age, in seconds.
  ///   - says: What it did.
  ///   - outcome: How it went.
  ///   - actor: Who did it.
  /// - Returns: The row.
  static func row(
    _ id: String,
    _ session: String?,
    _ age: Double,
    says: String = "clicked in Opie",
    outcome: ActivityLog.Outcome = .worked,
    actor: ActivityLog.Actor = .agent
  ) -> ActivityLog.Entry {
    ActivityLog.Entry(id: id, says: says, outcome: outcome, actor: actor, ageSeconds: age, session: session)
  }

  @Test("rows group by session in the order the status gives")
  func rowsGroupInTheStatusOrder() {
    let rules = Self.rules([
      Self.row("c1", "codex", 1, says: "typed in Notes"),
      Self.row("a1", "claude", 2, says: "clicked Save"),
      Self.row("c2", "codex", 3, says: "opened Notes"),
      Self.row("a2", "claude", 4, says: "scrolled"),
      Self.row("a3", "claude", 5, says: "pressed a key"),
      Self.row("a4", "claude", 6, says: "opened Opie"),
      Self.row("a5", "claude", 7, says: "dragged"),
    ])
    let groups = rules.sessionGroups(filter: .all, query: "", captures: [], order: ["claude", "codex"], lines: 4)
    #expect(groups.map(\.key) == ["claude", "codex"])
    #expect(groups.first?.runs.map(\.id) == ["a1", "a2", "a3", "a4"], "the newest four lines")
    #expect(groups.map(\.total) == [5, 2])
  }

  @Test("the person's rows come after the agents' and Terminal last")
  func thePersonsRowsComeAfterTheAgents() {
    let rules = Self.rules([
      Self.row("t1", nil, 1, says: "took a screenshot"),
      Self.row("y1", nil, 2, says: "clicked in Opie", actor: .you),
      Self.row("e1", "ended", 3, says: "opened Notes"),
      Self.row("a1", "claude", 4, says: "clicked Save"),
    ])
    let groups = rules.sessionGroups(filter: .all, query: "", captures: [], order: ["claude"], lines: 4)
    #expect(groups.map(\.key) == ["claude", "ended", nil, ""], "a session the status no longer lists follows")
    let terminal = rules.groups(filter: .all, query: "", captures: [], scope: .session(""))
      .flatMap(\.runs)
      .map(\.id)
    #expect(terminal == ["t1"], "Terminal's scope holds the agents' rows no host named, never the person's")
  }

  @Test("Terminal keeps the status's place for it, before the sessions it does not list and the person's rows")
  func terminalKeepsTheStatusPlace() {
    let rules = Self.rules([
      Self.row("a1", "claude", 1, says: "clicked Save"),
      Self.row("y1", nil, 2, says: "clicked in Opie", actor: .you),
      Self.row("e1", "ended", 3, says: "opened Notes"),
      Self.row("t1", nil, 4, says: "took a screenshot"),
    ])
    let groups = rules.sessionGroups(filter: .all, query: "", captures: [], order: ["", "claude"], lines: 4)
    #expect(groups.map(\.key) == ["", "claude", "ended", nil])
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
    #expect(ActivityAction(log.entries[0]) == .keys)
    let rules = ActivityGrouping(
      log: log,
      readAt: ActivityGroupingTests.readAt,
      now: ActivityGroupingTests.readAt,
      calendar: ActivityGroupingTests.calendar
    )
    let problems = rules.groups(filter: .problems, query: "", captures: [])
      .flatMap(\.runs)
      .map(\.id)
    #expect(problems == ["r2"])
  }

  @Test("a screenshot row takes its own capture before one near its time")
  func aScreenshotRowTakesItsOwnCapture() {
    let started = ActivityGroupingTests.readAt
      .addingTimeInterval(-20)
    let near = Screenshot(name: "near.png", takenAt: started)
    let own = Screenshot(name: "own.png", takenAt: started.addingTimeInterval(-60))
    let rules = Self.rules([
      ActivityLog.Entry(
        id: "s1",
        says: "took a screenshot",
        ageSeconds: 20,
        operation: "screenshot",
        capture: "own.png"
      ),
      ActivityLog.Entry(id: "s2", says: "took a screenshot", ageSeconds: 20, operation: "screenshot"),
    ])
    let shots = rules.shots(in: [near, own])
    #expect(shots == ["s1": "own.png", "s2": "near.png"])
  }

  @Test("a screenshot row whose named capture is gone takes none near its time")
  func aMissingNamedCaptureStaysMissing() {
    let started = ActivityGroupingTests.readAt
      .addingTimeInterval(-20)
    let near = Screenshot(name: "near.png", takenAt: started)
    let rules = Self.rules([
      ActivityLog.Entry(
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
    let rules = Self.rules([
      Self.row("c1", "codex", 1, says: "scrolled in Opie"),
      Self.row("a1", "claude", 2, says: "scrolled in Opie"),
      Self.row("a2", "claude", 3, says: "scrolled in Opie"),
    ])
    let lines = rules.groups(filter: .all, query: "", captures: [])
      .flatMap(\.runs)
    #expect(lines.map(\.id) == ["c1", "a2"])
    #expect(lines.map(\.count) == [1, 2])
  }

  @Test("one session's count today leaves out the other sessions' rows and the person's")
  func oneSessionsCountLeavesOutTheRest() {
    let rules = Self.rules([
      Self.row("a1", "claude", 1),
      Self.row("c1", "codex", 2),
      Self.row("y1", nil, 3, actor: .you),
      Self.row("a2", "claude", 4),
      Self.row("t1", nil, 5),
    ])
    #expect(rules.countToday == 5)
    #expect(rules.countToday(in: .session("claude")) == 2)
    #expect(rules.countToday(in: .session("")) == 1, "Terminal's are the agents' rows no host named")
  }
}

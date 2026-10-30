import Foundation
import Testing

@testable import AtlasService

@Suite("ActivityGrouping")
struct ActivityGroupingTests {
  /// 10:31:42 on 22 October 2026, in UTC: when every log here is read.
  static let readAt = Date(timeIntervalSince1970: 1_792_665_102)

  /// A Gregorian calendar in UTC, so a day is the same day on any Mac.
  static let calendar: Calendar = {
    var calendar = Calendar(identifier: .gregorian)
    calendar.timeZone = TimeZone(secondsFromGMT: 0) ?? .current
    return calendar
  }()

  /// The rules over `entries`, read and looked at 10:31:42.
  ///
  /// - Parameter entries: The log's rows, newest first.
  /// - Returns: The rules.
  static func rules(_ entries: [ActivityLog.Entry]) -> ActivityGrouping {
    ActivityGrouping(log: ActivityLog(entries: entries), readAt: readAt, now: readAt, calendar: calendar)
  }

  /// A row `age` seconds old, or with no age.
  ///
  /// - Parameters:
  ///   - id: Its id.
  ///   - age: Its age, in seconds; nil for none.
  ///   - says: What it did.
  ///   - outcome: How it went.
  ///   - actor: Who did it.
  ///   - operation: Its operation.
  /// - Returns: The row.
  static func row(
    _ id: String,
    _ age: Double?,
    says: String = "clicked in Opie",
    outcome: ActivityLog.Outcome = .worked,
    actor: ActivityLog.Actor = .agent,
    operation: String = "input"
  ) -> ActivityLog.Entry {
    ActivityLog.Entry(id: id, says: says, outcome: outcome, actor: actor, ageSeconds: age, operation: operation)
  }

  @Test("Problems shows only rows that did not work")
  func problemsShowsOnlyFailures() {
    let rules = Self.rules([
      Self.row("running", 1, says: "turning Show Center off", outcome: .running),
      Self.row("failed", 5, says: "couldn't click Save", outcome: .failed),
      Self.row("worked", 9, says: "clicked Ring"),
      Self.row("yours", 12, says: "couldn't type", outcome: .failed, actor: .you),
    ])
    let shown = ActivityMoments(rules, captures: [], scope: .all)
      .shown(filter: .problems, query: "")
      .flatMap(\.runs)
      .map(\.id)
    #expect(shown == ["failed", "yours"])
  }

  @Test("only screenshot rows show a screenshot")
  func onlyScreenshotRowsShowOne() {
    let capture = Screenshot(name: "1792665080000000000-capture.png", takenAt: Self.readAt.addingTimeInterval(-22.4))
    let rules = Self.rules([
      Self.row("click", 22, says: "clicked Ring"),
      Self.row("shot", 22, says: "took a screenshot", operation: "screenshot"),
      Self.row("failed shot", 22, says: "couldn't take a screenshot", outcome: .failed, operation: "screenshot"),
    ])
    #expect(rules.shots(in: [capture]) == ["shot": capture.name])
  }

  @Test("each capture goes to the nearest screenshot row within three seconds, and to one row only")
  func capturesGoToTheirNearestRow() {
    let newer = Screenshot(name: "2-capture.png", takenAt: Self.readAt.addingTimeInterval(-10))
    let older = Screenshot(name: "1-capture.png", takenAt: Self.readAt.addingTimeInterval(-11))
    let far = Screenshot(name: "0-capture.png", takenAt: Self.readAt.addingTimeInterval(-40))
    let rules = Self.rules([
      Self.row("newer", 10, says: "took a screenshot", operation: "screenshot"),
      Self.row("older", 11, says: "took a screenshot", operation: "screenshot"),
      Self.row("alone", 30, says: "took a screenshot", operation: "screenshot"),
    ])
    #expect(rules.shots(in: [newer, older, far]) == ["newer": newer.name, "older": older.name])
  }

  @Test("the same action repeated reads as one row with a count")
  func repeatsCollapse() {
    let rules = Self.rules([
      Self.row("running", 100, says: "clicked in Opie", outcome: .running, actor: .you),
      Self.row("3", 140, actor: .you),
      Self.row("2", 150, actor: .you),
      Self.row("1", 157, actor: .you),
      Self.row("agent", 160),
      Self.row("0", 170, actor: .you),
    ])
    let runs = ActivityMoments(rules, captures: [], scope: .all)
      .shown(filter: .all, query: "")
      .flatMap(\.runs)
    #expect(runs.map(\.id) == ["running", "1", "0", "agent"])
    #expect(runs.map(\.count) == [1, 3, 1, 1])
    #expect(runs[1].startedAt == Self.readAt.addingTimeInterval(-157), "a run starts when its first row did")
  }

  @Test(
    "actions with another between them never read as one, whatever the search or filter hides",
    arguments: [
      // A search for the clicks, with a scroll between them.
      (
        [
          row("2", 5, says: "clicked in Opie"), row("1", 6, says: "scrolled in Opie"),
          row("0", 7, says: "clicked in Opie"),
        ],
        ActivityFilter.all,
        "clicked"
      ),
      // The same typing twice, with a scroll between them.
      (
        [
          row("2", 5, says: "typed in Opie"), row("1", 6, says: "scrolled in Opie"), row("0", 7, says: "typed in Opie"),
        ],
        .typing,
        ""
      ),
      // The same failure twice, with an action that worked between them.
      (
        [
          row("2", 5, says: "couldn't click Save", outcome: .failed), row("1", 6, says: "clicked Ring"),
          row("0", 7, says: "couldn't click Save", outcome: .failed),
        ],
        .problems,
        ""
      ),
    ] as [([ActivityLog.Entry], ActivityFilter, String)]
  )
  func hiddenGapsKeepRunsApart(entries: [ActivityLog.Entry], filter: ActivityFilter, query: String) {
    let runs = ActivityMoments(Self.rules(entries), captures: [], scope: .all)
      .shown(filter: filter, query: query)
      .flatMap(\.runs)
    #expect(runs.map(\.id) == ["2", "0"])
    #expect(runs.map(\.count) == [1, 1])
  }

  @Test("search matches what was done, not who did it")
  func searchLooksAtWhatWasDone() {
    let rules = Self.rules([
      Self.row("yours", 5, says: "clicked in Opie", actor: .you),
      Self.row("agent", 9, says: "scrolled in Safari"),
    ])
    let found = { (query: String) in
      ActivityMoments(rules, captures: [], scope: .all)
        .shown(filter: .all, query: query)
        .flatMap(\.runs)
        .map(\.id)
    }
    #expect(found("you").isEmpty)
    #expect(found(" opie ") == ["yours"])
    #expect(found("") == ["yours", "agent"])
  }

  @Test("today's count holds every action started today, whatever repeats or filters show")
  func countTodayCountsActions() {
    let rules = Self.rules([
      Self.row("3", 140, actor: .you),
      Self.row("2", 150, actor: .you),
      Self.row("1", 157, actor: .you),
      Self.row("yesterday", 12 * 3600),
    ])
    #expect(rules.countToday == 3)
  }

  @Test("a row of the person's input holding many actions counts the same in the day's total and in its moment")
  func personalInputCountsTheSameInTheDayAndItsMoment() throws {
    let rules = Self.rules([
      ActivityLog.Entry(id: "personal", actor: .you, ageSeconds: 0, count: 2107)
    ])
    let moment = try #require(ActivityMoments(rules, captures: [], scope: .all).moments.first)
    #expect(rules.countToday == 2107)
    #expect(moment.actions == rules.countToday)
  }

  @Test("today's action total preserves repeats and defaults missing or invalid counts to one")
  func repeatsAndDefaultCountsUseActions() throws {
    let rules = Self.rules([
      ActivityLog.Entry(id: "newer", ageSeconds: 0, count: 7),
      ActivityLog.Entry(id: "older", ageSeconds: 1, count: 3),
      ActivityLog.Entry(id: "missing", ageSeconds: 2),
      ActivityLog.Entry(id: "zero", ageSeconds: 3, count: 0),
      ActivityLog.Entry(id: "negative", ageSeconds: 4, count: -2),
    ])
    let moment = try #require(ActivityMoments(rules, captures: [], scope: .all).moments.first)
    #expect(moment.runs.count == 1)
    #expect(moment.runs.first?.count == 5)
    #expect(rules.countToday == 13)
    #expect(moment.actions == rules.countToday)
  }

  @Test("today's action total counts only dated rows in the chosen session")
  func actionTotalsKeepTheirDateAndSessionScope() {
    let rules = Self.rules([
      ActivityLog.Entry(id: "chosen", ageSeconds: 0, count: 7, session: "chosen"),
      ActivityLog.Entry(id: "other", ageSeconds: 1, count: 3, session: "other"),
      ActivityLog.Entry(id: "personal", actor: .you, ageSeconds: 2, count: 2107),
      ActivityLog.Entry(id: "yesterday", ageSeconds: 12 * 3600, count: 100, session: "chosen"),
      ActivityLog.Entry(id: "undated", count: 200, session: "chosen"),
    ])
    #expect(rules.countToday == 2117)
    #expect(rules.countToday(in: .session("chosen")) == 7)
    #expect(rules.countToday(in: .session("other")) == 3)
    #expect(rules.countToday(in: .session("")) == 0)
  }

  @Test(
    "daily, repeated-run and moment action totals saturate at the integer boundary",
    arguments: [Int.max - 1, Int.max],
    [false, true]
  )
  func actionTotalsSaturate(count: Int, repeats: Bool) throws {
    let body = Data(
      """
      {"activity": [
        {"id": "newer", "ageSeconds": 0, "count": \(count), "says": "clicked"},
        {"id": "older", "ageSeconds": 1, "count": 1, "says": "\(repeats ? "clicked" : "scrolled")"},
        {"id": "oldest", "ageSeconds": 2, "count": 1, "says": "\(repeats ? "clicked" : "typed")"}
      ]}
      """.utf8
    )
    let log = try ViewerClient.activity(body, response: reply(200))
    let rules = Self.rules(log.entries)
    let moment = try #require(ActivityMoments(rules, captures: [], scope: .all).moments.first)
    #expect(rules.countToday == .max)
    #expect(moment.actions == .max)
    #expect(moment.runs.count == (repeats ? 1 : 3))
    #expect(moment.runs.first?.actions == (repeats ? .max : count))
  }

  @Test("a row's time is the same at every reading when the service gives its start")
  func startHoldsAcrossReadings() {
    let started = Self.readAt
      .addingTimeInterval(-4.4)
    let first = ActivityLog.Entry(id: "1", ageSeconds: 4, startedAt: started)
    let second = ActivityLog.Entry(id: "1", ageSeconds: 6, startedAt: started)
    let later = ActivityGrouping(
      log: ActivityLog(entries: [second]),
      readAt: Self.readAt.addingTimeInterval(1.6),
      now: Self.readAt,
      calendar: Self.calendar
    )
    #expect(Self.rules([first]).start(of: first) == started)
    #expect(later.start(of: second) == started)
  }
}

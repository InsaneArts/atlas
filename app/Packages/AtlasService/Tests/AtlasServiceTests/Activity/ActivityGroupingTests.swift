import AtlasServiceTesting
import Foundation
import Testing

@testable import AtlasService

@Suite("ActivityGrouping")
struct ActivityGroupingTests {
  @Test("Problems shows only rows that did not work")
  func problemsShowsOnlyFailures() {
    let rules = ActivityFixtures.rules([
      ActivityFixtures.row(id: "running", says: "turning Show Center off", outcome: .running, ageSeconds: 1),
      ActivityFixtures.row(id: "failed", says: "couldn't click Save", outcome: .failed, ageSeconds: 5),
      ActivityFixtures.row(id: "worked", says: "clicked Ring", ageSeconds: 9),
      ActivityFixtures.row(id: "yours", says: "couldn't type", outcome: .failed, actor: .you, ageSeconds: 12),
    ])
    let shown = ActivityMoments(rules, captures: [], scope: .all)
      .shown(filter: .problems, query: "")
      .flatMap(\.runs)
      .map(\.id)
    #expect(shown == ["failed", "yours"])
  }

  @Test("only screenshot rows show a screenshot")
  func onlyScreenshotRowsShowOne() {
    let capture = Screenshot(
      name: "1792665080000000000-capture.png",
      takenAt: ActivityFixtures.readAt.addingTimeInterval(-22.4)
    )
    let rules = ActivityFixtures.rules([
      ActivityFixtures.row(id: "click", says: "clicked Ring", ageSeconds: 22),
      ActivityFixtures.row(id: "shot", says: "took a screenshot", ageSeconds: 22, operation: "screenshot"),
      ActivityFixtures.row(
        id: "failed shot",
        says: "couldn't take a screenshot",
        outcome: .failed,
        ageSeconds: 22,
        operation: "screenshot"
      ),
    ])
    #expect(rules.shots(in: [capture]) == ["shot": capture.name])
  }

  @Test("each capture goes to the nearest screenshot row within three seconds, and to one row only")
  func capturesGoToTheirNearestRow() {
    let newer = Screenshot(name: "2-capture.png", takenAt: ActivityFixtures.readAt.addingTimeInterval(-10))
    let older = Screenshot(name: "1-capture.png", takenAt: ActivityFixtures.readAt.addingTimeInterval(-11))
    let far = Screenshot(name: "0-capture.png", takenAt: ActivityFixtures.readAt.addingTimeInterval(-40))
    let rules = ActivityFixtures.rules([
      ActivityFixtures.row(id: "newer", says: "took a screenshot", ageSeconds: 10, operation: "screenshot"),
      ActivityFixtures.row(id: "older", says: "took a screenshot", ageSeconds: 11, operation: "screenshot"),
      ActivityFixtures.row(id: "alone", says: "took a screenshot", ageSeconds: 30, operation: "screenshot"),
    ])
    #expect(rules.shots(in: [newer, older, far]) == ["newer": newer.name, "older": older.name])
  }

  @Test("the same action repeated reads as one row with a count")
  func repeatsCollapse() {
    let rules = ActivityFixtures.rules([
      ActivityFixtures.row(id: "running", says: "clicked in Opie", outcome: .running, actor: .you, ageSeconds: 100),
      ActivityFixtures.row(id: "3", says: "clicked in Opie", actor: .you, ageSeconds: 140),
      ActivityFixtures.row(id: "2", says: "clicked in Opie", actor: .you, ageSeconds: 150),
      ActivityFixtures.row(id: "1", says: "clicked in Opie", actor: .you, ageSeconds: 157),
      ActivityFixtures.row(id: "agent", says: "clicked in Opie", ageSeconds: 160),
      ActivityFixtures.row(id: "0", says: "clicked in Opie", actor: .you, ageSeconds: 170),
    ])
    let runs = ActivityMoments(rules, captures: [], scope: .all)
      .shown(filter: .all, query: "")
      .flatMap(\.runs)
    #expect(runs.map(\.id) == ["running", "1", "0", "agent"])
    #expect(runs.map(\.count) == [1, 3, 1, 1])
    #expect(
      runs[1].startedAt == ActivityFixtures.readAt.addingTimeInterval(-157),
      "a run starts when its first row did"
    )
  }

  @Test(
    "actions with another between them never read as one, whatever the search or filter hides",
    arguments: [
      // A search for the clicks, with a scroll between them.
      (
        [
          ActivityFixtures.row(id: "2", says: "clicked in Opie", ageSeconds: 5),
          ActivityFixtures.row(id: "1", says: "scrolled in Opie", ageSeconds: 6),
          ActivityFixtures.row(id: "0", says: "clicked in Opie", ageSeconds: 7),
        ],
        ActivityFilter.all,
        "clicked"
      ),
      // The same typing twice, with a scroll between them.
      (
        [
          ActivityFixtures.row(id: "2", says: "typed in Opie", ageSeconds: 5),
          ActivityFixtures.row(id: "1", says: "scrolled in Opie", ageSeconds: 6),
          ActivityFixtures.row(id: "0", says: "typed in Opie", ageSeconds: 7),
        ],
        .typing,
        ""
      ),
      // The same failure twice, with an action that worked between them.
      (
        [
          ActivityFixtures.row(id: "2", says: "couldn't click Save", outcome: .failed, ageSeconds: 5),
          ActivityFixtures.row(id: "1", says: "clicked Ring", ageSeconds: 6),
          ActivityFixtures.row(id: "0", says: "couldn't click Save", outcome: .failed, ageSeconds: 7),
        ],
        .problems,
        ""
      ),
    ] as [([ActivityLog.Entry], ActivityFilter, String)]
  )
  func hiddenGapsKeepRunsApart(entries: [ActivityLog.Entry], filter: ActivityFilter, query: String) {
    let runs = ActivityMoments(ActivityFixtures.rules(entries), captures: [], scope: .all)
      .shown(filter: filter, query: query)
      .flatMap(\.runs)
    #expect(runs.map(\.id) == ["2", "0"])
    #expect(runs.map(\.count) == [1, 1])
  }

  @Test("search matches what was done, not who did it")
  func searchLooksAtWhatWasDone() {
    let rules = ActivityFixtures.rules([
      ActivityFixtures.row(id: "yours", says: "clicked in Opie", actor: .you, ageSeconds: 5),
      ActivityFixtures.row(id: "agent", says: "scrolled in Safari", ageSeconds: 9),
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
    let rules = ActivityFixtures.rules([
      ActivityFixtures.row(id: "3", says: "clicked in Opie", actor: .you, ageSeconds: 140),
      ActivityFixtures.row(id: "2", says: "clicked in Opie", actor: .you, ageSeconds: 150),
      ActivityFixtures.row(id: "1", says: "clicked in Opie", actor: .you, ageSeconds: 157),
      ActivityFixtures.row(id: "yesterday", says: "clicked in Opie", ageSeconds: 12 * 3600),
    ])
    #expect(rules.countToday == 3)
  }

  @Test("a row of the person's input holding many actions counts the same in the day's total and in its moment")
  func personalInputCountsTheSameInTheDayAndItsMoment() throws {
    let rules = ActivityFixtures.rules([
      ActivityFixtures.row(id: "personal", actor: .you, ageSeconds: 0, count: 2107)
    ])
    let moment = try #require(ActivityMoments(rules, captures: [], scope: .all).moments.first)
    #expect(rules.countToday == 2107)
    #expect(moment.actions == rules.countToday)
  }

  @Test("today's action total preserves repeats and defaults missing or invalid counts to one")
  func repeatsAndDefaultCountsUseActions() throws {
    let rules = ActivityFixtures.rules([
      ActivityFixtures.row(id: "newer", ageSeconds: 0, count: 7),
      ActivityFixtures.row(id: "older", ageSeconds: 1, count: 3),
      ActivityFixtures.row(id: "missing", ageSeconds: 2),
      ActivityFixtures.row(id: "zero", ageSeconds: 3, count: 0),
      ActivityFixtures.row(id: "negative", ageSeconds: 4, count: -2),
    ])
    let moment = try #require(ActivityMoments(rules, captures: [], scope: .all).moments.first)
    #expect(moment.runs.count == 1)
    #expect(moment.runs.first?.count == 5)
    #expect(rules.countToday == 13)
    #expect(moment.actions == rules.countToday)
  }

  @Test("today's action total counts only dated rows in the chosen session")
  func actionTotalsKeepTheirDateAndSessionScope() {
    let rules = ActivityFixtures.rules([
      ActivityFixtures.row(id: "chosen", ageSeconds: 0, count: 7, session: "chosen"),
      ActivityFixtures.row(id: "other", ageSeconds: 1, count: 3, session: "other"),
      ActivityFixtures.row(id: "personal", actor: .you, ageSeconds: 2, count: 2107),
      ActivityFixtures.row(id: "yesterday", ageSeconds: 12 * 3600, count: 100, session: "chosen"),
      ActivityFixtures.row(id: "undated", count: 200, session: "chosen"),
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
    let rules = ActivityFixtures.rules(log.entries)
    let moment = try #require(ActivityMoments(rules, captures: [], scope: .all).moments.first)
    #expect(rules.countToday == .max)
    #expect(moment.actions == .max)
    #expect(moment.runs.count == (repeats ? 1 : 3))
    #expect(moment.runs.first?.actions == (repeats ? .max : count))
  }

  @Test("a row's time is the same at every reading when the service gives its start")
  func startHoldsAcrossReadings() {
    let started = ActivityFixtures.readAt
      .addingTimeInterval(-4.4)
    let first = ActivityFixtures.row(id: "1", ageSeconds: 4, startedAt: started)
    let second = ActivityFixtures.row(id: "1", ageSeconds: 6, startedAt: started)
    let later = ActivityGrouping(
      log: ActivityLog(entries: [second]),
      readAt: ActivityFixtures.readAt.addingTimeInterval(1.6),
      now: ActivityFixtures.readAt,
      calendar: ActivityFixtures.calendar
    )
    #expect(
      ActivityFixtures.rules([first])
        .start(of: first) == started
    )
    #expect(later.start(of: second) == started)
  }
}

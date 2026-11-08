import AtlasServiceTesting
import Foundation
import Testing

@testable import AtlasService

@Suite("Activity outline")
struct ActivityOutlineTests {
  /// The runs of `entries`, read and looked at 10:31:42.
  ///
  /// - Parameter entries: The log's rows, newest first.
  /// - Returns: The runs, newest first.
  private static func runs(_ entries: [ActivityLog.Entry]) -> [ActivityMoment] {
    ActivityMoments(ActivityFixtures.rules(entries), captures: [], scope: .all).moments
  }

  @Test("each session's runs on one day form one session, in the order its newest run comes")
  func eachSessionsRunsFormOneSession() {
    let runs = Self.runs([
      ActivityLog.Entry(id: "a2", says: "typed", ageSeconds: 5, session: "claude", verb: "type", app: "Notes"),
      ActivityLog.Entry(id: "b1", says: "clicked", ageSeconds: 10, session: "codex", verb: "click", app: "Mail"),
      ActivityLog.Entry(id: "a1", says: "clicked", ageSeconds: 400, session: "claude", verb: "click", app: "Opie"),
    ])
    let groups = ActivitySessionGroup.grouped(runs)
    #expect(groups.map(\.stream) == [.session("claude"), .session("codex")])
    #expect(groups.map { $0.runs.map(\.app?.name) } == [["Notes", "Opie"], ["Mail"]])
  }

  @Test("a tally counts kinds in the order first done, with actions, screenshots, problems and apps")
  func aTallyCountsKindsInOrder() {
    let tally = ActivityTally(
      ActivityFixtures.rules([
        ActivityLog.Entry(
          id: "s1",
          says: "took a screenshot",
          ageSeconds: 2,
          operation: "screenshot",
          session: "claude",
          verb: "screenshot"
        ),
        ActivityLog.Entry(
          id: "c2",
          says: "couldn't click",
          outcome: .failed,
          ageSeconds: 3,
          session: "claude",
          verb: "click",
          app: "Google Chrome"
        ),
        ActivityLog.Entry(id: "k1", says: "pressed", ageSeconds: 4, session: "claude", verb: "key", app: "TextEdit"),
        ActivityLog.Entry(
          id: "t1",
          says: "typed",
          ageSeconds: 5,
          count: 4,
          session: "claude",
          verb: "type",
          app: "TextEdit"
        ),
      ])
      .runs(captures: [], first: 0) { _ in 0 }
      .map(\.run)
    )
    #expect(tally.order == [.typing, .keys, .pointer, .screenshot])
    #expect(tally.actions == 7)
    #expect(tally.screenshots == 1)
    #expect(tally.problems == 1)
    #expect(tally.apps == ["TextEdit", "Google Chrome"])
    #expect(tally.verbs[.pointer] == ["click"])
  }

  @Test("a tally counts a line refused before anything was sent only among the refused")
  func aTallyCountsRefusalsApart() {
    let runs = ActivityFixtures.rules([
      ActivityLog.Entry(
        id: "n1",
        says: "didn't type in Notes",
        outcome: .notSent,
        ageSeconds: 2,
        session: "claude",
        verb: "type",
        app: "Notes"
      ),
      ActivityLog.Entry(id: "c1", says: "clicked", ageSeconds: 3, session: "claude", verb: "click", app: "Notes"),
    ])
    .runs(captures: [], first: 0) { _ in 0 }
    .map(\.run)
    let tally = ActivityTally(runs)
    #expect(tally.order == [.pointer])
    #expect(tally.actions == 1)
    #expect(tally.refused == 1)
    #expect(ActivityTally(Array(runs.prefix(1))).kinds.isEmpty)
  }
}

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
      ActivityFixtures.row(id: "a2", says: "typed", ageSeconds: 5, session: "claude", verb: "type", app: "Notes"),
      ActivityFixtures.row(id: "b1", says: "clicked", ageSeconds: 10, session: "codex", verb: "click", app: "Mail"),
      ActivityFixtures.row(id: "a1", says: "clicked", ageSeconds: 400, session: "claude", verb: "click", app: "Opie"),
    ])
    let groups = ActivitySessionGroup.grouped(runs)
    #expect(groups.map(\.stream) == [.session("claude"), .session("codex")])
    #expect(groups.map { $0.runs.map(\.app?.name) } == [["Notes", "Opie"], ["Mail"]])
  }

  @Test("a tally counts kinds in the order first done, with actions, screenshots, problems and apps")
  func aTallyCountsKindsInOrder() {
    let tally = ActivityTally(
      ActivityFixtures.rules([
        ActivityFixtures.row(
          id: "s1",
          says: "took a screenshot",
          ageSeconds: 2,
          operation: "screenshot",
          session: "claude",
          verb: "screenshot"
        ),
        ActivityFixtures.row(
          id: "c2",
          says: "couldn't click",
          outcome: .failed,
          ageSeconds: 3,
          session: "claude",
          verb: "click",
          app: "Google Chrome"
        ),
        ActivityFixtures.row(id: "k1", says: "pressed", ageSeconds: 4, session: "claude", verb: "key", app: "TextEdit"),
        ActivityFixtures.row(
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
    #expect(tally.gist == ActivityGist(kind: .typing, count: 2, app: "TextEdit", verbs: ["type", "key"]))
    #expect(tally.verbs[.pointer] == ["click"])
  }

  @Test("a gist counts keys as typing, leaves control out and prefers typing among equals")
  func aGistCountsKeysAsTyping() {
    #expect(
      ActivityGist(kinds: [.keys: 2, .pointer: 3, .typing: 2], app: "TextEdit")
        == ActivityGist(kind: .typing, count: 4, app: "TextEdit")
    )
    #expect(ActivityGist(kinds: [.pointer: 1, .typing: 1], app: nil)?.kind == .typing)
    #expect(ActivityGist(kinds: [.control: 5], app: nil) == nil)
    #expect(ActivityGist(kinds: [.screenshot: 2, .control: 5], app: nil)?.count == 2)
  }

  @Test("a record's gist reads its verbs and its busiest app, leaving unknown verbs out")
  func aRecordsGistReadsItsVerbs() {
    #expect(
      ActivityGist(counts: ["click": 3, "type": 1, "environment": 9], apps: ["Google Chrome": 3, "Notes": 1])
        == ActivityGist(kind: .pointer, count: 3, app: "Google Chrome", verbs: ["click"])
    )
    #expect(ActivityGist(counts: ["environment": 2], apps: [:]) == nil)
    #expect(ActivityGist(counts: ["click": .max, "drag": 1], apps: [:])?.count == .max, "huge counts saturate")
    #expect(ActivityGist(counts: ["type": .max, "key": 1], apps: [:])?.count == .max)
    #expect(ActivityGist(counts: [:], apps: ["Notes": 1]) == nil)
  }

  @Test("a session's rows give its gist, the person's own and refused ones left out")
  func aSessionsRowsGiveItsGist() {
    let gist = ActivityGist([
      ActivityFixtures.row(id: "t2", says: "typed", session: "claude", verb: "type", app: "TextEdit"),
      ActivityFixtures.row(id: "k1", says: "pressed", session: "claude", verb: "key", app: "TextEdit"),
      ActivityFixtures.row(id: "c1", says: "clicked", session: "claude", verb: "click", app: "Google Chrome"),
      ActivityFixtures.row(id: "y1", says: "clicked", actor: .you, verb: "click", app: "Notes"),
      ActivityFixtures.row(id: "y2", says: "clicked", actor: .you, verb: "click", app: "Notes"),
    ])
    #expect(gist == ActivityGist(kind: .typing, count: 2, app: "TextEdit", verbs: ["type", "key"]))
    #expect(ActivityGist([]) == nil)
    let refused = ActivityFixtures.row(
      id: "n1",
      says: "didn't type in TextEdit",
      outcome: .notSent,
      session: "claude",
      verb: "type",
      app: "TextEdit"
    )
    #expect(ActivityGist([refused]) == nil, "a row refused before anything was sent did nothing")
  }
}

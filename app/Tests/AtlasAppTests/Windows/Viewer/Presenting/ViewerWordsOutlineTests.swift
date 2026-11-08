import AtlasService
import Testing

@testable import AtlasApp

@Suite("ViewerWords+Outline")
struct ViewerWordsOutlineTests {
  @Test("a session's title says what its verbs did: closed, moved or a mix, never opened or clicked for them")
  func titlesFollowTheVerbs() {
    #expect(
      ViewerWords.sessionTitle(ActivityGist(kind: .window, count: 2, app: "TextEdit", verbs: ["close"]), working: false)
        == "Closed TextEdit"
    )
    #expect(
      ViewerWords.sessionTitle(ActivityGist(kind: .window, count: 2, app: "TextEdit", verbs: ["open"]), working: true)
        == "Opening TextEdit"
    )
    #expect(
      ViewerWords.sessionTitle(
        ActivityGist(kind: .window, count: 3, app: nil, verbs: ["open", "close"]),
        working: false
      )
        == "Opened and closed apps"
    )
    #expect(
      ViewerWords.sessionTitle(
        ActivityGist(kind: .window, count: 2, app: "Probe", verbs: ["install", "open"]),
        working: false
      ) == "Opened and installed Probe"
    )
    #expect(
      ViewerWords.sessionTitle(ActivityGist(kind: .window, count: 1, app: nil, verbs: ["cleanup"]), working: false)
        == "Cleaned up"
    )
    #expect(
      ViewerWords.sessionTitle(ActivityGist(kind: .window, count: 1, app: "Notes"), working: false)
        == "Worked with Notes",
      "verbs not known say nothing they did not see"
    )
    #expect(
      ViewerWords.sessionTitle(ActivityGist(kind: .pointer, count: 1, app: "Notes"), working: false)
        == "Used the pointer in Notes"
    )
    #expect(
      ViewerWords.sessionTitle(ActivityGist(kind: .pointer, count: 4, app: "Opie", verbs: ["move"]), working: false)
        == "Moved the pointer in Opie"
    )
    #expect(
      ViewerWords.sessionTitle(
        ActivityGist(kind: .pointer, count: 4, app: "Opie", verbs: ["click", "drag"]),
        working: true
      )
        == "Using the pointer in Opie"
    )
    #expect(
      ViewerWords.sessionTitle(ActivityGist(kind: .pointer, count: 1, app: "Opie", verbs: ["press"]), working: false)
        == "Clicked in Opie"
    )
    #expect(
      ViewerWords.sessionTitle(ActivityGist(kind: .screenshot, count: 2, app: nil), working: false)
        == "Took 2 screenshots"
    )
  }

  @Test("a run's summary names what its verbs did, and a run in no app counts its commands and screenshots")
  func summariesFollowTheVerbs() throws {
    let grouping = ActivityGrouping(
      log: ActivityLog(entries: [
        ActivityLog.Entry(id: "c2", says: "closed TextEdit", ageSeconds: 1, verb: "close", app: "TextEdit"),
        ActivityLog.Entry(id: "m1", says: "moved the pointer", ageSeconds: 2, verb: "move", app: "TextEdit"),
      ]),
      readAt: .now,
      now: .now,
      calendar: .current
    )
    let run = try #require(ActivityMoments(grouping, captures: [], scope: .all).moments.first)
    let closing = run.tally
    #expect(ViewerWords.runSummary(closing, app: "TextEdit") == "Moved the pointer and closed TextEdit · 2 actions")
  }
}

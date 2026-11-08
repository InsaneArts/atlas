import AtlasService
import Testing

@testable import AtlasApp

@Suite("ViewerWords+Outline")
struct ViewerWordsOutlineTests {
  @Test("what apps and the pointer did follows their verbs: closed, moved or a mix, never opened or clicked for them")
  func phrasesFollowTheVerbs() {
    #expect(ViewerWords.window(["close"], object: "TextEdit") == "closed TextEdit")
    #expect(ViewerWords.window(["open", "close"], object: "") == "opened and closed apps")
    #expect(ViewerWords.window(["install", "open"], object: "Probe") == "opened and installed Probe")
    #expect(ViewerWords.window(["cleanup"], object: "") == "cleaned up")
    #expect(
      ViewerWords.window([], object: "Notes") == "worked with Notes",
      "verbs not known say nothing they did not see"
    )
    #expect(ViewerWords.pointer([]) == "used the pointer")
    #expect(ViewerWords.pointer(["move"]) == "moved the pointer")
    #expect(ViewerWords.pointer(["click", "drag"]) == "used the pointer")
    #expect(ViewerWords.pointer(["press"]) == "clicked")
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
    let refusing = ActivityGrouping(
      log: ActivityLog(entries: [
        ActivityLog.Entry(
          id: "n1",
          says: "didn't type in Notes",
          outcome: .notSent,
          ageSeconds: 1,
          verb: "type",
          app: "Notes"
        )
      ]),
      readAt: .now,
      now: .now,
      calendar: .current
    )
    let refused = try #require(ActivityMoments(refusing, captures: [], scope: .all).moments.first)
    #expect(ViewerWords.runSummary(refused.tally, app: "Notes") == "1 action not sent", "nothing refused reads as done")
  }
}

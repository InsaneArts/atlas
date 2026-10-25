import AtlasService
import AtlasStatus
import Testing

@testable import AtlasApp

@Suite("Words")
struct WordsTests {
  @Test(
    "an age reads as just now, seconds, minutes or hours",
    arguments: [
      (2.0, "just now"), (42, "42 s ago"), (600, "10 min ago"), (7200, "2 h ago"),
    ]
  )
  func ago(seconds: Double, words: String) {
    #expect(Words.ago(seconds) == words)
  }

  // Each case sits on a unit's edge, where a row would otherwise read "60 s" or "60 min".
  @Test(
    "a short age moves to the next unit at a whole minute and a whole hour",
    arguments: [(0.4, "now"), (9.0, "9 s"), (59.9, "59 s"), (60, "1 min"), (3599, "59 min"), (3600, "1 h")]
  )
  func shortAgo(seconds: Double, words: String) {
    #expect(Words.shortAgo(seconds) == words)
  }

  @Test("unsupported ages read as unknown and never trap")
  func invalidAges() {
    for seconds in [Double.nan, .infinity, -.infinity, 1e100, .greatestFiniteMagnitude, Double(Int.max) * 3600] {
      #expect(Words.ago(seconds) == Words.unknown)
      #expect(Words.shortAgo(seconds) == Words.unknown)
    }
  }

  @Test("an operation reads as what it did, or what could not be done, never its payload or its name")
  func operations() {
    #expect(Words.describe(.init(operation: "run", ok: true)) == "ran a command")
    #expect(Words.describe(.init(operation: "run", ok: false)) == "couldn't run a command")
    #expect(Words.describe(.init(operation: "input", ok: false, actionCount: 2)) == "couldn't send 2 actions")
    #expect(Words.describe(.init(operation: "input", ok: true, actionCount: 1)) == "sent 1 action")
    #expect(Words.describe(.init(operation: "logout", ok: false)) == "couldn't log out the helper")
    #expect(Words.describe(.init(operation: "newop", ok: false)) == "couldn't finish an action")
  }

  @Test("an operation the service describes reads in the service's words")
  func serviceWords() {
    let row = ViewerStatus.Activity(operation: "input", ok: true, actionCount: 1, says: "clicked Save in TextEdit")
    #expect(Words.describe(row) == "clicked Save in TextEdit")
  }

  @Test("a command's message shows as a sentence, with one full stop")
  func sentence() {
    #expect(Words.sentence("Atlas.app must be in Applications") == "Atlas.app must be in Applications.")
    #expect(Words.sentence("Done.") == "Done.")
    #expect(Words.sentence("Waiting…") == "Waiting…")
    #expect(Words.sentence("") == "")
  }

  @Test("a size reads as width × height, or nothing when unknown")
  func size() {
    #expect(Words.size(.init(width: 2560, height: 1440)) == "2560 × 1440")
    #expect(Words.size(nil) == nil)
    #expect(Words.size(.init(width: 0, height: 10)) == nil)
  }

  @Test("unsupported display sizes have no words and never trap")
  func invalidSizes() {
    for side in [Double.nan, .infinity, -.infinity, 1e100, Double(Int.max), -1, 0, 32769] {
      #expect(Words.size(.init(width: side, height: 1)) == nil)
      #expect(Words.size(.init(width: 1, height: side)) == nil)
    }
    #expect(Words.size(.init(width: 32768, height: 1.5)) == "32768 × 2")
  }

  @Test("a picture's age reads as live, updated or stale")
  func freshness() {
    #expect(Words.freshness(1) == "Live")
    #expect(Words.freshness(5) == "Updated 5 s ago")
    #expect(Words.freshness(Freshness.stale) == "Picture is stale")
    #expect(Words.freshness(nil) == "Waiting for the first picture")
  }
}

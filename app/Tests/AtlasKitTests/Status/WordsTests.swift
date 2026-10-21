import Testing

@testable import AtlasKit

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

  @Test("an operation reads as what it did, or what could not be done, never its payload or its name")
  func operations() {
    #expect(Words.describe(.init(operation: "run", ok: true)) == "ran a command")
    #expect(Words.describe(.init(operation: "run", ok: false)) == "couldn't run a command")
    #expect(Words.describe(.init(operation: "input", ok: false, actionCount: 2)) == "couldn't send 2 actions")
    #expect(Words.describe(.init(operation: "input", ok: true, actionCount: 1)) == "sent 1 action")
    #expect(Words.describe(.init(operation: "logout", ok: false)) == "couldn't log out the helper")
    #expect(Words.describe(.init(operation: "newop", ok: false)) == "couldn't finish an action")
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

  @Test("a picture's age reads as live, updated or stale")
  func freshness() {
    #expect(Words.freshness(1) == "Live")
    #expect(Words.freshness(5) == "Updated 5 s ago")
    #expect(Words.freshness(Freshness.stale) == "Picture is stale")
    #expect(Words.freshness(nil) == "Waiting for the first picture")
  }
}

import Foundation
import Testing

@testable import AtlasService

@Suite("FeedAnswer")
struct FeedAnswerTests {
  /// The answer `text` reads as with `status`.
  private func read(_ text: String, status: Int = 200) throws -> FeedAnswer {
    try FeedAnswer.read(Data(text.utf8), status: status)
  }

  @Test("events say what to read again and where the reader stands after them")
  func eventsSayWhatToReadAgain() throws {
    let answer = try read(
      #"{"ok":true,"feed":"f","head":5,"events":[{"feed":"f","seq":4,"kind":"asks"},{"feed":"f","seq":5,"kind":"status"}]}"#
    )
    #expect(answer == .events(kinds: [.asks, .status], last: FeedPosition(feed: "f", after: 5), poll: false))
  }

  @Test("a kind this app does not know reads everything again")
  func anUnknownKindReadsEverything() throws {
    let answer = try read(#"{"ok":true,"feed":"f","head":1,"events":[{"feed":"f","seq":1,"kind":"windows"}]}"#)
    #expect(answer == .events(kinds: Set(ChangeKind.allCases), last: FeedPosition(feed: "f", after: 1), poll: false))
  }

  @Test("a reset, a wait that ended with nothing, a full service and an unavailable feed")
  func everyOtherAnswer() throws {
    #expect(try read(#"{"ok":true,"reset":{"feed":"g","head":9}}"#) == .reset(FeedPosition(feed: "g", after: 9)))
    #expect(try read(#"{"ok":true,"feed":"f","head":3,"events":[]}"#) == .events(kinds: [], last: nil, poll: false))
    #expect(
      try read(#"{"ok":true,"feed":"f","head":3,"events":[],"poll":true}"#)
        == .events(kinds: [], last: nil, poll: true)
    )
    #expect(try read(#"{"ok":false,"unavailable":true}"#, status: 503) == .unavailable)
    #expect(throws: ServiceError.self) { try read("{}", status: 404) }
  }
}

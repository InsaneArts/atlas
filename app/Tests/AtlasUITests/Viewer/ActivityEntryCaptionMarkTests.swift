import Testing

@testable import AtlasUI

@Suite("ActivityEntry+CaptionMark")
struct ActivityEntryCaptionMarkTests {
  /// An action as a caption shows it.
  ///
  /// - Parameters:
  ///   - kind: Whose action it was.
  ///   - isNewest: Whether it is the newest.
  /// - Returns: The entry.
  private func entry(_ kind: ActivityKind, isNewest: Bool) -> ActivityEntry {
    ActivityEntry(id: 0, initial: "A", line: "Agent typed", when: "now", kind: kind, isNewest: isNewest)
  }

  @Test("a failure is marked whatever its age; otherwise only the newest action breathes")
  func failureFirstThenNewest() {
    #expect(entry(.failed, isNewest: false).captionMark == .failed)
    #expect(entry(.failed, isNewest: true).captionMark == .failed)
    #expect(entry(.others, isNewest: true).captionMark == .newest)
    #expect(entry(.yours, isNewest: false).captionMark == nil)
  }
}

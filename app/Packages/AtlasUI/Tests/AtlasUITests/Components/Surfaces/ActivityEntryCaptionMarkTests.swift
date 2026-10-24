import Testing

@testable import AtlasUI

@Suite("ActivityEntry+CaptionMark")
struct ActivityEntryCaptionMarkTests {
  /// An action as a caption shows it.
  ///
  /// - Parameters:
  ///   - mark: Its mark.
  ///   - isNewest: Whether it is the newest.
  /// - Returns: The entry.
  private func entry(_ mark: ActivityMarkKind, isNewest: Bool) -> ActivityEntry {
    ActivityEntry(id: "0", line: "Agent typed", when: "now", mark: mark, isNewest: isNewest)
  }

  @Test("a failure is marked whatever its age; otherwise only the newest action breathes")
  func failureFirstThenNewest() {
    #expect(entry(.failed, isNewest: false).captionMark == .failed)
    #expect(entry(.failed, isNewest: true).captionMark == .failed)
    #expect(entry(.quiet, isNewest: true).captionMark == .newest)
    #expect(entry(.quiet, isNewest: false).captionMark == nil)
  }
}

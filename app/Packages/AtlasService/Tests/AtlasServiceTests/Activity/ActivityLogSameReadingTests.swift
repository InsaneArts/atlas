import AtlasServiceTesting
import Foundation
import Testing

@testable import AtlasService

@Suite("ActivityLog.isSameReading")
struct ActivityLogSameReadingTests {
  /// When the rows below started.
  static let start = Date(timeIntervalSince1970: 1_792_665_000)

  @Test("a later reading of the same rows reads the same, whatever their ages say")
  func aLaterReadingOfTheSameRowsReadsTheSame() {
    let first = ActivityLog(entries: [
      ActivityLog.Entry(id: "a", says: "clicked Save", ageSeconds: 3, startedAt: Self.start),
      ActivityLog.Entry(id: "b", says: "typed", ageSeconds: 9, startedAt: Self.start.addingTimeInterval(-6)),
    ])
    var later = first
    later.entries[0].ageSeconds = 5
    later.entries[1].ageSeconds = 11
    #expect(first.isSameReading(as: later))
  }

  @Test("a row with no start whose age changed reads as changed")
  func aRowWithNoStartWhoseAgeChangedReadsAsChanged() {
    let first = ActivityLog(entries: [ActivityLog.Entry(id: "a", ageSeconds: 3)])
    var later = first
    later.entries[0].ageSeconds = 5
    #expect(!first.isSameReading(as: later))
  }

  @Test("a new row, a changed row or a changed session reads as changed")
  func aNewOrChangedRowReadsAsChanged() {
    let first = ActivityLog(entries: [ActivityLog.Entry(id: "a", says: "clicked Save", startedAt: Self.start)])
    var added = first
    added.entries.insert(ActivityLog.Entry(id: "b", startedAt: Self.start), at: 0)
    #expect(!first.isSameReading(as: added))
    var changed = first
    changed.entries[0].outcome = .failed
    #expect(!first.isSameReading(as: changed))
    var named = first
    named.sessions["s"] = HostSessionSummary(name: "atlas", host: .claude)
    #expect(!first.isSameReading(as: named))
  }

  @Test("a screenshot row whose capture isn't listed yet is not found, and is once it is")
  func aScreenshotRowWhoseCaptureIsntListedYetIsNotFound() {
    let rules = ActivityFixtures.rules([
      ActivityLog.Entry(id: "s", ageSeconds: 4, operation: "screenshot", capture: "shot.png")
    ])
    #expect(!rules.findsEveryShot(in: []))
    #expect(rules.findsEveryShot(in: [Screenshot(name: "shot.png")]))
  }
}

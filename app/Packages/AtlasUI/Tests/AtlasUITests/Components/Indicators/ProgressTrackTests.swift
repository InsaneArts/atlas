import Testing

@testable import AtlasUI

@Suite("ProgressTrack")
struct ProgressTrackTests {
  @Test("the progress track's current segment is the long one")
  func currentSegmentIsLong() {
    let widths = (0..<6)
      .map { ProgressTrack.width(ProgressTrack.segment($0, current: 2)) }
    #expect(widths == [14, 14, 22, 14, 14, 14])
    #expect(ProgressTrack.segment(1, current: 2) == .done)
    #expect(ProgressTrack.segment(3, current: 2) == .upcoming)
  }
}

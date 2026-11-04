import Foundation
import Testing

@testable import AtlasService

@Suite("Frame rectangles")
struct FrameRectangleTests {
  /// The records of `rectangles`, as x, y, width and height.
  private static func records(_ rectangles: [[UInt32]]) -> Data {
    Data(
      rectangles.flatMap {
        $0.flatMap { value in
          (0..<4)
            .reversed()
            .map { UInt8(truncatingIfNeeded: value >> ($0 * 8)) }
        }
      }
    )
  }

  /// What reading `rectangles` in a 4 × 4 picture gives, with exactly their pixels after them.
  private static func read(_ rectangles: [[UInt32]], extra: Int = 0) -> Result<[FrameRectangle], FramePipeError> {
    let pixels = rectangles.reduce(0) { $0 + Int($1[2] * $1[3]) * 3 }
    let length = UInt64(rectangles.count * 16 + pixels + extra)
    return Result { () throws(FramePipeError) in
      try FrameRectangle.records(Self.records(rectangles), width: 4, height: 4, payloadLength: length)
    }
  }

  @Test("rectangles inside the picture, in order and apart, are read with their pixels")
  func ordered() throws {
    let found = try Self.read([[0, 0, 1, 1], [2, 1, 2, 1]])
      .get()
    #expect(found.map(\.bytes) == [3, 6])
    #expect(
      Self.read([[0, 0, 1, 1], [2, 0, 2, 1], [0, 3, 4, 1]]) == .failure(.invalid),
      "no smaller than a full picture"
    )
  }

  @Test("empty, outside, unordered, overlapping or oversized rectangles are refused")
  func refused() {
    for rectangles: [[UInt32]] in [
      [], [[0, 0, 0, 1]], [[3, 0, 2, 1]], [[0, 0, 1, UInt32.max]], [[2, 0, 1, 1], [0, 0, 1, 1]],
      [[0, 0, 2, 2], [1, 1, 2, 2]], [[0, 0, 4, 4]],
    ] {
      #expect(Self.read(rectangles) == .failure(.invalid), "\(rectangles)")
    }
    #expect(Self.read([[0, 0, 1, 1]], extra: 1) == .failure(.invalid), "pixels past the rectangles")
    let many = (0..<257)
      .map { [UInt32($0 % 4), UInt32($0 / 4), 1, 1] }
    #expect(
      (try? FrameRectangle.records(Self.records(many), width: 4_096, height: 4_096, payloadLength: 257 * 19)) == nil
    )
  }
}

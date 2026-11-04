import Foundation

/// One changed area of a delta, in pixels.
public struct FrameRectangle: Sendable, Equatable {
  /// The left edge.
  public var x: UInt32
  /// The top edge.
  public var y: UInt32
  /// The width, above zero.
  public var width: UInt32
  /// The height, above zero.
  public var height: UInt32

  /// Creates a rectangle.
  ///
  /// - Parameters:
  ///   - x: The left edge.
  ///   - y: The top edge.
  ///   - width: The width.
  ///   - height: The height.
  public init(x: UInt32, y: UInt32, width: UInt32, height: UInt32) {
    self.x = x
    self.y = y
    self.width = width
    self.height = height
  }

  /// Its pixels' bytes.
  public var bytes: UInt64 {
    UInt64(width) * UInt64(height) * 3
  }

  /// A delta's rectangle records, read from the front of its payload, for a `width` × `height` picture whose payload is
  /// `payloadLength` bytes. They must be non-empty, inside the picture, sorted by y then x, apart from each other, and
  /// leave exactly their pixels' bytes, which with the records stay smaller than a full picture.
  ///
  /// - Parameters:
  ///   - records: The records, 16 bytes each.
  ///   - width: The picture's width.
  ///   - height: The picture's height.
  ///   - payloadLength: The whole payload's length, records included.
  /// - Returns: The rectangles, in order.
  /// - Throws: ``FramePipeError/invalid`` for records that break any of those rules.
  public static func records(
    _ records: Data,
    width: UInt32,
    height: UInt32,
    payloadLength: UInt64
  ) throws(FramePipeError) -> [Self] {
    let record = FramePipeLimits.rectangleRecord
    let count = records.count / record
    let sides = UInt32(1)...FramePipeLimits.side
    guard
      records.count.isMultiple(of: record),
      (1...Int(FramePipeLimits.rectangles)).contains(count),
      sides.contains(width),
      sides.contains(height)
    else { throw .invalid }
    let bytes = Array(records)
    let word = { (at: Int) in bytes[at..<(at + 4)].reduce(UInt32(0)) { $0 << 8 | UInt32($1) } }
    var found: [Self] = []
    var pixels: UInt64 = 0
    for start in stride(from: 0, to: bytes.count, by: record) {
      let rectangle = Self(x: word(start), y: word(start + 4), width: word(start + 8), height: word(start + 12))
      let inside =
        rectangle.width > 0 && rectangle.height > 0
        && UInt64(rectangle.x) + UInt64(rectangle.width) <= UInt64(width)
        && UInt64(rectangle.y) + UInt64(rectangle.height) <= UInt64(height)
      let ordered = found.last.map { ($0.y, $0.x) < (rectangle.y, rectangle.x) } ?? true
      // Overlap is checked only for a rectangle inside the picture, whose edges cannot overflow.
      guard inside, ordered, found.allSatisfy({ $0.isApart(from: rectangle) }) else { throw .invalid }
      pixels += rectangle.bytes
      found.append(rectangle)
    }
    let picture = UInt64(width) * UInt64(height) * 3
    guard UInt64(records.count) + pixels == payloadLength, payloadLength < picture else { throw .invalid }
    return found
  }

  /// Whether the two share no pixel.
  private func isApart(from other: Self) -> Bool {
    x >= other.x + other.width || other.x >= x + width || y >= other.y + other.height || other.y >= y + height
  }
}

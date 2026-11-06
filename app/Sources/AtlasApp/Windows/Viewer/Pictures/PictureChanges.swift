import CoreGraphics
import Foundation

/// Show changes' comparison of two pictures of the helper's screen, done here and sent nowhere: both drawn small on one
/// grid of squares, and a square counts as changed where its pixels differ, on average, by more than a little.
enum PictureChanges {
  /// 48: squares across the picture.
  nonisolated static let across = 48
  /// 6: each square's side, in the small drawing's pixels.
  nonisolated static let side = 6
  /// 10: the least average difference per colour, out of 255, a square counts as changed; less is noise, such as a
  /// JPEG's.
  nonisolated static let threshold = 10

  /// The squares that changed between two pictures, as shares of the picture, left to right and top to bottom.
  ///
  /// - Parameters:
  ///   - before: The picture before.
  ///   - after: The picture after, drawn at the first one's shape.
  /// - Returns: The squares; none when either cannot be drawn.
  @concurrent nonisolated static func squares(before: CGImage, after: CGImage) async -> [CGRect] {
    guard before.width > 0, before.height > 0 else { return [] }
    let width = across * side
    let down = max(1, Int((Double(across) * Double(before.height) / Double(before.width)).rounded()))
    let height = down * side
    guard let first = pixels(before, width: width, height: height),
      let second = pixels(after, width: width, height: height)
    else { return [] }
    let square = (across: 1 / Double(across), down: 1 / Double(down))
    var changed: [CGRect] = []
    for row in 0..<down {
      for column in 0..<across {
        guard difference(first, second, row: row, column: column) > threshold * side * side * 3 else { continue }
        changed.append(
          CGRect(
            x: Double(column) * square.across,
            y: Double(row) * square.down,
            width: square.across,
            height: square.down
          )
        )
      }
    }
    return changed
  }

  /// How much one square differs between two drawings: the sum, over its pixels, of each colour's difference.
  ///
  /// - Parameters:
  ///   - first: The drawing before.
  ///   - second: The drawing after, of the same size.
  ///   - row: The square's row.
  ///   - column: The square's column.
  /// - Returns: The difference.
  private nonisolated static func difference(
    _ first: (bytes: [UInt8], rowBytes: Int),
    _ second: (bytes: [UInt8], rowBytes: Int),
    row: Int,
    column: Int
  ) -> Int {
    var total = 0
    for y in row * side..<(row + 1) * side {
      for x in column * side..<(column + 1) * side {
        let at = y * first.rowBytes + x * 4
        for channel in 0..<3 {
          total += abs(Int(first.bytes[at + channel]) - Int(second.bytes[at + channel]))
        }
      }
    }
    return total
  }

  /// A picture drawn at `width` by `height`, as its rows of red, green, blue and padding bytes, top row first.
  ///
  /// - Parameters:
  ///   - image: The picture.
  ///   - width: The drawing's width.
  ///   - height: Its height.
  /// - Returns: The bytes and how many each row takes, or nil when it cannot be drawn.
  nonisolated static func pixels(_ image: CGImage, width: Int, height: Int) -> (bytes: [UInt8], rowBytes: Int)? {
    guard let space = CGColorSpace(name: CGColorSpace.sRGB),
      let context = CGContext(
        data: nil,
        width: width,
        height: height,
        bitsPerComponent: 8,
        bytesPerRow: width * 4,
        space: space,
        bitmapInfo: CGImageAlphaInfo.noneSkipLast.rawValue
      )
    else { return nil }
    context.interpolationQuality = .medium
    context.draw(image, in: CGRect(x: 0, y: 0, width: width, height: height))
    guard let drawn = context.makeImage(), let data = drawn.dataProvider?.data as Data? else { return nil }
    return ([UInt8](data), drawn.bytesPerRow)
  }
}

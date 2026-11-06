import CoreGraphics
import Testing

@testable import AtlasApp

@Suite("PictureChanges")
struct PictureChangesTests {
  /// A white picture `width` by `height`, with a black block where one is given, measured from its top-left.
  ///
  /// - Parameters:
  ///   - width: Its width in pixels.
  ///   - height: Its height in pixels.
  ///   - block: The black block, from the top-left; none by default.
  /// - Returns: The picture.
  /// - Throws: The failed requirement, when it cannot be drawn.
  static func picture(width: Int = 480, height: Int = 300, block: CGRect? = nil) throws -> CGImage {
    let space = try #require(CGColorSpace(name: CGColorSpace.sRGB))
    let context = try #require(
      CGContext(
        data: nil,
        width: width,
        height: height,
        bitsPerComponent: 8,
        bytesPerRow: width * 4,
        space: space,
        bitmapInfo: CGImageAlphaInfo.noneSkipLast.rawValue
      )
    )
    context.setFillColor(CGColor(gray: 1, alpha: 1))
    context.fill(CGRect(x: 0, y: 0, width: width, height: height))
    if let block {
      context.setFillColor(CGColor(gray: 0, alpha: 1))
      // Drawn from the bottom-left, as Core Graphics draws.
      context.fill(CGRect(x: block.minX, y: CGFloat(height) - block.maxY, width: block.width, height: block.height))
    }
    return try #require(context.makeImage())
  }

  @Test("two pictures alike change nowhere")
  func aliked() async throws {
    let picture = try Self.picture()
    #expect(await PictureChanges.squares(before: picture, after: picture).isEmpty)
  }

  @Test("the squares that changed are found where they changed, as shares of the picture from its top-left")
  func changedSquares() async throws {
    let before = try Self.picture()
    let after = try Self.picture(block: CGRect(x: 0, y: 0, width: 60, height: 40))
    let squares = await PictureChanges.squares(before: before, after: after)
    #expect(!squares.isEmpty)
    #expect(squares.allSatisfy { $0.maxX <= 0.15 && $0.maxY <= 0.2 }, "\(squares)")
    #expect(squares.contains { $0.minX == 0 && $0.minY == 0 })
  }

  @Test("a picture after of another size is compared at the first one's shape")
  func anotherSize() async throws {
    let before = try Self.picture()
    let after = try Self.picture(width: 240, height: 150, block: CGRect(x: 210, y: 130, width: 30, height: 20))
    let squares = await PictureChanges.squares(before: before, after: after)
    #expect(!squares.isEmpty)
    #expect(squares.allSatisfy { $0.minX >= 0.8 && $0.minY >= 0.8 }, "\(squares)")
  }
}

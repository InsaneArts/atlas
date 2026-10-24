import CoreGraphics
import Testing

@testable import AtlasUI

@Suite("DesktopPicture")
struct DesktopPictureTests {
  /// A one-pixel image of one grey.
  ///
  /// - Parameter white: The grey, from black (0) to white (255).
  /// - Returns: The image.
  /// - Throws: When no image could be made.
  private func pixel(_ white: UInt8) throws -> CGImage {
    let context = try #require(
      CGContext(
        data: nil,
        width: 1,
        height: 1,
        bitsPerComponent: 8,
        bytesPerRow: 0,
        space: CGColorSpaceCreateDeviceGray(),
        bitmapInfo: CGImageAlphaInfo.none.rawValue
      )
    )
    context.setFillColor(gray: CGFloat(white) / 255, alpha: 1)
    context.fill(CGRect(x: 0, y: 0, width: 1, height: 1))
    return try #require(context.makeImage())
  }

  @Test("pictures are equal when their ids are, whatever their pixels")
  func equalByID() throws {
    let black = try pixel(0)
    let white = try pixel(255)
    #expect(DesktopPicture(id: "frame", image: black) == DesktopPicture(id: "frame", image: white))
    #expect(DesktopPicture(id: "frame", image: black) != DesktopPicture(id: "next", image: black))
  }
}

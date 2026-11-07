import CoreGraphics

@testable import AtlasUI

/// Pictures the drawing tests read back pixel by pixel.
enum TestPictures {
  /// A picture `width` by `height`, green in its outer four columns on each side and red between.
  ///
  /// - Parameters:
  ///   - width: Its width in pixels.
  ///   - height: Its height in pixels.
  /// - Returns: The picture.
  static func picture(width: Int, height: Int) -> DesktopPicture? {
    guard
      let context = CGContext(
        data: nil,
        width: width,
        height: height,
        bitsPerComponent: 8,
        bytesPerRow: 0,
        space: CGColorSpace(name: CGColorSpace.sRGB) ?? CGColorSpaceCreateDeviceRGB(),
        bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
      )
    else { return nil }
    context.setFillColor(red: 1, green: 0, blue: 0, alpha: 1)
    context.fill(CGRect(x: 0, y: 0, width: width, height: height))
    context.setFillColor(red: 0, green: 1, blue: 0, alpha: 1)
    context.fill(CGRect(x: 0, y: 0, width: 4, height: height))
    context.fill(CGRect(x: width - 4, y: 0, width: 4, height: height))
    return context.makeImage()
      .map { DesktopPicture(id: "\(width)x\(height)", image: $0) }
  }
}

import CoreGraphics
import SwiftUI
import Testing

/// A view drawn once at one pixel a point, in the dark appearance, for reading the colours it puts where.
@MainActor
struct Pixels {
  /// The drawing's pixels, RGBA, eight bits each, top row first.
  private let bytes: [UInt8]
  /// The drawing's width in pixels.
  let width: Int

  /// Draws `view`, settled.
  ///
  /// - Parameter view: The view.
  /// - Throws: When nothing was drawn.
  init(_ view: some View) throws {
    let renderer = ImageRenderer(content: view.environment(\.colorScheme, .dark))
    renderer.scale = 1
    let image = try #require(renderer.cgImage, "nothing was drawn")
    width = image.width
    var bytes = [UInt8](repeating: 0, count: image.width * image.height * 4)
    let drawn = bytes.withUnsafeMutableBytes { buffer -> Bool in
      guard
        let context = CGContext(
          data: buffer.baseAddress,
          width: image.width,
          height: image.height,
          bitsPerComponent: 8,
          bytesPerRow: image.width * 4,
          space: CGColorSpace(name: CGColorSpace.sRGB) ?? CGColorSpaceCreateDeviceRGB(),
          bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
        )
      else { return false }
      context.draw(image, in: CGRect(x: 0, y: 0, width: image.width, height: image.height))
      return true
    }
    try #require(drawn, "no bitmap to read")
    self.bytes = bytes
  }

  /// The colour at a point, top left first, its channels from 0 to 1 and un-premultiplied.
  ///
  /// - Parameters:
  ///   - x: Across, in points from the left.
  ///   - y: Down, in points from the top.
  /// - Returns: Red, green, blue and alpha.
  func colour(x: Int, y: Int) -> Colour {
    let index = (y * width + x) * 4
    let alpha = Double(bytes[index + 3]) / 255
    guard alpha > 0 else { return Colour(red: 0, green: 0, blue: 0, alpha: 0) }
    let channel = { (offset: Int) in min(1, Double(bytes[index + offset]) / 255 / alpha) }
    return Colour(red: channel(0), green: channel(1), blue: channel(2), alpha: alpha)
  }

  /// Whether a colour is the test pictures' green.
  ///
  /// - Parameter colour: A drawn colour.
  /// - Returns: Whether it is.
  static func isGreen(_ colour: Colour) -> Bool {
    colour.green > 0.8 && colour.red < 0.3 && colour.alpha > 0.9
  }

  /// Whether a colour is the test pictures' red.
  ///
  /// - Parameter colour: A drawn colour.
  /// - Returns: Whether it is.
  static func isRed(_ colour: Colour) -> Bool {
    colour.red > 0.8 && colour.green < 0.3 && colour.alpha > 0.9
  }
}

import CoreGraphics
import SwiftUI
import Testing

/// A panel piece drawn once, for reading the colours it puts where.
@MainActor
struct PanelPixels {
  /// The drawing's pixels, RGBA, eight bits each, top row first.
  private let bytes: [UInt8]
  /// The drawing's width in pixels.
  let width: Int
  /// The drawing's height in pixels.
  let height: Int

  /// Draws `view`, settled, as a render draws it.
  ///
  /// - Parameters:
  ///   - view: The view.
  ///   - colorScheme: The appearance, independent of the desktop's.
  ///   - scale: The number of pixels per point.
  /// - Throws: When nothing was drawn.
  init(_ view: some View, colorScheme: ColorScheme = .dark, scale: CGFloat = 1) throws {
    let renderer = ImageRenderer(
      content: view.environment(\.animatesArrivals, false)
        .environment(\.colorScheme, colorScheme)
    )
    renderer.scale = scale
    let image = try #require(renderer.cgImage, "nothing was drawn")
    width = image.width
    height = image.height
    var bytes = [UInt8](repeating: 0, count: width * height * 4)
    let drawn = bytes.withUnsafeMutableBytes { buffer -> Bool in
      guard
        let context = CGContext(
          data: buffer.baseAddress,
          width: image.width,
          height: image.height,
          bitsPerComponent: 8,
          bytesPerRow: image.width * 4,
          space: CGColorSpace(name: CGColorSpace.sRGB) ?? CGColorSpaceCreateDeviceRGB(),
          bitmapInfo: CGBitmapInfo.byteOrder32Big.rawValue | CGImageAlphaInfo.premultipliedLast.rawValue
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
  ///   - x: Across, in pixels from the left.
  ///   - y: Down, in pixels from the top.
  /// - Returns: Red, green, blue and alpha.
  func colour(x: Int, y: Int) -> (red: Double, green: Double, blue: Double, alpha: Double) {
    let index = (y * width + x) * 4
    let alpha = Double(bytes[index + 3]) / 255
    guard alpha > 0 else { return (0, 0, 0, 0) }
    let channel = { (offset: Int) in min(1, Double(bytes[index + offset]) / 255 / alpha) }
    return (channel(0), channel(1), channel(2), alpha)
  }

  /// How colourful a colour is: its brightest channel less its dimmest.
  ///
  /// - Parameter colour: The colour.
  /// - Returns: From 0, grey, to 1.
  static func chroma(_ colour: (red: Double, green: Double, blue: Double, alpha: Double)) -> Double {
    max(colour.red, colour.green, colour.blue) - min(colour.red, colour.green, colour.blue)
  }
}

import AppKit
import Foundation

/// A tiny PNG, as the service sends.
///
/// - Parameters:
///   - width: Its width in pixels.
///   - height: Its height in pixels.
///   - points: Its size in points, which the PNG records as its resolution; nil records none, as the helper's do.
/// - Returns: The PNG's bytes.
func png(width: Int = 4, height: Int = 3, points: CGSize? = nil) -> Data {
  let bitmap = NSBitmapImageRep(
    bitmapDataPlanes: nil,
    pixelsWide: width,
    pixelsHigh: height,
    bitsPerSample: 8,
    samplesPerPixel: 4,
    hasAlpha: true,
    isPlanar: false,
    colorSpaceName: .deviceRGB,
    bytesPerRow: 0,
    bitsPerPixel: 0
  )
  if let points { bitmap?.size = points }
  return bitmap?.representation(using: .png, properties: [:]) ?? Data()
}

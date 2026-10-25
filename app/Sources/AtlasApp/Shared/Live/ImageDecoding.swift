import CoreGraphics
import Foundation
import ImageIO

/// Decodes the pictures the tabs read, a screenshot's or an app icon's, off the main actor, so a large PNG never
/// holds the window up.
enum ImageDecoding {
  /// The first image a PNG's or JPEG's bytes hold.
  ///
  /// - Parameter data: The image's bytes.
  /// - Returns: The image, or nil when the bytes are not one.
  @concurrent nonisolated static func image(_ data: Data) async -> CGImage? {
    guard let source = CGImageSourceCreateWithData(data as CFData, nil) else { return nil }
    // Decoded here and kept decoded: an image left lazy decodes on the main thread the first time it is drawn.
    let options = [kCGImageSourceShouldCacheImmediately: true, kCGImageSourceShouldCache: true] as CFDictionary
    return CGImageSourceCreateImageAtIndex(source, 0, options)
  }
}

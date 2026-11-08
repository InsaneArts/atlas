import Foundation
import ImageIO
import UniformTypeIdentifiers

/// Encodes the pictures the person copies or saves off the main actor, so what reaches the clipboard or a file is
/// always the PNG it says it is, a picture kept as JPEG included.
enum ImageEncoding {
  /// The picture as PNG: as it is when it already is one, else decoded and encoded again.
  ///
  /// - Parameter data: The picture's bytes, PNG or JPEG.
  /// - Returns: The PNG, or nil when the bytes are not a picture.
  @concurrent nonisolated static func png(_ data: Data) async -> Data? {
    guard let source = CGImageSourceCreateWithData(data as CFData, nil) else { return nil }
    if CGImageSourceGetType(source) as String? == UTType.png.identifier { return data }
    guard let image = CGImageSourceCreateImageAtIndex(source, 0, nil) else { return nil }
    let png = NSMutableData()
    let type = UTType.png.identifier as CFString
    guard let destination = CGImageDestinationCreateWithData(png as CFMutableData, type, 1, nil) else { return nil }
    CGImageDestinationAddImage(destination, image, nil)
    return CGImageDestinationFinalize(destination) ? png as Data : nil
  }
}

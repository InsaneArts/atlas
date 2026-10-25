import AtlasService
import AtlasUI
import CoreGraphics
import ImageIO

extension DesktopPicture {
  /// A frame's picture, decoded once, with the size in points that input is mapped through.
  struct Decoded: Sendable {
    /// The picture, ready to draw.
    let picture: DesktopPicture
    /// The picture's size in points: its pixels at the PNG's own resolution, 72 dots an inch when it names none.
    /// The helper's PNGs name none, so their points are their pixels; mapping reads only the shape.
    let size: CGSize
  }

  /// The resolution a PNG that names none is read at, in dots an inch, as AppKit reads one: a point a pixel.
  nonisolated static let pointsPerInch = 72.0

  /// The picture a frame carries; nil when its data is not an image.
  ///
  /// - Parameter frame: A picture of the helper's desktop, as the service sent it.
  init?(frame: Frame) {
    guard let decoded = Self.decoded(frame) else { return nil }
    self = decoded.picture
  }

  /// Decodes a frame's PNG once: the picture and its size in points.
  ///
  /// - Parameter frame: A picture of the helper's desktop, as the service sent it.
  /// - Returns: The picture and its size, or nil when its data is not an image.
  @concurrent nonisolated static func decode(_ frame: Frame) async -> Decoded? {
    decoded(frame)
  }

  /// Creates a frame's picture with its pixels decoded before it is drawn.
  ///
  /// - Parameter frame: A picture of the helper's desktop, as the service sent it.
  /// - Returns: The picture and its size, or nil when its data is not an image.
  private nonisolated static func decoded(_ frame: Frame) -> Decoded? {
    let options = [kCGImageSourceShouldCacheImmediately: true, kCGImageSourceShouldCache: true] as CFDictionary
    guard
      let source = CGImageSourceCreateWithData(frame.png as CFData, nil),
      let image = CGImageSourceCreateImageAtIndex(source, 0, options)
    else { return nil }
    let properties = CGImageSourceCopyPropertiesAtIndex(source, 0, nil) as? [CFString: Any]
    let resolution = { (key: CFString) -> Double in
      guard let dots = properties?[key] as? Double, dots > 0 else { return pointsPerInch }
      return dots
    }
    let size = CGSize(
      width: Double(image.width) * pointsPerInch / resolution(kCGImagePropertyDPIWidth),
      height: Double(image.height) * pointsPerInch / resolution(kCGImagePropertyDPIHeight)
    )
    return Decoded(picture: DesktopPicture(id: frame.id, image: image), size: size)
  }
}

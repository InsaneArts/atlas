import AtlasUI

/// The screenshots' thumbnails read so far, by name, keeping only the newest ``limit`` so a long session never holds
/// every one.
struct ThumbnailCache {
  /// 96: about six screens of the four-column grid, so scrolling back never reads them again, at roughly 0.6 MB a
  /// decoded 480-point thumbnail, under 60 MB in all.
  static let limit = 96

  /// The thumbnails, by name.
  private(set) var pictures: [String: DesktopPicture] = [:]
  /// The names, oldest kept first.
  private var order: [String] = []

  /// The thumbnail of `name`, if read.
  ///
  /// - Parameter name: The screenshot's file name.
  subscript(name: String) -> DesktopPicture? { pictures[name] }

  /// Keeps a thumbnail, letting the oldest go past ``limit``.
  ///
  /// - Parameters:
  ///   - picture: The thumbnail.
  ///   - name: The screenshot's file name.
  mutating func keep(_ picture: DesktopPicture, for name: String) {
    if pictures.updateValue(picture, forKey: name) == nil { order.append(name) }
    while order.count > Self.limit {
      pictures.removeValue(forKey: order.removeFirst())
    }
  }
}

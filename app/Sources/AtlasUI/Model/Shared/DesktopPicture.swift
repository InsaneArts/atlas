import CoreGraphics

/// One picture of the helper's desktop, ready to draw. Two pictures are equal when they are the same frame, so a
/// state never compares pixels.
package struct DesktopPicture: Equatable, Sendable {
  /// The frame's id, as the service named it.
  package var id: String
  /// The picture's pixels.
  package var image: CGImage

  /// Creates a picture from a frame's id and its decoded pixels.
  ///
  /// - Parameters:
  ///   - id: The frame's id, as the service named it.
  ///   - image: The picture's pixels.
  package init(id: String, image: CGImage) {
    self.id = id
    self.image = image
  }

  /// Whether two pictures are the same frame.
  ///
  /// - Parameters:
  ///   - lhs: One picture.
  ///   - rhs: The other.
  /// - Returns: Whether their frame ids are equal, whatever their pixels.
  package static func == (lhs: DesktopPicture, rhs: DesktopPicture) -> Bool {
    lhs.id == rhs.id
  }
}

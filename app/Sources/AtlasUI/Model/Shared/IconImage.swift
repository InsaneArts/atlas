import CoreGraphics

/// An app's icon, ready to draw. Two icons are equal when they are the same app's, so a state never compares pixels.
package struct IconImage: Equatable, Sendable {
  /// What the icon belongs to, such as the app's path.
  package var id: String
  /// The icon's pixels.
  package var image: CGImage

  /// Creates an icon.
  ///
  /// - Parameters:
  ///   - id: What the icon belongs to.
  ///   - image: The icon's pixels.
  package init(id: String, image: CGImage) {
    self.id = id
    self.image = image
  }

  /// Whether two icons are the same app's.
  ///
  /// - Parameters:
  ///   - lhs: One icon.
  ///   - rhs: The other.
  /// - Returns: Whether their ids are equal, whatever their pixels.
  package static func == (lhs: IconImage, rhs: IconImage) -> Bool {
    lhs.id == rhs.id
  }
}

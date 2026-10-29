import AtlasUI
import CoreGraphics

/// An app's icon found: its picture, its PNG's digest, and the bundle identifier it was found under, when known.
nonisolated struct AppIconFound: Sendable {
  /// The icon, decoded.
  var image: CGImage
  /// The PNG's digest, which tells one icon from a changed one.
  var sha: String
  /// The bundle identifier it was found under, which a name-only want learns.
  var bundleId: String?

  /// 8: the digits of the digest an icon's drawn identity carries, so a changed icon is drawn anew.
  static let digits = 8

  /// The icon as a window draws it, known by what it was wanted under and its digest.
  ///
  /// - Parameter key: The text of the key it was wanted under (``AppIconKey/text``).
  /// - Returns: The image.
  func drawn(as key: String) -> IconImage {
    IconImage(id: "\(key) \(sha.prefix(Self.digits))", image: image)
  }
}

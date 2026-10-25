import AppKit
import SwiftUI

/// The helper account's picture, for the `accountPicture` environment value: `HelperPicture.png` from Atlas.app's
/// resources, or, drawing outside the app as the render modes and tests do, the file beside the sources.
enum AccountPictureImage {
  /// The picture to draw, or nil when the file cannot be read.
  static let image: Image? = picture.map { Image(nsImage: $0) }

  /// The picture as AppKit reads it.
  private static let picture: NSImage? = {
    if Bundle.main.bundlePath.hasSuffix(".app"),
      let bundled = Bundle.main.url(forResource: "HelperPicture", withExtension: "png")
    {
      return NSImage(contentsOf: bundled)
    }
    let source = URL(fileURLWithPath: #filePath)
      .deletingLastPathComponent()
      .deletingLastPathComponent()
      .deletingLastPathComponent()
      .deletingLastPathComponent()
      .deletingLastPathComponent()
      .appendingPathComponent("Resources/HelperPicture.png")
    return NSImage(contentsOf: source)
  }()
}

import AppKit
import SwiftUI

/// The app's own icon, for the `appIcon` environment value: Atlas.app's, or, drawing outside the app as the render
/// modes and tests do, the icon file beside the sources.
enum AppIconImage {
  /// The icon to draw.
  static let image = Image(nsImage: icon)

  /// The icon as AppKit has it, as the permission panel drags it.
  static let icon: NSImage = {
    if Bundle.main.bundlePath.hasSuffix(".app"), let icon = NSApp?.applicationIconImage {
      return icon
    }
    let source = URL(fileURLWithPath: #filePath)
      .deletingLastPathComponent()
      .deletingLastPathComponent()
      .deletingLastPathComponent()
      .deletingLastPathComponent()
      .deletingLastPathComponent()
      .appendingPathComponent("Resources/AppIcon.icns")
    return NSImage(contentsOf: source) ?? NSImage()
  }()
}

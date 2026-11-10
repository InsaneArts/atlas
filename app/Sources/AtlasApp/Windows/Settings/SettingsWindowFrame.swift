import AtlasUI
import CoreGraphics

/// Where the Settings window stands: at the size the person left it, never smaller than its first 740 by 572, kept
/// whole on the screen it is on. A place restored from an earlier launch, or one the window was left at before the
/// screen changed, shrinks to the screen and moves just enough to fit, and a screen too small holds it from its
/// top-left corner.
enum SettingsWindowFrame {
  /// The smallest the window may be.
  static let smallest = CGSize(width: SettingsLayout.width, height: SettingsLayout.height)

  /// The window's frame at its size now, at least the smallest and at most the screen's, moved inside the screen's
  /// visible frame when it runs past an edge.
  ///
  /// - Parameters:
  ///   - current: The window's frame now, whose top-left corner it keeps where it can.
  ///   - visible: The screen's visible frame, or nil for none known.
  /// - Returns: The frame.
  static func frame(current: CGRect, within visible: CGRect? = nil) -> CGRect {
    var size = CGSize(width: max(current.width, smallest.width), height: max(current.height, smallest.height))
    if let visible {
      size.width = min(size.width, max(visible.width, smallest.width))
      size.height = min(size.height, max(visible.height, smallest.height))
    }
    var frame = CGRect(x: current.minX, y: current.maxY - size.height, width: size.width, height: size.height)
    guard let visible else { return frame }
    frame.origin.x = min(max(frame.minX, visible.minX), max(visible.maxX - size.width, visible.minX))
    frame.origin.y = max(min(frame.minY, visible.maxY - size.height), min(visible.minY, visible.maxY - size.height))
    return frame
  }
}

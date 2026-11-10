import AtlasUI
import CoreGraphics

/// Where the Settings window stands: always its fixed size, kept whole on the screen it is on. A place restored from
/// an earlier launch, or one the window was left at before the screen changed, is moved just enough to fit, and a
/// screen too small holds it from its top-left corner.
enum SettingsWindowFrame {
  /// The window's frame at its fixed size, moved inside the screen's visible frame when it runs past an edge.
  ///
  /// - Parameters:
  ///   - current: The window's frame now, whose top-left corner it keeps where it can.
  ///   - visible: The screen's visible frame, or nil for none known.
  /// - Returns: The frame.
  static func frame(current: CGRect, within visible: CGRect? = nil) -> CGRect {
    let size = CGSize(width: SettingsLayout.width, height: SettingsLayout.height)
    var frame = CGRect(x: current.minX, y: current.maxY - size.height, width: size.width, height: size.height)
    guard let visible else { return frame }
    frame.origin.x = min(max(frame.minX, visible.minX), max(visible.maxX - size.width, visible.minX))
    frame.origin.y = max(min(frame.minY, visible.maxY - size.height), min(visible.minY, visible.maxY - size.height))
    return frame
  }
}

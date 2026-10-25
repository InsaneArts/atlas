import AtlasUI
import CoreGraphics

/// Where the Settings window goes as it fits a pane: as tall as the pane, its top edge where it is, and
/// never taller than the screen nor past its bottom. The window has a full-size content view, so its frame is as tall
/// as its content.
enum SettingsWindowFrame {
  /// The frame that fits a content height, from wherever the window is now, mid-resize included.
  ///
  /// - Parameters:
  ///   - current: The window's frame now.
  ///   - contentHeight: The height to fit: the title and tabs over the pane.
  ///   - visible: The screen's visible frame, or nil for none known.
  /// - Returns: The frame: Settings' width, the content's height, and the current top edge; a frame the screen
  ///   cannot hold is cut to its height and lifted off its bottom.
  static func frame(current: CGRect, contentHeight: CGFloat, within visible: CGRect? = nil) -> CGRect {
    let height = visible.map { min(contentHeight, $0.height) } ?? contentHeight
    var bottom = current.maxY - height
    if let visible, bottom < visible.minY { bottom = visible.minY }
    return CGRect(x: current.minX, y: bottom, width: SettingsLayout.width, height: height)
  }

  /// Whether the window eases to its new frame, or jumps: it eases only while on screen and Reduce Motion is off.
  ///
  /// - Parameters:
  ///   - isVisible: Whether the window is on screen.
  ///   - reduceMotion: Whether the Reduce Motion setting is on.
  /// - Returns: Whether the resize animates.
  static func animates(isVisible: Bool, reduceMotion: Bool) -> Bool {
    isVisible && !reduceMotion
  }
}

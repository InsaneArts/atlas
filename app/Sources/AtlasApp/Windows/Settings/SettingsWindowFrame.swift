import AtlasUI
import CoreGraphics

/// Validates restored and user-sized Settings frames against the usable screen, without fitting page content.
enum SettingsWindowFrame {
  /// The smallest usable size, reduced only when the physical screen is smaller.
  ///
  /// - Parameter visible: The usable screen bounds, when known.
  /// - Returns: The minimum frame size for that screen.
  static func minimum(within visible: CGRect?) -> CGSize {
    CGSize(
      width: min(SettingsLayout.minimumWidth, visible?.width ?? SettingsLayout.minimumWidth),
      height: min(SettingsLayout.minimumHeight, visible?.height ?? SettingsLayout.minimumHeight)
    )
  }

  /// Keeps the user's size where possible, enlarging old narrow frames and clamping all edges to the screen.
  ///
  /// - Parameters:
  ///   - current: A restored or user-sized frame.
  ///   - visible: The usable screen bounds, when known.
  /// - Returns: A usable frame, preserving the top edge when its minimum height grows.
  static func frame(current: CGRect, within visible: CGRect? = nil) -> CGRect {
    let minimum = minimum(within: visible)
    let width = min(max(current.width, minimum.width), visible?.width ?? .greatestFiniteMagnitude)
    let height = min(max(current.height, minimum.height), visible?.height ?? .greatestFiniteMagnitude)
    var origin = CGPoint(x: current.minX, y: current.maxY - height)
    if let visible {
      origin.x = min(max(origin.x, visible.minX), visible.maxX - width)
      origin.y = min(max(origin.y, visible.minY), visible.maxY - height)
    }
    return CGRect(origin: origin, size: CGSize(width: width, height: height))
  }
}

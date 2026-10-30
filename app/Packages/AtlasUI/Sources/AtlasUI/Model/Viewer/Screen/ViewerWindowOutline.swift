import CoreGraphics

/// A window shown on the screen from the Apps pane: where it lies, as shares of the helper's display, and its label.
public struct ViewerWindowOutline: Equatable, Sendable {
  /// Where the window lies: each edge as a share of the display, 0 at its left or top and 1 at its right or bottom.
  public var frame: CGRect
  /// The app and the window, such as "Opie · Ring settings".
  public var title: String
  /// The window's size in points, such as "520 × 860".
  public var size: String
  /// The app's icon, or the tile drawn from its name; nil draws none.
  public var icon: AppIconFace?

  /// Creates an outline.
  ///
  /// - Parameters:
  ///   - frame: Where the window lies, as shares of the display.
  ///   - title: The app and the window.
  ///   - size: The window's size in points.
  ///   - icon: The app's icon.
  public init(frame: CGRect, title: String, size: String, icon: AppIconFace? = nil) {
    self.frame = frame
    self.title = title
    self.size = size
    self.icon = icon
  }
}

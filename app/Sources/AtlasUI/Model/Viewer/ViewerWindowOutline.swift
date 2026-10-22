import CoreGraphics

/// A window shown on the screen from the Apps pane: where it lies, as shares of the helper's display, and its label.
package struct ViewerWindowOutline: Equatable, Sendable {
  /// Where the window lies: each edge as a share of the display, 0 at its left or top and 1 at its right or bottom.
  package var frame: CGRect
  /// The app and the window, such as "Opie · Ring settings".
  package var title: String
  /// The window's size in points, such as "520 × 860".
  package var size: String
  /// The app's icon; nil draws none.
  package var icon: IconImage?

  /// Creates an outline.
  ///
  /// - Parameters:
  ///   - frame: Where the window lies, as shares of the display.
  ///   - title: The app and the window.
  ///   - size: The window's size in points.
  ///   - icon: The app's icon.
  package init(frame: CGRect, title: String, size: String, icon: IconImage? = nil) {
    self.frame = frame
    self.title = title
    self.size = size
    self.icon = icon
  }
}

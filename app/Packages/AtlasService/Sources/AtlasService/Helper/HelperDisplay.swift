/// The helper's display as the Apps list measures its windows against it: its size in points, from its top-leading
/// corner.
public struct HelperDisplay: Equatable, Sendable {
  /// The display's width, in points.
  public var width: Double
  /// The display's height, in points.
  public var height: Double

  /// Creates a display.
  ///
  /// - Parameters:
  ///   - width: Its width, in points.
  ///   - height: Its height, in points.
  public init(width: Double, height: Double) {
    self.width = width
    self.height = height
  }

  /// Whether a point lies on the display.
  ///
  /// - Parameters:
  ///   - x: The point's distance from the left edge, in points.
  ///   - y: The point's distance from the top edge, in points.
  /// - Returns: Whether it does.
  func holds(x: Double, y: Double) -> Bool {
    x >= 0 && y >= 0 && x < width && y < height
  }
}

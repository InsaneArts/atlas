extension HelperWindow {
  /// Whether a point of the display lies inside the window.
  ///
  /// - Parameters:
  ///   - x: The point's distance from the display's left edge, in points.
  ///   - y: The point's distance from the display's top edge, in points.
  /// - Returns: Whether it does.
  func holds(x: Double, y: Double) -> Bool {
    x >= self.x && y >= self.y && x < self.x + width && y < self.y + height
  }

  /// Whether the window stands where `other` stood: the same place and size.
  ///
  /// - Parameter other: The window as an earlier reading listed it.
  /// - Returns: Whether neither moved nor resized.
  public func standsAsBefore(_ other: HelperWindow) -> Bool {
    x == other.x && y == other.y && width == other.width && height == other.height
  }
}

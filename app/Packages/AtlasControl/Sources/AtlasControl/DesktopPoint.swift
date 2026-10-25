import Foundation

/// A point on the helper's display, in its points.
public struct DesktopPoint: Equatable, Sendable {
  /// From the display's left edge, in points.
  public var x: Double
  /// From the display's top edge, in points.
  public var y: Double

  /// Creates a point.
  ///
  /// - Parameters:
  ///   - x: From the display's left edge, in points.
  ///   - y: From the display's top edge, in points.
  public init(x: Double, y: Double) {
    self.x = x
    self.y = y
  }

  /// How far it is to `other`, in points.
  func distance(to other: DesktopPoint) -> Double { hypot(x - other.x, y - other.y) }
}

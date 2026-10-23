import CoreGraphics

/// Where the drag ghost travels: from the permission panel's icon to System Settings' list, in the overlay's points,
/// measured from its top left.
package struct DragGhostState: Equatable, Sendable {
  /// The middle of the panel's icon.
  package var from: CGPoint
  /// Where Atlas lands in System Settings' list.
  package var to: CGPoint

  /// Creates the ghost's path.
  ///
  /// - Parameters:
  ///   - from: The middle of the panel's icon.
  ///   - to: Where Atlas lands in System Settings' list.
  package init(from: CGPoint, to: CGPoint) {
    self.from = from
    self.to = to
  }

  /// Where the ghost is `progress` of the way from the icon to the list.
  ///
  /// - Parameter progress: How far along, from 0 to 1.
  /// - Returns: The point.
  package func point(at progress: Double) -> CGPoint {
    CGPoint(x: from.x + (to.x - from.x) * progress, y: from.y + (to.y - from.y) * progress)
  }
}

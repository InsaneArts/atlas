import CoreGraphics
import SwiftUI

/// The corner of the stage the small screen keeps to over Activity and Apps. The person can drag it anywhere; let go,
/// it settles in the nearest corner, which it keeps while the window is open.
enum MiniCorner: CaseIterable, Sendable {
  /// Top left.
  case topLeading
  /// Top right.
  case topTrailing
  /// Bottom left.
  case bottomLeading
  /// Bottom right, where it starts.
  case bottomTrailing

  /// Where in the stage it sits.
  var alignment: Alignment {
    switch self {
    case .topLeading: .topLeading
    case .topTrailing: .topTrailing
    case .bottomLeading: .bottomLeading
    case .bottomTrailing: .bottomTrailing
    }
  }

  /// Whether it lies on the stage's left.
  var isLeading: Bool {
    self == .topLeading || self == .bottomLeading
  }

  /// Whether it lies along the stage's top.
  var isTop: Bool {
    self == .topLeading || self == .topTrailing
  }

  /// The corner nearest a point: the quarter of the stage it falls in, a point on a middle line going right or down.
  ///
  /// - Parameters:
  ///   - point: The small screen's centre, in the stage's points from its top left.
  ///   - size: The stage's size.
  /// - Returns: The corner.
  static func nearest(to point: CGPoint, in size: CGSize) -> MiniCorner {
    let leading = point.x < size.width / 2
    let top = point.y < size.height / 2
    switch (top, leading) {
    case (true, true): return .topLeading
    case (true, false): return .topTrailing
    case (false, true): return .bottomLeading
    case (false, false): return .bottomTrailing
    }
  }

  /// The small screen's centre in this corner of a stage of `size`, ``ViewerLayout/miniInset`` from its two edges.
  ///
  /// - Parameter size: The stage's size.
  /// - Returns: The centre, in the stage's points from its top left.
  func centre(in size: CGSize) -> CGPoint {
    let mini = ViewerLayout.miniSize
    let inset = ViewerLayout.miniInset
    let x = isLeading ? inset + mini.width / 2 : size.width - inset - mini.width / 2
    let y = isTop ? inset + mini.height / 2 : size.height - inset - mini.height / 2
    return CGPoint(x: x, y: y)
  }

  /// The corner the small screen settles in once let go of: the nearest to where the drag left its centre.
  ///
  /// - Parameters:
  ///   - translation: How far it was dragged from this corner.
  ///   - size: The stage's size.
  /// - Returns: The corner.
  func settling(after translation: CGSize, in size: CGSize) -> MiniCorner {
    let from = centre(in: size)
    return Self.nearest(to: CGPoint(x: from.x + translation.width, y: from.y + translation.height), in: size)
  }
}

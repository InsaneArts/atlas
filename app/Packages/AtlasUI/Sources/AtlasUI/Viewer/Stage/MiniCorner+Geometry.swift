import CoreGraphics
import SwiftUI

extension MiniCorner {
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

  /// The small screen's centre in this corner of a stage of `size`, ``ViewerLayout/pipInset`` from its two edges.
  ///
  /// - Parameters:
  ///   - size: The stage's size.
  ///   - mini: The small screen's size.
  /// - Returns: The centre, in the stage's points from its top left.
  func centre(in size: CGSize, mini: CGSize) -> CGPoint {
    let inset = ViewerLayout.pipInset
    let x = isLeading ? inset + mini.width / 2 : size.width - inset - mini.width / 2
    let y = isTop ? inset + mini.height / 2 : size.height - inset - mini.height / 2
    return CGPoint(x: x, y: y)
  }

  /// The corner the small screen settles in once let go of: the nearest to where its momentum would carry its centre,
  /// so a quick throw toward a corner lands there even when the pointer stopped short.
  ///
  /// - Parameters:
  ///   - translation: How far it was dragged from this corner, where the pointer would come to rest.
  ///   - size: The stage's size.
  ///   - mini: The small screen's size.
  /// - Returns: The corner.
  func settling(after translation: CGSize, in size: CGSize, mini: CGSize) -> MiniCorner {
    let from = centre(in: size, mini: mini)
    return Self.nearest(to: CGPoint(x: from.x + translation.width, y: from.y + translation.height), in: size)
  }

  /// How fast the small screen is going toward `corner` as it is let go, as the spring that lands it takes it: its
  /// speed along the way left, as a share of that way each second. Nothing when it is already there or thrown away
  /// from it, so it never sets off backwards.
  ///
  /// - Parameters:
  ///   - corner: The corner it lands in.
  ///   - translation: How far it was dragged from this corner when let go.
  ///   - velocity: How fast the pointer moved as it let go, in points a second.
  ///   - size: The stage's size.
  ///   - mini: The small screen's size.
  /// - Returns: The velocity, zero or more.
  func landingVelocity(
    to corner: MiniCorner,
    after translation: CGSize,
    velocity: CGSize,
    in size: CGSize,
    mini: CGSize
  ) -> Double {
    let from = centre(in: size, mini: mini)
    let to = corner.centre(in: size, mini: mini)
    let way = CGVector(dx: to.x - from.x - translation.width, dy: to.y - from.y - translation.height)
    let length = way.dx * way.dx + way.dy * way.dy
    guard length > 0 else { return 0 }
    return max(0, Double((velocity.width * way.dx + velocity.height * way.dy) / length))
  }
}

import SwiftUI

/// The badge's own measures.
enum StateBadgeLayout {
  /// 11: the dot's side.
  static let side: CGFloat = 11
  /// 2: the ground ring round the dot, and the waiting dot's inner ring.
  static let ring: CGFloat = 2
  /// 15: the badge's whole side, its dot and ground ring.
  static let reach = side + 2 * ring
  /// 0.8: the scale a badge grows from and shrinks to, near its size, so it reads as arriving rather than popping.
  static let arrivingScale: CGFloat = 0.8
}

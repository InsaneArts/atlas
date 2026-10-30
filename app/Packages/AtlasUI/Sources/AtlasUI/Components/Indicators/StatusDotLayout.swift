import SwiftUI

/// The marks' own measures for status dots and squares.
enum StatusDotLayout {
  /// 7: a status dot.
  static let regular: CGFloat = 7
  /// 8: a status dot beside a heading.
  static let large: CGFloat = 8
  /// 8: the stopped square.
  static let square: CGFloat = 8
  /// 2: the square's corners, softened as the system's stop buttons are.
  static let squareCorner: CGFloat = 2
  /// 4: the glow around a dot, half an equivalent 8-point CSS blur, since a shadow's radius spreads about
  /// twice as far.
  static let glow: CGFloat = 4
  /// 0.35: the low point of a breath.
  static let dimmed = 0.35
}

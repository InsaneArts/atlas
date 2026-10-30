import SwiftUI

/// The check path's coordinate space and stroke width.
enum CheckStrokeLayout {
  /// 12: the square the path is drawn on.
  static let square: CGFloat = 12
  /// The stroke's start, at the left.
  static let start = CGPoint(x: 2.2, y: 6.3)
  /// Its corner, at the bottom.
  static let corner = CGPoint(x: 4.9, y: 8.9)
  /// Its end, at the top right.
  static let end = CGPoint(x: 9.8, y: 3.3)
}

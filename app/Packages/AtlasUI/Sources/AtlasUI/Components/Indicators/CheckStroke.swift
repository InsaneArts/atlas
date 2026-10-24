import SwiftUI

/// A check drawn as one open stroke, so it can draw itself along its length with `trim` (v3 S3, X11). The path is
/// v3's `M2.2 6.3 L4.9 8.9 L9.8 3.3` on a 12-point square, scaled to the frame.
struct CheckStroke: Shape {
  /// The check's path in the frame.
  ///
  /// - Parameter rect: The frame to draw the check in.
  /// - Returns: The check's path.
  func path(in rect: CGRect) -> Path {
    var path = Path()
    path.move(to: point(Layout.start, in: rect))
    path.addLine(to: point(Layout.corner, in: rect))
    path.addLine(to: point(Layout.end, in: rect))
    return path
  }

  /// A point of v3's 12-point square, scaled into the frame.
  ///
  /// - Parameters:
  ///   - unit: The point on the 12-point square.
  ///   - rect: The frame.
  /// - Returns: The point in the frame.
  private func point(_ unit: CGPoint, in rect: CGRect) -> CGPoint {
    CGPoint(
      x: rect.minX + unit.x / Layout.square * rect.width,
      y: rect.minY + unit.y / Layout.square * rect.height
    )
  }
}

/// v3's check path (`ckSvg`).
private enum Layout {
  /// 12: the square the path is drawn on.
  static let square: CGFloat = 12
  /// The stroke's start, at the left.
  static let start = CGPoint(x: 2.2, y: 6.3)
  /// Its corner, at the bottom.
  static let corner = CGPoint(x: 4.9, y: 8.9)
  /// Its end, at the top right.
  static let end = CGPoint(x: 9.8, y: 3.3)
}

#if DEBUG
  #Preview("Check") {
    CheckStroke()
      .stroke(Color.textPrimary, style: StrokeStyle(lineWidth: Stroke.thick, lineCap: .round, lineJoin: .round))
      .frame(width: IconSize.m, height: IconSize.m)
      .sampleSurface()
  }
#endif

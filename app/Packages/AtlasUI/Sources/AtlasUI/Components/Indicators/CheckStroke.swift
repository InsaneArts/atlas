import SwiftUI

/// A check drawn as one open stroke, so it can draw itself along its length with `trim`. The path is `M2.2
/// 6.3 L4.9 8.9 L9.8 3.3` on a 12-point square, scaled to the frame.
struct CheckStroke: Shape {
  /// The check's path in the frame.
  ///
  /// - Parameter rect: The frame to draw the check in.
  /// - Returns: The check's path.
  func path(in rect: CGRect) -> Path {
    var path = Path()
    path.move(to: point(CheckStrokeLayout.start, in: rect))
    path.addLine(to: point(CheckStrokeLayout.corner, in: rect))
    path.addLine(to: point(CheckStrokeLayout.end, in: rect))
    return path
  }

  /// A point of the path's 12-point square, scaled into the frame.
  ///
  /// - Parameters:
  ///   - unit: The point on the 12-point square.
  ///   - rect: The frame.
  /// - Returns: The point in the frame.
  private func point(_ unit: CGPoint, in rect: CGRect) -> CGPoint {
    CGPoint(
      x: rect.minX + unit.x / CheckStrokeLayout.square * rect.width,
      y: rect.minY + unit.y / CheckStrokeLayout.square * rect.height
    )
  }
}

#if DEBUG
  #Preview("Check") {
    CheckStroke()
      .stroke(Color.textPrimary, style: StrokeStyle(lineWidth: Stroke.thick, lineCap: .round, lineJoin: .round))
      .frame(width: IconSize.m, height: IconSize.m)
      .sampleSurface()
  }
#endif

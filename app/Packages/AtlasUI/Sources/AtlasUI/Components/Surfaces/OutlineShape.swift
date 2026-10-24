import SwiftUI

/// A rounded rectangle round a frame, `outset` outside it, whose frame animates, so a spotlight glides between frames
/// instead of jumping. Cut out, it is the whole space less the rectangle, for an even-odd fill that dims everything
/// else.
struct OutlineShape: Shape {
  /// The frame outlined, in the shape's own space.
  var frame: CGRect
  /// How far outside the frame the outline lies.
  let outset: CGFloat
  /// Whether the shape is the whole space with the rectangle cut out of it.
  var isCutOut = false

  /// The frame's origin and size, which SwiftUI moves between two frames.
  var animatableData: AnimatablePair<AnimatablePair<CGFloat, CGFloat>, AnimatablePair<CGFloat, CGFloat>> {
    get {
      AnimatablePair(
        AnimatablePair(frame.origin.x, frame.origin.y),
        AnimatablePair(frame.size.width, frame.size.height)
      )
    }
    set {
      frame = CGRect(
        x: newValue.first.first,
        y: newValue.first.second,
        width: newValue.second.first,
        height: newValue.second.second
      )
    }
  }

  /// The outline, and the whole space round it when cut out.
  ///
  /// - Parameter rect: The space the shape is drawn in.
  /// - Returns: The path.
  func path(in rect: CGRect) -> Path {
    var path = Path()
    if isCutOut {
      path.addRect(rect)
    }
    let corner = Radius.extraSmall + outset
    path.addRoundedRect(
      in: frame.insetBy(dx: -outset, dy: -outset),
      cornerSize: CGSize(width: corner, height: corner)
    )
    return path
  }
}

#if DEBUG
  #Preview("Outline and cut-out") {
    HStack(spacing: Spacing.m) {
      OutlineShape(
        frame: CGRect(x: Spacing.l, y: Spacing.l, width: IconSize.xxl, height: IconSize.xxl),
        outset: Spacing.xs
      )
      .stroke(Color.accent, lineWidth: Stroke.thick)
      OutlineShape(
        frame: CGRect(x: Spacing.l, y: Spacing.l, width: IconSize.xxl, height: IconSize.xxl),
        outset: Spacing.xs,
        isCutOut: true
      )
      .fill(Color.letterbox, style: FillStyle(eoFill: true))
    }
    .frame(width: SampleWidth.narrow, height: SampleWidth.narrow / 2)
    .sampleSurface()
  }
#endif

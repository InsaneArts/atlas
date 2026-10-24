import SwiftUI

/// A part of a picture picked out as a spotlight (v3 V14): everything outside it dimmed, and a blue ring with a soft
/// blue halo around it. It takes the whole space it is given; `frame` is in that space. Its frame animates, so the
/// spotlight glides from one part to the next on whatever animation the change carries.
struct OutlineHighlight: View {
  /// The part picked out, in the view's own space.
  let frame: CGRect
  /// How dark the picture goes outside the part picked out.
  let dim: Double

  /// The dim round the part picked out, its halo and its ring.
  var body: some View {
    ZStack {
      OutlineShape(frame: frame, outset: .zero, isCutOut: true)
        .fill(
          Color.letterbox
            .opacity(dim),
          style: FillStyle(eoFill: true)
        )
      OutlineShape(frame: frame, outset: (Layout.haloWidth + Layout.ringWidth) / 2)
        .stroke(Color.accentRing, lineWidth: Layout.haloWidth - Layout.ringWidth)
      OutlineShape(frame: frame, outset: Layout.ringWidth / 2)
        .stroke(Color.accent, lineWidth: Layout.ringWidth)
    }
    .allowsHitTesting(false)
    .accessibilityHidden(true)
  }
}

/// A rounded rectangle round a frame, `outset` outside it, whose frame animates, so a spotlight glides between frames
/// instead of jumping. Cut out, it is the whole space less the rectangle, for an even-odd fill that dims everything
/// else.
private struct OutlineShape: Shape {
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

/// The outline's own measures, from v3's `.outline`.
private enum Layout {
  /// 2: the blue ring, just outside the part picked out.
  static let ringWidth = Stroke.thick
  /// 6: how far the halo reaches outside the part, the ring's 2 under its inner edge.
  static let haloWidth: CGFloat = 6
  #if DEBUG
    /// The part the preview picks out of the sample desktop: a window-sized piece near its top left.
    static let sampleFrame = CGRect(x: 120, y: 60, width: 220, height: 160)
    /// The dim the preview draws, the viewer's.
    static let sampleDim = 0.36
  #endif
}

#if DEBUG
  #Preview("Samples") {
    ZStack {
      if let picture = SampleDesktop.picture {
        PictureFrame(picture: picture)
      }
      OutlineHighlight(frame: Layout.sampleFrame, dim: Layout.sampleDim)
    }
    .frame(width: SampleWidth.column * 2, height: SampleWidth.column * 1.25)
    .sampleSurface()
  }
#endif

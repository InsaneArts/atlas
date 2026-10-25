import SwiftUI

/// A part of a picture picked out as a spotlight: everything outside it dimmed, and a blue ring with a soft
/// blue halo around it. It takes the whole space it is given; `frame` is in that space. Its frame animates,
/// so the spotlight glides from one part to the next on whatever animation the change carries.
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

/// The outline's own measures for its edge.
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

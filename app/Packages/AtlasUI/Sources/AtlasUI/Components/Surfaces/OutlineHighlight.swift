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
      OutlineShape(frame: frame, outset: (OutlineHighlightLayout.haloWidth + OutlineHighlightLayout.ringWidth) / 2)
        .stroke(Color.accentRing, lineWidth: OutlineHighlightLayout.haloWidth - OutlineHighlightLayout.ringWidth)
      OutlineShape(frame: frame, outset: OutlineHighlightLayout.ringWidth / 2)
        .stroke(Color.accent, lineWidth: OutlineHighlightLayout.ringWidth)
    }
    .allowsHitTesting(false)
    .accessibilityHidden(true)
  }
}

#if DEBUG
  #Preview("Samples") {
    ZStack {
      if let picture = SampleDesktop.picture {
        PictureFrame(picture: picture)
      }
      OutlineHighlight(frame: OutlineHighlightLayout.sampleFrame, dim: OutlineHighlightLayout.sampleDim)
    }
    .frame(width: SampleWidth.column * 2, height: SampleWidth.column * 1.25)
    .sampleSurface()
  }
#endif

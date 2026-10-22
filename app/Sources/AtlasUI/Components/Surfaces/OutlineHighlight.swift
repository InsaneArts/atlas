import SwiftUI

/// A part of a picture picked out: everything outside it dimmed, and a blue ring with a soft blue halo around it. It
/// takes the whole space it is given; `frame` is in that space.
struct OutlineHighlight: View {
  /// The part picked out, in the view's own space.
  let frame: CGRect

  var body: some View {
    GeometryReader { proxy in
      ZStack(alignment: .topLeading) {
        Path { path in
          path.addRect(CGRect(origin: .zero, size: proxy.size))
          path.addRoundedRect(in: frame, cornerSize: CGSize(width: Radius.medium, height: Radius.medium))
        }
        .fill(
          Color.stageBackground
            .opacity(Layout.dim),
          style: FillStyle(eoFill: true)
        )
        band(
          Color.accent
            .opacity(Layout.halo),
          outset: Layout.haloWidth,
          width: Layout.haloWidth - Stroke.thick
        )
        band(Color.accent, outset: Stroke.thick, width: Stroke.thick)
      }
    }
    .allowsHitTesting(false)
    .accessibilityHidden(true)
  }

  /// A band around the frame, its outer edge `outset` outside it, its corners following the frame's.
  ///
  /// - Parameters:
  ///   - color: The band's paint.
  ///   - outset: How far outside the frame its outer edge lies.
  ///   - width: How wide it is.
  /// - Returns: The band, placed.
  private func band(_ color: Color, outset: CGFloat, width: CGFloat) -> some View {
    RoundedRectangle(cornerRadius: Radius.medium + outset)
      .strokeBorder(color, lineWidth: width)
      .frame(width: frame.width + outset * 2, height: frame.height + outset * 2)
      .offset(x: frame.minX - outset, y: frame.minY - outset)
  }
}

/// The outline's own measures, from the board (mac.py `board_show`).
private enum Layout {
  /// 0.35: how dark the picture goes outside the part picked out.
  static let dim = 0.35
  /// 0.25: how strong the blue halo is.
  static let halo = 0.25
  /// 6: how far the halo reaches outside the frame, its inner 2 under the ring.
  static let haloWidth: CGFloat = 6
}

#if DEBUG
  #Preview("Samples") {
    ZStack {
      if let picture = SampleDesktop.picture {
        PictureFrame(picture: picture)
      }
      OutlineHighlight(frame: CGRect(x: 120, y: 60, width: 220, height: 160))
    }
    .frame(width: SampleWidth.column * 2, height: SampleWidth.column * 1.25)
    .sampleSurface()
  }
#endif

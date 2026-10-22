import SwiftUI

/// A window shown from the Apps tab, over the picture: the outline where the window lies, everything else dimmed,
/// and its label above it with the app's icon, the app and window's names and its size. It takes no clicks.
struct ScreenOutline: View {
  /// The window shown.
  let outline: ViewerWindowOutline
  /// The picture's size in pixels, which the outline is letterboxed by.
  let picture: CGSize

  var body: some View {
    GeometryReader { proxy in
      placed(ViewerLayout.outlineFrame(outline.frame, in: proxy.size, picture: picture))
    }
    .allowsHitTesting(false)
    .accessibilityElement(children: .combine)
  }

  /// The outline at `frame`, with its label above it, never above the picture's top nor past its sides; a long name
  /// is cut short to fit.
  ///
  /// - Parameter frame: Where the window lies in the view.
  /// - Returns: The outline and its label.
  private func placed(_ frame: CGRect) -> some View {
    ZStack(alignment: .topLeading) {
      OutlineHighlight(frame: frame)
      OutlineLabelPlacement(
        anchor: CGPoint(x: frame.minX, y: max(Spacing.m, frame.minY - ViewerLayout.outlineLabelLift))
      ) {
        FloatingChip {
          AppTile(image: outline.icon?.image, side: IconSize.radio)
          Text(outline.title)
            .truncationMode(.tail)
          Text(outline.size)
            .fontWeight(.regular)
            .foregroundStyle(Color.textTertiary)
            .monospacedDigit()
            .layoutPriority(1)
        }
      }
    }
  }
}

#if DEBUG
  #Preview("Show on screen") {
    if let outline = PreviewStates.viewerShowOnScreen.screen.outline, let picture = SampleDesktop.picture {
      ZStack {
        PictureFrame(picture: picture)
        ScreenOutline(outline: outline, picture: CGSize(width: picture.image.width, height: picture.image.height))
      }
      .aspectRatio(ViewerLayout.placeholderAspect, contentMode: .fit)
      .frame(width: SampleWidth.column * 2)
      .sampleSurface()
    }
  }
#endif

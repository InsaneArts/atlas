import SwiftUI

/// A window shown from the Apps tab, as a spotlight over the picture (v3 V14): everything but the window dimmed, the
/// outline round it, and its label on glass with the app's icon, the app and window's names and its size, above the
/// outline or inside it near an edge. Showing another window glides the outline and its label there; under Reduce
/// Motion they move at once. It takes no clicks.
struct ScreenOutline: View {
  /// The window shown.
  let outline: ViewerWindowOutline
  /// The picture's size in pixels, which the outline is letterboxed by.
  let picture: CGSize
  @Environment(\.accessibilityReduceMotion) private var reduceMotion

  /// The spotlight and its label, where the window lies in the picture.
  var body: some View {
    GeometryReader { proxy in
      placed(ViewerLayout.outlineFrame(outline.frame, in: proxy.size, picture: picture))
    }
    // Keyed on the window's place on the display, so a resized window redraws the outline with the picture at once.
    .animation(Motion.animation(Motion.state, reduceMotion: reduceMotion), value: outline.frame)
    .allowsHitTesting(false)
    .accessibilityElement(children: .combine)
  }

  /// The spotlight at `frame`, with its label.
  ///
  /// - Parameter frame: Where the window lies in the view.
  /// - Returns: The spotlight and its label.
  private func placed(_ frame: CGRect) -> some View {
    ZStack(alignment: .topLeading) {
      OutlineHighlight(frame: frame, dim: ViewerLayout.spotlightDim)
      OutlineLabelPlacement(outline: frame) {
        GlassChip {
          AppTile(image: outline.icon?.image, side: IconSize.s)
          Text(outline.title)
            .truncationMode(.tail)
          Text(outline.size)
            .font(Typography.digits(.callout))
            .foregroundStyle(Color.textSecondary)
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

  #Preview("Interactive") {
    @Previewable @State var atEdge = false
    if let picture = SampleDesktop.picture,
      let middle = PreviewStates.viewerShowOnScreen.screen.outline,
      let edge = PreviewStates.viewerShowOnScreenRightEdge.screen.outline
    {
      InteractivePreview {
        Toggle(String("At the right edge"), isOn: $atEdge)
      } content: {
        ZStack {
          PictureFrame(picture: picture)
          ScreenOutline(
            outline: atEdge ? edge : middle,
            picture: CGSize(width: picture.image.width, height: picture.image.height)
          )
        }
        .aspectRatio(ViewerLayout.placeholderAspect, contentMode: .fit)
        .frame(width: SampleWidth.column * 2)
      }
    }
  }
#endif

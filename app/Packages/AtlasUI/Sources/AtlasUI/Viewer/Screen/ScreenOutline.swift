import SwiftUI

/// A window shown from the Apps tab, as a spotlight over the picture: everything but the window dimmed, the outline
/// round it, and its label on glass with the app's icon, the app and window's names, its size and an × that stops
/// showing it (Escape does too), above the outline or inside it near an edge. Showing another window glides the
/// outline and its label there; under Reduce Motion they move at once. Only the × takes clicks.
struct ScreenOutline: View {
  /// The window shown.
  let outline: ViewerWindowOutline
  /// The picture's size in pixels, which the outline is letterboxed by.
  let picture: CGSize
  /// Receives the ×.
  let send: @MainActor (ViewerEvent) -> Void
  @Environment(\.accessibilityReduceMotion) private var reduceMotion

  /// The spotlight and its label, where the window lies in the picture.
  var body: some View {
    GeometryReader { proxy in
      placed(ViewerLayout.outlineFrame(outline.frame, in: proxy.size, picture: picture))
    }
    // Keyed on the window's place on the display, so a resized window redraws the outline with the picture at once.
    .animation(Motion.animation(Motion.state, reduceMotion: reduceMotion), value: outline.frame)
    .accessibilityElement(children: .contain)
  }

  /// The spotlight at `frame`, with its label.
  ///
  /// - Parameter frame: Where the window lies in the view.
  /// - Returns: The spotlight and its label.
  private func placed(_ frame: CGRect) -> some View {
    ZStack(alignment: .topLeading) {
      OutlineHighlight(frame: frame, dim: ViewerLayout.spotlightDim)
        .allowsHitTesting(false)
      OutlineLabelPlacement(outline: frame) {
        GlassChip {
          AppTile(image: outline.icon?.image, side: IconSize.s)
          Text(outline.title)
            .truncationMode(.tail)
          Text(outline.size)
            .font(Typography.digits(.callout))
            .foregroundStyle(Color.textSecondary)
            .layoutPriority(1)
          Button {
            send(.clearOutline)
          } label: {
            Image(systemName: SymbolName.clear.rawValue)
              .font(Typography.symbol(.row, weight: .semibold))
              .foregroundStyle(Color.textSecondary)
          }
          .buttonStyle(.plain)
          .help(String("Stop Showing (esc)"))
          .accessibilityLabel(Text(verbatim: "Stop showing the window"))
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
        ScreenOutline(
          outline: outline,
          picture: CGSize(width: picture.image.width, height: picture.image.height)
        ) { _ in }
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
          ) { _ in }
        }
        .aspectRatio(ViewerLayout.placeholderAspect, contentMode: .fit)
        .frame(width: SampleWidth.column * 2)
      }
    }
  }
#endif

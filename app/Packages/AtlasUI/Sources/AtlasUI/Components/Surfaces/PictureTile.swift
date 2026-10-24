import SwiftUI

/// A picture's thumbnail cut to its frame: the picture fills it edge to edge, on black until it arrives, with a fine
/// edge, or a blue ring and a soft blue halo around it while it is the one selected (v3 `.stile.sel`). The caller
/// gives its size. A picture that arrives fades in, or simply appears under Reduce Motion.
struct PictureTile: View {
  /// The picture; nil shows black until it arrives.
  let picture: DesktopPicture?
  /// The frame's corners: a tile's 10 by default, a small thumbnail's 5.
  var radius = Radius.medium
  /// Whether it is the one selected, which its ring shows.
  var isSelected = false
  @Environment(\.accessibilityReduceMotion) private var reduceMotion

  /// The picture in its frame, with its edge or its ring.
  var body: some View {
    Color.letterbox
      .overlay {
        if let picture {
          Image(decorative: picture.image, scale: 1)
            .resizable()
            .interpolation(.high)
            .aspectRatio(contentMode: .fill)
            .transition(.opacity)
        }
      }
      .animation(Motion.animation(Motion.fade, reduceMotion: reduceMotion), value: picture)
      .clipShape(RoundedRectangle(cornerRadius: radius))
      .overlay {
        if !isSelected {
          RoundedRectangle(cornerRadius: radius)
            .strokeBorder(Color.strokeStrong, lineWidth: Stroke.hairline)
        }
      }
      .background {
        if isSelected {
          ring
        }
      }
  }

  /// The selected ring: blue just outside the frame, then the soft accent ring outside that (v3 `0 0 0 2px accent,
  /// 0 0 0 5px ring`).
  private var ring: some View {
    ZStack {
      RoundedRectangle(cornerRadius: radius + Layout.haloReach)
        .fill(Color.accentRing)
        .padding(-Layout.haloReach)
      RoundedRectangle(cornerRadius: radius + Stroke.thick)
        .fill(Color.accent)
        .padding(-Stroke.thick)
    }
  }
}

/// The tile's own measures.
private enum Layout {
  /// 5: how far the soft ring reaches outside the frame, past the blue one's 2 (v3 `.stile.sel`).
  static let haloReach: CGFloat = 5
}

#if DEBUG
  #Preview("Samples") {
    HStack(spacing: Spacing.l) {
      PictureTile(picture: SampleDesktop.picture, isSelected: true)
        .aspectRatio(ViewerLayout.placeholderAspect, contentMode: .fit)
        .frame(width: SampleWidth.narrow)
      PictureTile(picture: SampleDesktop.picture)
        .aspectRatio(ViewerLayout.placeholderAspect, contentMode: .fit)
        .frame(width: SampleWidth.narrow)
      PictureTile(picture: nil)
        .aspectRatio(ViewerLayout.placeholderAspect, contentMode: .fit)
        .frame(width: SampleWidth.narrow)
    }
    .padding(Spacing.s)
    .sampleSurface()
  }
#endif

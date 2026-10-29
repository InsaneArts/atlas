import SwiftUI

/// A picture's thumbnail in its frame: the whole picture at its own shape, never cut or zoomed, on black until it
/// arrives and on that ground wherever the frame's shape is not the picture's; a fine edge, a brighter 1-point edge
/// under the pointer, and while it is the one selected a blue ring outside it (``PictureRing``). The selection changes
/// at once, with nothing moving or resizing. In a list it may draw no edge at all, on a quiet ground until it arrives.
/// The caller gives its size, at the picture's shape where it knows it. A picture that arrives fades in, or simply
/// appears under Reduce Motion.
struct PictureTile: View {
  /// The picture; nil shows black until it arrives.
  let picture: DesktopPicture?
  /// The frame's corners: a tile's 10 by default, a small thumbnail's 5.
  var radius = Radius.medium
  /// Whether it is the one selected, which its ring shows.
  var isSelected = false
  /// Whether the pointer is over it, which a brighter edge shows; nothing moves.
  var isHighlighted = false
  /// Whether it draws its fine edge, or its edge under the pointer; it does by default.
  var showsEdge = true
  /// What shows until the picture arrives, and beside it where the frame is not its shape; black by default.
  var ground = Color.letterbox
  @Environment(\.accessibilityReduceMotion) private var reduceMotion

  /// The picture in its frame, with its edge, and its ring outside it.
  var body: some View {
    ground
      .overlay {
        if let picture {
          Image(decorative: picture.image, scale: 1)
            .resizable()
            .interpolation(.high)
            .aspectRatio(contentMode: .fit)
            .transition(.opacity)
        }
      }
      .animation(Motion.animation(Motion.fade, reduceMotion: reduceMotion), value: picture)
      .clipShape(RoundedRectangle(cornerRadius: radius, style: .continuous))
      .overlay {
        if showsEdge {
          RoundedRectangle(cornerRadius: radius, style: .continuous)
            .strokeBorder(
              isHighlighted ? Color.strokeHover : Color.strokeStrong,
              lineWidth: isHighlighted ? Stroke.regular : Stroke.hairline
            )
        }
      }
      .overlay {
        if isSelected {
          RoundedRectangle(cornerRadius: PictureRing.outerRadius(around: radius), style: .continuous)
            .strokeBorder(Color.accent, lineWidth: PictureRing.width)
            .padding(-PictureRing.reach)
            .allowsHitTesting(false)
        }
      }
      // The ring comes and goes at once, whatever animation the change that selected it runs in.
      .transaction(value: isSelected) { $0.animation = nil }
  }
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

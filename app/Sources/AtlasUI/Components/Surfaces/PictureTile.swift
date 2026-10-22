import SwiftUI

/// A picture's thumbnail cut to its frame: the picture fills it edge to edge, on black until it arrives, with a fine
/// edge, or a ring of the ground and then blue around it while it is the one selected. The caller gives its size. A
/// picture that arrives fades in, or simply appears under Reduce Motion.
struct PictureTile: View {
  /// The picture; nil shows black until it arrives.
  let picture: DesktopPicture?
  /// The frame's corners: a tile's 10 by default, a small thumbnail's 5.
  var radius = Radius.medium
  /// Whether it is the one selected, which its ring shows.
  var isSelected = false
  @Environment(\.accessibilityReduceMotion) private var reduceMotion

  var body: some View {
    Color.stageBackground
      .overlay {
        if let picture {
          Image(decorative: picture.image, scale: 1)
            .resizable()
            .interpolation(.high)
            .aspectRatio(contentMode: .fill)
            .transition(reduceMotion ? .identity : .opacity)
        }
      }
      .animation(reduceMotion ? nil : Motion.fade, value: picture)
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

  /// The selected ring: a band of the ground, then a band of blue, outside the frame.
  private var ring: some View {
    ZStack {
      RoundedRectangle(cornerRadius: radius + Stroke.thick * 2)
        .fill(Color.accent)
        .padding(-Stroke.thick * 2)
      RoundedRectangle(cornerRadius: radius + Stroke.thick)
        .fill(Color.ground)
        .padding(-Stroke.thick)
    }
  }
}

#if DEBUG
  #Preview("Samples") {
    HStack(spacing: Spacing.xxl) {
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
    .padding(Spacing.m)
    .sampleSurface()
  }
#endif

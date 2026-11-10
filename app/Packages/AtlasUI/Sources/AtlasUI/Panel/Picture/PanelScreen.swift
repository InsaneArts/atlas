import SwiftUI

/// The screen in the panel's picture frame of its own shape, resting on the ground: letterboxed on black only when its
/// shape is past 4:3 or 16:9, greyed and darkened while dimmed, with a fine light inner edge and a soft shadow; the
/// empty stage until the first picture arrives.
struct PanelScreen: View {
  /// The screen; nil shows the empty stage.
  let picture: DesktopPicture?
  /// Whether the screen is greyed and darkened.
  let isDimmed: Bool

  /// The screen, or the empty stage, in its frame.
  var body: some View {
    ZStack {
      if let picture {
        Color.letterbox
        Image(decorative: picture.image, scale: 1)
          .resizable()
          .interpolation(.high)
          .aspectRatio(contentMode: .fit)
          .saturation(PictureTreatment.saturation(for: look))
          .overlay(Color.letterbox.opacity(PictureTreatment.shade(for: look)))
      } else {
        Color.fillPlaceholder
      }
    }
    .frame(width: size.width, height: size.height)
    .clipShape(shape)
    .overlay(
      shape
        .strokeBorder(Color.strokeHighlight, lineWidth: Stroke.hairline)
    )
    .elevation(.resting)
  }

  /// How the screen is drawn: greyed and darkened as a paused picture is while dimmed.
  private var look: PictureLook {
    isDimmed ? .paused : .live
  }

  /// The frame: the screen's own shape, so it fills the frame with no black at its sides.
  private var size: CGSize {
    PanelLayout.pictureSize(aspect: picture?.aspectRatio)
  }

  /// The frame's outline.
  private var shape: RoundedRectangle {
    RoundedRectangle(cornerRadius: PanelLayout.pictureCorner, style: .continuous)
  }
}

#if DEBUG
  #Preview("Live") {
    PanelScreen(picture: SampleDesktop.picture, isDimmed: false)
      .padding(Spacing.xxl)
      .sampleSurface()
  }

  #Preview("Dimmed") {
    PanelScreen(picture: SampleDesktop.picture, isDimmed: true)
      .padding(Spacing.xxl)
      .sampleSurface()
  }

  #Preview("Empty") {
    PanelScreen(picture: nil, isDimmed: false)
      .padding(Spacing.xxl)
      .sampleSurface()
  }
#endif

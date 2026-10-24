import SwiftUI

/// The screen in the panel's picture frame, resting on the ground (v3 P3): letterboxed on black, greyed and darkened
/// while dimmed, with a fine light inner edge and a soft shadow; the empty stage until the first picture arrives.
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
          .saturation(isDimmed ? PanelLayout.dimSaturation : 1)
          .overlay(Color.letterbox.opacity(isDimmed ? PanelLayout.dimShade : 0))
      } else {
        Color.fillPlaceholder
      }
    }
    .frame(width: PanelLayout.picture.width, height: PanelLayout.picture.height)
    .clipShape(shape)
    .overlay(
      shape
        .strokeBorder(Color.strokeHighlight, lineWidth: Stroke.hairline)
    )
    .elevation(.resting)
  }

  /// The frame's outline.
  private var shape: RoundedRectangle {
    RoundedRectangle(cornerRadius: Radius.medium)
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

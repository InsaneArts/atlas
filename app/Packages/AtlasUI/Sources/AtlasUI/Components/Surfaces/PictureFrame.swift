import SwiftUI

/// The helper's screen as one letterboxed picture on black, with its edge: a quiet hairline, or a blue rim and glow
/// while the person holds control. Dimmed while an alert is over it, faded while paused, and the empty stage while
/// there is no picture yet.
struct PictureFrame: View {
  /// How large the frame's corners are.
  enum Size: Sendable {
    /// 12: the main window's screen.
    case regular
    /// 10: the panel's picture and the mini screen.
    case small
  }

  /// The picture; nil shows the empty stage.
  let picture: DesktopPicture?
  /// How it is drawn.
  var look = PictureLook.live
  /// How large its corners are.
  var size = Size.regular

  var body: some View {
    ZStack {
      if let picture {
        Color.letterbox
        Image(decorative: picture.image, scale: 1)
          .resizable()
          .interpolation(.high)
          .aspectRatio(contentMode: .fit)
          .pictureTreatment(look)
      } else {
        Color.fillPlaceholder
      }
    }
    .clipShape(shape)
    .controlRim(look == .inControl, radius: size == .regular ? Radius.large : Radius.medium)
    .accessibilityElement()
    .accessibilityLabel("The helper's screen")
  }

  /// The frame's shape.
  private var shape: RoundedRectangle {
    RoundedRectangle(cornerRadius: size == .regular ? Radius.large : Radius.medium)
  }
}

#if DEBUG
  #Preview("Samples") {
    PictureFrameSamples()
  }
#endif

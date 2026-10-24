import SwiftUI

/// The helper's screen at one view point a display point, in a view that scrolls both ways, with the input surface
/// over the whole picture so a click maps to the same display point however far it is scrolled.
struct ActualSizeScreen<Input: View>: View {
  /// The picture.
  let picture: DesktopPicture
  /// The helper's display, in points: the picture's size here.
  let size: CGSize
  /// How the picture is drawn: live, dimmed, paused or inside control's rim.
  let look: PictureLook
  /// A window shown from the Apps tab, outlined over the picture; nil shows none.
  let outline: ViewerWindowOutline?
  /// The surface laid over the picture that takes the mouse and keyboard.
  let input: Input

  var body: some View {
    ScrollView([.horizontal, .vertical]) {
      ZStack {
        Image(decorative: picture.image, scale: 1)
          .resizable()
          .interpolation(.high)
          .pictureTreatment(look)
        if let outline {
          ScreenOutline(outline: outline, picture: CGSize(width: picture.image.width, height: picture.image.height))
        }
        input
      }
      .frame(width: size.width, height: size.height)
    }
    .background(Color.stageBackground)
    .clipShape(RoundedRectangle(cornerRadius: Radius.large))
    .controlRim(look == .inControl, radius: Radius.large)
  }
}

#if DEBUG
  #Preview("Actual size") {
    if let picture = SampleDesktop.picture {
      ActualSizeScreen(
        picture: picture,
        size: CGSize(width: picture.image.width, height: picture.image.height),
        look: .inControl,
        outline: nil,
        input: EmptyView()
      )
      .frame(width: SampleWidth.column * 2, height: SampleWidth.column)
      .sampleSurface()
    }
  }
#endif

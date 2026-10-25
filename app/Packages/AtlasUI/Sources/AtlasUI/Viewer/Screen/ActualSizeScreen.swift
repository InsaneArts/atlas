import SwiftUI

/// The helper's screen at one view point a display point, in a view that scrolls both ways, with the input surface
/// over the whole picture so a click maps to the same display point however far it is scrolled. Its corners are the
/// fitted screen's; the black that darkens a look is the picture's own.
struct ActualSizeScreen<Content: View>: View {
  /// The helper's display, in points: the picture's size here.
  let size: CGSize
  /// The picture, what is outlined on it and the input surface, stacked.
  @ViewBuilder let content: () -> Content

  /// The picture at one point a point, scrolling, in the screen's corners.
  var body: some View {
    ScrollView([.horizontal, .vertical]) {
      ZStack {
        content()
      }
      .frame(width: size.width, height: size.height)
    }
    .background(Color.letterbox)
    .clipShape(RoundedRectangle(cornerRadius: Radius.large))
  }
}

#if DEBUG
  #Preview("Actual size") {
    if let picture = SampleDesktop.picture {
      ActualSizeScreen(size: CGSize(width: picture.image.width, height: picture.image.height)) {
        StillPicture(ViewerLivePicture(picture: picture, saturation: 1, corner: Radius.large))
      }
      .frame(width: SampleWidth.column * 2, height: SampleWidth.column)
      .sampleSurface()
    }
  }
#endif

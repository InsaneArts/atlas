import SwiftUI

/// The live picture's slot drawn as an image: what previews and renders pass, and what the screen drew before
/// the app drew its picture through a layer. It fits the picture at its own shape and keeps the colour its
/// look keeps.
public struct StillPicture: View {
  /// What the slot draws.
  private let live: ViewerLivePicture

  /// Creates the image.
  ///
  /// - Parameter live: What the slot draws.
  public init(_ live: ViewerLivePicture) {
    self.live = live
  }

  /// The picture, fitted, in its look's colour.
  public var body: some View {
    Image(decorative: live.picture.image, scale: 1)
      .resizable()
      .interpolation(.high)
      .aspectRatio(contentMode: .fit)
      .saturation(live.saturation)
  }
}

#if DEBUG
  #Preview("Paused") {
    if let picture = SampleDesktop.picture {
      StillPicture(
        ViewerLivePicture(
          picture: picture,
          saturation: PictureTreatment.saturation(for: .paused),
          corner: ViewerLayout.screenRadius
        )
      )
      .frame(width: SampleWidth.column * 2)
      .sampleSurface()
    }
  }
#endif

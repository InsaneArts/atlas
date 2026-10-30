#if DEBUG
  import SwiftUI

  /// The mini screen's sample, over the sample desktop, at the size the main window gives it.
  struct MiniScreenSamples: View {
    /// The small screen live, then paused.
    var body: some View {
      Group {
        if let picture = SampleDesktop.picture {
          HStack(spacing: Spacing.xxl) {
            sample(MiniScreenState(picture: picture))
            sample(MiniScreenState(picture: picture, isPaused: true))
          }
        }
      }
      .padding(Spacing.xxl)
      .sampleSurface()
    }

    /// One mini screen, its picture drawn as the previews draw the live slot.
    ///
    /// - Parameter state: The picture, and whether it is paused.
    /// - Returns: The mini screen at the main window's size.
    private func sample(_ state: MiniScreenState) -> some View {
      let size = ViewerLayout.pipSize(bodyWidth: .infinity, aspect: state.picture.aspectRatio)
      return MiniScreen(state: state, corner: ViewerLayout.pipRadius) {
      } picture: {
        StillPicture(
          ViewerLivePicture(
            picture: state.picture,
            saturation: PictureTreatment.saturation(for: state.look),
            corner: ViewerLayout.pipRadius
          )
        )
      }
      .frame(width: size.width, height: size.height)
    }
  }
#endif

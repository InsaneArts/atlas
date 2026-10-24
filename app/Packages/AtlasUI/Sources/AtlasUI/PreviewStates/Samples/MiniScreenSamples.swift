#if DEBUG
  import SwiftUI

  /// The mini screen's sample, over the sample desktop.
  struct MiniScreenSamples: View {
    var body: some View {
      Group {
        if let picture = SampleDesktop.picture {
          HStack(spacing: Spacing.xxl) {
            MiniScreen(state: MiniScreenState(picture: picture)) {}
            MiniScreen(state: MiniScreenState(picture: picture, isPaused: true)) {}
          }
        }
      }
      .padding(Spacing.xxl)
      .sampleSurface()
    }
  }
#endif

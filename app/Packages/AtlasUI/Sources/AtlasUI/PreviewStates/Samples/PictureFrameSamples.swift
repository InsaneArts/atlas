#if DEBUG
  import SwiftUI

  /// The picture frame's samples: each look, then the empty stage, at the panel's small corners.
  struct PictureFrameSamples: View {
    var body: some View {
      Grid(horizontalSpacing: Spacing.m, verticalSpacing: Spacing.m) {
        GridRow {
          PictureFrame(picture: SampleDesktop.picture)
          PictureFrame(picture: SampleDesktop.picture, look: .inControl)
        }
        GridRow {
          PictureFrame(picture: SampleDesktop.picture, look: .dimmed)
          PictureFrame(picture: SampleDesktop.picture, look: .paused)
        }
        GridRow {
          PictureFrame(picture: nil, size: .small)
          PictureFrame(picture: SampleDesktop.picture, size: .small)
        }
      }
      .frame(width: SampleWidth.column * 2, height: SampleWidth.column * 2)
      .sampleSurface()
    }
  }
#endif

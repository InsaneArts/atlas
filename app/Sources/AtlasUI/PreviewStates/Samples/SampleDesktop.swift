#if DEBUG
  import SwiftUI

  /// A stand-in desktop for the panel's and the viewer's samples: a window on a blue ground, drawn once at a
  /// display's size, one pixel a point. Its colours, sizes and words are a sample desktop's, not the app's, so they
  /// are not tokens.
  package enum SampleDesktop {
    /// The desktop as a picture, or nil should drawing it fail.
    @MainActor package static let picture: DesktopPicture? = {
      let renderer = ImageRenderer(content: SampleDesktopScene())
      renderer.scale = 1
      return renderer.cgImage.map { DesktopPicture(id: "sample-desktop", image: $0) }
    }()
  }
#endif

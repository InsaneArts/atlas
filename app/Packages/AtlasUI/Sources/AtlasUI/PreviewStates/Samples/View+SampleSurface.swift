#if DEBUG
  import SwiftUI

  extension View {
    /// Sets a sample on the ground the windows use, in their text colour, light or dark as the preview is, with room
    /// around it.
    ///
    /// - Returns: The sample on its ground.
    func sampleSurface() -> some View {
      padding(Spacing.l)
        .windowSurface()
    }
  }
#endif

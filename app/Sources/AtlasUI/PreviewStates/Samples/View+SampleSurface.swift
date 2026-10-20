#if DEBUG
  import SwiftUI

  extension View {
    /// Sets a sample on the ground the windows use, in their dark appearance and text colour, with room around it.
    ///
    /// - Returns: The sample on its ground.
    func sampleSurface() -> some View {
      padding(Spacing.xxl)
        .windowSurface()
    }
  }
#endif

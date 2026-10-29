import SwiftUI

extension View {
  /// Sets a window's content on the ground every window draws on, in its text colour, both in the Mac's appearance.
  ///
  /// - Returns: The content on the window's ground.
  func windowSurface() -> some View {
    background(Color.ground)
      .foregroundStyle(Color.textPrimary)
  }
}

#if DEBUG
  #Preview("Samples") {
    ModifierSamples(.windowSurface)
  }
#endif

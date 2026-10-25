import SwiftUI

extension View {
  /// Sets a window's content on the dark ground every window draws on, in its dark appearance and text colour.
  ///
  /// - Returns: The content on the window's ground.
  func windowSurface() -> some View {
    background(Color.ground)
      .foregroundStyle(Color.textPrimary)
      .environment(\.colorScheme, .dark)
  }
}

#if DEBUG
  #Preview("Samples") {
    ModifierSamples(.windowSurface)
  }
#endif

#if DEBUG
  import SwiftUI

  extension View {
    /// Sets a setup window sample at its window's size, on the ground every window draws on.
    ///
    /// - Parameter size: The window's size: the card's by default, or the permission panel's.
    /// - Returns: The sample in its window.
    func onboardingSample(_ size: CGSize = OnboardingLayout.card) -> some View {
      frame(width: size.width, height: size.height)
        .windowSurface()
    }
  }
#endif

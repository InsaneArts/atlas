import SwiftUI

extension View {
  /// Shows the view only after ``Motion/appear``'s delay, fading it in, so a quick answer never flashes it (v3 R12).
  ///
  /// - Returns: The view, held back for its delay.
  func appearsAfterDelay() -> some View {
    modifier(DelayedAppearance())
  }
}

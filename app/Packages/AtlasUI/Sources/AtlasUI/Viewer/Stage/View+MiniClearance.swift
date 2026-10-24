import SwiftUI

extension View {
  /// Keeps a scroll view's content clear of the small screen in the stage's corner, as its pane is told
  /// (``EnvironmentValues/miniClearance``).
  ///
  /// - Returns: The scroll view, its content margined at the end the small screen covers.
  func clearsMiniScreen() -> some View {
    modifier(MiniClearing())
  }
}

import SwiftUI

extension View {
  /// Makes the view the helper's screen that moves between the stage and the corner: the Screen pane's fitted picture
  /// or the mini screen.
  ///
  /// - Parameter namespace: The namespace the picture moves in, the window's.
  /// - Returns: The view, carrying the screen's identity.
  func screenZoom(in namespace: Namespace.ID) -> some View {
    modifier(ScreenZoom(namespace: namespace))
  }
}

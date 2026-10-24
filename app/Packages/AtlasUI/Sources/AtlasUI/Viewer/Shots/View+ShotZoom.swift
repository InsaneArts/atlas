import SwiftUI

extension View {
  /// Makes the view a screenshot's picture that zooms between its tile and the large view.
  ///
  /// - Parameters:
  ///   - id: The screenshot's file name.
  ///   - namespace: The namespace the picture moves in, the window's.
  /// - Returns: The view, carrying the screenshot's identity.
  func shotZoom(id: String, in namespace: Namespace.ID) -> some View {
    modifier(ShotZoom(id: id, namespace: namespace))
  }
}

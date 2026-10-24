import SwiftUI

extension View {
  /// Draws a picture of the helper's screen in its look: dimmed, paused, or as it is.
  ///
  /// - Parameter look: How the picture is drawn.
  /// - Returns: The picture, treated.
  func pictureTreatment(_ look: PictureLook) -> some View {
    modifier(PictureTreatment(look: look))
  }
}

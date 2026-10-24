import SwiftUI

/// How a picture of the helper's screen is drawn for its look: greyed and darkened while dimmed, faded while paused,
/// as it is otherwise. The fitted frame and the screen at actual size share it, so neither drops a treatment.
struct PictureTreatment: ViewModifier {
  /// How the picture is drawn.
  let look: PictureLook

  /// Draws the picture in its look.
  ///
  /// - Parameter content: The picture.
  /// - Returns: The picture, treated.
  func body(content: Content) -> some View {
    content
      .saturation(look == .dimmed ? Layout.dimSaturation : 1)
      .overlay(Color.letterbox.opacity(look == .dimmed ? Layout.dimShade : 0))
      .opacity(look == .paused ? Opacity.dimmed : 1)
  }
}

/// The treatment's own measures, from the boards (v3.py `stage`).
private enum Layout {
  /// 0.55: how much colour a dimmed picture keeps.
  static let dimSaturation = 0.55
  /// 0.4: the black over a dimmed picture, which leaves it at the board's 0.6 brightness.
  static let dimShade = 0.4
}

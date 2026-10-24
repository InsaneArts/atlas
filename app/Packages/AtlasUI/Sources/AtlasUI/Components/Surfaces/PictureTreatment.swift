import SwiftUI

/// How a picture of the helper's screen is drawn for its look: greyed and darkened while dimmed, a little
/// less while paused, as it is otherwise. The fitted frame, the screen at actual size and the live picture's
/// layer share its two numbers, so none drops a treatment.
struct PictureTreatment: ViewModifier {
  /// How the picture is drawn.
  let look: PictureLook

  /// Draws the picture in its look.
  ///
  /// - Parameter content: The picture.
  /// - Returns: The picture, treated.
  func body(content: Content) -> some View {
    content
      .saturation(Self.saturation(for: look))
      .overlay(
        Color.letterbox
          .opacity(Self.shade(for: look))
      )
  }

  /// How much colour a picture keeps in `look`.
  ///
  /// - Parameter look: How the picture is drawn.
  /// - Returns: The saturation, 1 for all of it.
  nonisolated static func saturation(for look: PictureLook) -> Double {
    switch look {
    case .dimmed: Layout.dimSaturation
    case .paused: Layout.pausedSaturation
    case .live, .inControl: 1
    }
  }

  /// How much black lies over a picture in `look`, which darkens it.
  ///
  /// - Parameter look: How the picture is drawn.
  /// - Returns: The black's opacity, 0 for none.
  nonisolated static func shade(for look: PictureLook) -> Double {
    switch look {
    case .dimmed: Layout.dimShade
    case .paused: Layout.pausedShade
    case .live, .inControl: 0
    }
  }
}

/// The treatment's own measures for dimmed and paused pictures.
private enum Layout {
  /// 0.55: how much colour a dimmed picture keeps.
  static let dimSaturation = 0.55
  /// 0.4: the black over a dimmed picture, which leaves it at 0.6 brightness.
  static let dimShade = 0.4
  /// 0.5: how much colour a paused picture keeps.
  static let pausedSaturation = 0.5
  /// 0.3: the black over a paused picture, which leaves it at 0.7 brightness.
  static let pausedShade = 0.3
}

#if DEBUG
  #Preview("Looks") {
    ModifierSamples(.pictureTreatment)
  }
#endif

/// How a picture of the helper's screen is drawn for its look: greyed and darkened while dimmed, a little
/// less while paused, as it is otherwise. Every picture of the screen, the panel's, the mini screen's and the
/// live picture's layer among them, shares its two numbers, so none drops a treatment.
enum PictureTreatment {
  /// How much colour a picture keeps in `look`.
  ///
  /// - Parameter look: How the picture is drawn.
  /// - Returns: The saturation, 1 for all of it.
  nonisolated static func saturation(for look: PictureLook) -> Double {
    switch look {
    case .dimmed: PictureTreatmentLayout.dimSaturation
    case .paused: PictureTreatmentLayout.pausedSaturation
    case .live, .inControl: 1
    }
  }

  /// How much black lies over a picture in `look`, which darkens it.
  ///
  /// - Parameter look: How the picture is drawn.
  /// - Returns: The black's opacity, 0 for none.
  nonisolated static func shade(for look: PictureLook) -> Double {
    switch look {
    case .dimmed: PictureTreatmentLayout.dimShade
    case .paused: PictureTreatmentLayout.pausedShade
    case .live, .inControl: 0
    }
  }
}
